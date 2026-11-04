import java.io.IOException;
import java.net.ConnectException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.NoRouteToHostException;
import java.net.Socket;
import java.net.SocketException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;

/**
 * Bài 2.2 – Kết nối có thời gian chờ và phân loại lỗi (Mã giả 2.2 SAFE_CONNECT, Mục 2.2.3).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out SafeConnect [host] [port] [timeout_ms]   (mặc định 127.0.0.1 9000 3000)
 */
public class SafeConnect {

    /** Mã giả 2.2 – SAFE_CONNECT(host, port, timeout_ms). */
    static Socket safeConnect(String host, int port, int timeoutMs) throws NetError, IOException {
        InetAddress[] addrs;
        try {
            addrs = InetAddress.getAllByName(host);                     // dòng 2–6: RESOLVE
        } catch (UnknownHostException e) {                              // tên miền không tồn tại
            throw new NetError("NOT_FOUND", false, e);
        }
        int refused = 0;
        Exception lastError = null;                                     // dòng 7
        for (InetAddress a : addrs) {                                   // dòng 8
            Socket s = new Socket();                                    // dòng 9: họ địa chỉ chọn theo a khi connect
            boolean ok = false;
            try {
                s.connect(new InetSocketAddress(a, port), timeoutMs);   // dòng 10–11: CONNECT có CONNECT_TIMEOUT
                ok = true;
                return s;                                               // thành công: trả socket
            } catch (ConnectException e) {                              // dòng 14–15: nhận gói RST
                refused++;
                lastError = e;
            } catch (SocketTimeoutException | NoRouteToHostException e) { // dòng 16–17: Timeout, HostUnreachable
                lastError = e;
            } catch (SocketException e) {                               // NetworkUnreachable: Java không có lớp riêng
                lastError = e;
            } finally {                                                 // dòng 18–20 (IOException khác: chuyển lên nơi gọi)
                if (!ok) s.close();                                     // chỉ đóng khi thất bại
            }
        }
        if (refused > 0 && refused == addrs.length)                     // dòng 22–24
            throw new NetError("REFUSED", false, lastError);
        throw new NetError("UNREACHABLE", true, lastError);             // dòng 25
    }

    public static void main(String[] args) throws Exception {
        String host;
        int port, timeoutMs;
        try {
            Object[] ep = Endpoint.parse(args, "127.0.0.1", 9000);
            host = (String) ep[0];
            port = (Integer) ep[1];
            timeoutMs = args.length >= 3 ? Integer.parseInt(args[2]) : 3000;
        } catch (ConfigError | NumberFormatException e) {
            System.out.println("[client] Lỗi cấu hình: " + e.getMessage());
            System.exit(2);
            return;
        }
        System.out.println("[client] Kết nối tới " + host + ":" + port + ", thời gian chờ " + timeoutMs + " ms");
        long t0 = System.nanoTime();
        Socket s;
        try {
            s = safeConnect(host, port, timeoutMs);
        } catch (NetError e) {
            long ms = (System.nanoTime() - t0) / 1_000_000;
            System.out.println("[client] NetError " + e.kind + ", thử lại: " + (e.retry ? "có" : "không")
                    + ", sau " + ms + " ms");
            Throwable c = e.getCause();
            System.out.println("[client] Nguyên nhân: " + c.getClass().getSimpleName() + ": " + c.getMessage());
            System.exit(1);
            return;
        }
        long ms = (System.nanoTime() - t0) / 1_000_000;
        try (s) {                                                       // nơi gọi chịu trách nhiệm đóng socket
            System.out.println("[client] Thành công: " + s.getLocalAddress().getHostAddress() + ":" + s.getLocalPort()
                    + " -> " + s.getInetAddress().getHostAddress() + ":" + s.getPort() + ", sau " + ms + " ms");
        }
        System.out.println("[client] Đã đóng kết nối");
    }
}
