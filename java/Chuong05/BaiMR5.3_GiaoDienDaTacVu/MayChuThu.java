import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.Executors;

/**
 * Bài mở rộng MR5.3 – Máy chủ thử cục bộ cho ứng dụng giao diện đa tác vụ (chỉ chạy trong máy).
 *
 * Một tiến trình cung cấp ba dịch vụ giả lập:
 *  - HTTP  <port>/tcp : GET /tep?kb=<số KB>&tre=<ms> – gửi tệp <kb> KB theo từng khối 16 KB,
 *                       nghỉ <tre> ms sau mỗi khối (giả lập đường truyền chậm, độ trễ khác nhau);
 *  - "DNS" <port>/udp : nhận "A <tên>", trả "<tên> <địa chỉ>" hoặc "<tên> NXDOMAIN" sau 100–400 ms
 *                       (bảng tên giả lập, không phải giao thức DNS thật – xem Bài 4.3);
 *  - cổng mở <port+1>, <port+2>/tcp: chỉ nhận rồi đóng kết nối, để ứng dụng kiểm tra cổng.
 *
 * Chạy: java -cp out MayChuThu <host> <port>
 */
public class MayChuThu {
    static final int CHUNK = 16 * 1024;
    record Entry(String ip, int ms) {}
    static final Map<String, Entry> NAMES = Map.of("router.lab", new Entry("10.10.0.1", 100), "may-in.lab", new Entry("10.10.0.21", 250),
            "camera-1.lab", new Entry("10.10.0.31", 400), "nas.lab", new Entry("10.10.0.40", 150));

    static synchronized void log(String text) { System.out.println("[server] " + text); }   // in nguyên dòng một lần

    static Map<String, String> query(String q) {
        Map<String, String> m = new HashMap<>();
        if (q != null) for (String kv : q.split("&")) {
            int i = kv.indexOf('=');
            if (i > 0) m.put(kv.substring(0, i), kv.substring(i + 1));
        }
        return m;
    }

    static void handleFile(HttpExchange ex) throws IOException {
        if (!ex.getRequestURI().getPath().equals("/tep")) {
            ex.sendResponseHeaders(404, -1);
            ex.close();
            return;
        }
        Map<String, String> q = query(ex.getRequestURI().getRawQuery());
        int kb = Integer.parseInt(q.getOrDefault("kb", "256")), delay = Integer.parseInt(q.getOrDefault("tre", "50"));
        long size = kb * 1024L;
        log("HTTP GET /tep " + kb + " KB, nghỉ " + delay + " ms mỗi khối");
        ex.getResponseHeaders().set("Content-Type", "application/octet-stream");
        ex.sendResponseHeaders(200, size);                         // có Content-Length
        long sent = 0;
        byte[] block = new byte[CHUNK];
        Arrays.fill(block, (byte) 'x');
        try (OutputStream out = ex.getResponseBody()) {
            while (sent < size) {
                int n = (int) Math.min(CHUNK, size - sent);
                out.write(block, 0, n);
                out.flush();
                sent += n;
                if (sent < size) Thread.sleep(delay);            // khối cuối: không cần nghỉ
            }
        } catch (IOException e) {
            log("HTTP: client đóng kết nối giữa chừng (" + (sent / 1024) + "/" + kb + " KB)");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    static void dnsServer(String host, int port) throws IOException {
        DatagramSocket s = new DatagramSocket(new InetSocketAddress(host, port));
        byte[] buf = new byte[512];
        while (true) {
            DatagramPacket p = new DatagramPacket(buf, buf.length);
            s.receive(p);
            String text = new String(p.getData(), 0, p.getLength(), StandardCharsets.UTF_8);
            String name = (text.startsWith("A ") ? text.substring(2) : text).strip();
            SocketAddress addr = p.getSocketAddress();
            Thread.ofVirtual().start(() -> {                      // trả lời sau một độ trễ, không chặn vòng nhận
                Entry e = NAMES.getOrDefault(name, new Entry("NXDOMAIN", 300));
                try {
                    Thread.sleep(e.ms());
                    byte[] ans = (name + " " + e.ip()).getBytes(StandardCharsets.UTF_8);
                    s.send(new DatagramPacket(ans, ans.length, addr));
                } catch (IOException | InterruptedException ignored) { }
            });
        }
    }

    static void openPort(String host, int port) throws IOException {
        ServerSocket ls = new ServerSocket();
        ls.setReuseAddress(true);
        ls.bind(new InetSocketAddress(host, port));
        while (true) {
            Socket c = ls.accept();
            c.close();
        }
    }

    static Thread daemon(Runnable r) {
        Thread t = new Thread(r);
        t.setDaemon(true);
        t.start();
        return t;
    }

    public static void main(String[] args) throws IOException {
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        HttpServer httpd = HttpServer.create(new InetSocketAddress(host, port), 32);
        httpd.createContext("/", MayChuThu::handleFile);
        httpd.setExecutor(Executors.newVirtualThreadPerTaskExecutor());   // mỗi yêu cầu một luồng ảo
        daemon(() -> { try { dnsServer(host, port); } catch (IOException e) { log("DNS lỗi: " + e.getMessage()); } });
        for (int p : new int[] {port + 1, port + 2})
            daemon(() -> { try { openPort(host, p); } catch (IOException e) { log("cổng " + p + " lỗi: " + e.getMessage()); } });
        Runtime.getRuntime().addShutdownHook(new Thread(() -> log("dừng")));
        httpd.start();
        log("HTTP " + host + ":" + port + "/tcp, \"DNS\" " + host + ":" + port + "/udp, cổng mở " + (port + 1) + ", " + (port + 2));
    }
}
