import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP (TCP port forwarder).
 *
 * Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (vòng đời socket TCP, SEND_ALL,
 * SHUTDOWN một chiều) và Chương 6 (mỗi kết nối một luồng).
 * Mỗi client nối vào cổng nghe được ghép với một kết nối mới tới máy đích; hai luồng
 * bơm dữ liệu theo hai chiều. Khi một chiều gặp EOF, bộ chuyển tiếp gọi SHUTDOWN(WRITE)
 * (Socket.shutdownOutput) trên socket bên kia để báo "hết dữ liệu" (chuyển tiếp gói FIN), chứ
 * không đóng hẳn, vì chiều ngược lại có thể vẫn đang truyền.
 * Đây là nguyên lý của "ssh -L", proxy TCP và cân bằng tải lớp 4.
 *
 * Chạy: java -cp out ChuyenTiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>
 */
public class ChuyenTiep {
    static final AtomicInteger counter = new AtomicInteger();

    /** Chép dữ liệu src -> dst tới khi src hết dữ liệu, rồi báo EOF cho dst; trả về số byte đã chép. */
    static long pump(Socket src, Socket dst, String label) {
        long total = 0;
        try {
            InputStream in = src.getInputStream();
            OutputStream out = dst.getOutputStream();
            byte[] buf = new byte[65536];
            int n;
            while ((n = in.read(buf)) != -1) {          // -1: phía bên kia đã đóng chiều gửi (EOF)
                out.write(buf, 0, n);                    // SEND_ALL: write() gửi hết mảng
                total += n;
            }
        } catch (IOException e) {
            System.out.println("[chuyen-tiep] " + label + ": lỗi " + e.getMessage());
        } finally {
            try {
                dst.shutdownOutput();                    // chuyển tiếp FIN, không đóng hẳn socket
            } catch (IOException ignored) { }
        }
        return total;
    }

    static void serve(Socket client, String thost, int tport) {
        int cid = counter.incrementAndGet();
        String addr = client.getInetAddress().getHostAddress() + ":" + client.getPort();
        Socket upstream = new Socket();
        try {
            upstream.connect(new InetSocketAddress(thost, tport), 3000);   // thời gian chờ kết nối 3 s
        } catch (IOException e) {
            System.out.println("[chuyen-tiep] #" + cid + " " + addr + ": không nối được đích "
                    + thost + ":" + tport + " (" + e.getMessage() + ")");
            closeQuietly(client);                        // client nhận EOF (hoặc RST) ngay
            return;
        }
        System.out.println("[chuyen-tiep] #" + cid + " " + addr + " <-> đích " + thost + ":" + tport);
        AtomicLong up = new AtomicLong(), down = new AtomicLong();
        Thread t1 = new Thread(() -> up.set(pump(client, upstream, "#" + cid + " lên")));
        Thread t2 = new Thread(() -> down.set(pump(upstream, client, "#" + cid + " xuống")));
        t1.start(); t2.start();
        try {
            t1.join(); t2.join();                        // chờ cả hai chiều kết thúc
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        closeQuietly(client);
        closeQuietly(upstream);
        System.out.println("[chuyen-tiep] #" + cid + " xong: lên " + up.get() + " byte, xuống " + down.get() + " byte");
    }

    static void closeQuietly(Socket s) {
        try { s.close(); } catch (IOException ignored) { }
    }

    public static void main(String[] args) throws IOException {
        if (args.length < 4) {
            System.err.println("Cách dùng: java -cp out ChuyenTiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>");
            System.exit(1);
        }
        String lhost = args[0], thost = args[2];
        int lport = Integer.parseInt(args[1]), tport = Integer.parseInt(args[3]);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[chuyen-tiep] Dừng")));  // Ctrl+C
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(lhost, lport));
            System.out.println("[chuyen-tiep] " + lhost + ":" + lport + " -> " + thost + ":" + tport);
            while (true) {
                Socket client = srv.accept();
                Thread t = new Thread(() -> serve(client, thost, tport));
                t.setDaemon(true);
                t.start();
            }
        }
    }
}
