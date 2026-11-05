import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài mở rộng MR6.1 – Tắt server an toàn (graceful shutdown): mỗi kết nối một luồng ảo + shutdown hook.
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng) và Bài 5.6 (hạn chót, hủy).
 * Giao thức dòng văn bản: client gửi "LAM <ms>", server mô phỏng công việc dài <ms> mili giây rồi trả "XONG <ms>".
 * Khi nhận SIGTERM (lệnh kill, trình quản lý dịch vụ, Docker/Kubernetes) hoặc SIGINT (Ctrl+C), JVM chạy
 * shutdown hook; hook thực hiện:
 *  1. Đóng socket nghe: ngừng nhận kết nối mới (client mới bị từ chối).
 *  2. Kết nối đang rỗi: gửi "BYE" rồi đóng ngay.
 *  3. Kết nối đang làm dở: cho làm xong yêu cầu hiện tại, trả kết quả, gửi "BYE", đóng.
 *  4. Quá hạn chờ (--han-cho-ms): ngắt (interrupt) các yêu cầu còn dở, báo "HUY" cho client.
 * PID được ghi vào server.pid để có thể gửi tín hiệu bằng: kill -TERM $(cat server.pid)
 *
 * Chạy: java -cp out ServerTatAnToan [host] [port] [--han-cho-ms 2000]
 */
public class ServerTatAnToan {
    static final AtomicInteger counter = new AtomicInteger();
    static volatile boolean stopping = false;                  // được đặt khi nhận tín hiệu dừng
    static final Map<Integer, Conn> connections = new ConcurrentHashMap<>();   // mã kết nối -> trạng thái
    static final AtomicInteger xong = new AtomicInteger(), huy = new AtomicInteger();

    /** Trạng thái một kết nối; mọi thay đổi busy/closed đều trong khối synchronized(this). */
    static final class Conn {
        final int cid;
        final Socket sock;
        Thread thread;
        boolean busy, closed;
        Integer job;
        Conn(int cid, Socket sock) { this.cid = cid; this.sock = sock; }

        void send(String text) {
            try {
                OutputStream out = sock.getOutputStream();
                out.write(text.getBytes(StandardCharsets.UTF_8));
            } catch (IOException ignored) { }
        }

        void close() {
            closed = true;
            try { sock.close(); } catch (IOException ignored) { }
        }
    }

    static synchronized void log(String text) { System.out.println("[server] " + text); }

    static void handle(Conn c) {
        log("#" + c.cid + " kết nối từ cổng " + c.sock.getPort());
        try {
            BufferedReader in = new BufferedReader(new InputStreamReader(c.sock.getInputStream(), StandardCharsets.UTF_8));
            while (true) {
                String line;
                try {
                    line = in.readLine();                              // chờ lệnh kế tiếp (luồng ảo: chặn rẻ)
                } catch (IOException e) {
                    line = null;
                }
                synchronized (c) {
                    if (c.closed) return;                              // hook đã gửi BYE và đóng khi ta đang rỗi
                    if (line == null) {                                // client đã đóng
                        log("#" + c.cid + " client đóng kết nối");
                        c.close();
                        return;
                    }
                    String[] parts = line.strip().split("\\s+");
                    if (parts.length != 2 || !parts[0].equals("LAM") || !parts[1].matches("\\d+")) {
                        c.send("LOI lệnh không hợp lệ\n");
                        continue;
                    }
                    c.job = Integer.parseInt(parts[1]);
                    c.busy = true;
                }
                log("#" + c.cid + " bắt đầu LAM " + c.job);
                try {
                    Thread.sleep(c.job);                               // công việc dài (có thể bị ngắt khi quá hạn)
                } catch (InterruptedException e) {
                    huy.incrementAndGet();
                    log("#" + c.cid + " quá hạn chờ -> hủy LAM " + c.job);
                    synchronized (c) {
                        c.send("HUY yêu cầu bị hủy vì server tắt\n");
                        c.close();
                    }
                    return;
                }
                synchronized (c) {
                    c.send("XONG " + c.job + "\n");
                    xong.incrementAndGet();
                    if (stopping) {                                    // làm xong việc dở thì mới chào
                        c.send("BYE server đang tắt\n");
                        log("#" + c.cid + " XONG " + c.job + " -> gửi BYE, đóng");
                        c.close();
                        return;
                    }
                    log("#" + c.cid + " XONG " + c.job);
                    c.job = null;
                    c.busy = false;
                }
            }
        } catch (IOException e) {
            synchronized (c) { c.close(); }
        } finally {
            connections.remove(c.cid);
        }
    }

    /** Quy trình tắt, chạy trong shutdown hook (JVM gọi khi nhận SIGTERM/SIGINT). */
    static void shutdown(ServerSocket srv, int hanChoMs) {
        long t0 = System.nanoTime();
        stopping = true;
        try { srv.close(); } catch (IOException ignored) { }      // 1. ngừng nhận kết nối mới
        log("Nhận tín hiệu dừng (SIGTERM/SIGINT): ngừng nhận kết nối mới");
        List<Conn> pending = new ArrayList<>(connections.values());
        pending.sort((a, b) -> Integer.compare(a.cid, b.cid));
        log("Chờ tối đa " + hanChoMs + " ms cho " + pending.size() + " kết nối");
        for (Conn c : pending) {
            synchronized (c) {                                     // 2. đang rỗi: chào và đóng ngay
                if (!c.busy && !c.closed) {
                    c.send("BYE server đang tắt\n");
                    log("#" + c.cid + " đang rỗi -> gửi BYE, đóng");
                    c.close();
                }
            }
        }
        long deadline = t0 + hanChoMs * 1_000_000L;               // 3. chờ việc dở, tối đa tới hạn chót
        List<Conn> still = new ArrayList<>();
        for (Conn c : pending) {
            long left = (deadline - System.nanoTime()) / 1_000_000;
            try {
                if (left > 0) c.thread.join(left);
            } catch (InterruptedException ignored) { }
            if (c.thread.isAlive()) still.add(c);
        }
        if (!still.isEmpty()) {                                    // 4. quá hạn: hủy phần còn lại
            log("Hết hạn chờ: hủy " + still.size() + " kết nối");
            for (Conn c : still) c.thread.interrupt();
            for (Conn c : still) {
                try { c.thread.join(); } catch (InterruptedException ignored) { }
            }
        }
        try { Files.deleteIfExists(Path.of("server.pid")); } catch (IOException ignored) { }
        log(String.format(Locale.ROOT, "Đã tắt sau %.1f s: xong %d yêu cầu, hủy %d", (System.nanoTime() - t0) / 1e9, xong.get(), huy.get()));
    }

    public static void main(String[] args) throws IOException {
        List<String> pos = new ArrayList<>();
        int hanCho = 2000;
        for (int i = 0; i < args.length; i++) {
            if (args[i].equals("--han-cho-ms")) hanCho = Integer.parseInt(args[++i]);
            else pos.add(args[i]);
        }
        String host = pos.size() > 0 ? pos.get(0) : "127.0.0.1";
        int port = pos.size() > 1 ? Integer.parseInt(pos.get(1)) : 9000;
        ServerSocket srv = new ServerSocket();
        srv.setReuseAddress(true);
        srv.bind(new InetSocketAddress(host, port), 16);
        int han = hanCho;
        Runtime.getRuntime().addShutdownHook(new Thread(() -> shutdown(srv, han)));
        Files.writeString(Path.of("server.pid"), String.valueOf(ProcessHandle.current().pid()));
        log("Lắng nghe tại " + host + ":" + port + ", PID ghi vào server.pid");
        while (!stopping) {
            Socket s;
            try {
                s = srv.accept();
            } catch (IOException e) {
                break;                                             // socket nghe đã đóng khi tắt
            }
            Conn c = new Conn(counter.incrementAndGet(), s);
            connections.put(c.cid, c);
            c.thread = Thread.ofVirtual().unstarted(() -> handle(c));
            c.thread.start();
        }
        // Luồng main dừng ở đây; JVM chờ shutdown hook làm xong rồi mới thoát.
        try { Thread.sleep(Long.MAX_VALUE); } catch (InterruptedException ignored) { }
    }
}
