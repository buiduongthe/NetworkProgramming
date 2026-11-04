import java.io.DataInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.lang.management.ManagementFactory;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 6.2 – Server tra cứu "bất đồng bộ": mỗi kết nối một tác vụ (Mã giả 6.3, Mục 6.2.3).
 * Java cài đặt theo hướng của Mục 5.2.3: mỗi tác vụ là một luồng ảo (virtual thread, JDK 21) viết mã
 * chặn thông thường; khi chờ vào/ra, luồng ảo được tháo khỏi luồng hệ điều hành nên không chiếm luồng nào.
 * Chạy: java -cp out AsyncServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class AsyncServer {
    static final AtomicInteger TASKS = new AtomicInteger();             // số tác vụ phiên đang chạy

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {                  // như Mã giả 6.1
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 128);
            System.out.println("[server] Server bất đồng bộ đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT_ASYNC
                String addr = NetUtil.addr(conn);
                int n = TASKS.incrementAndGet();
                Thread.ofVirtual().start(() -> serveAsync(conn, addr)); // RUN_BACKGROUND: không chờ
                int threads = ManagementFactory.getThreadMXBean().getThreadCount();   // luồng hệ điều hành
                System.out.println("[server] Nhận " + addr + " (tác vụ: " + n + ", luồng: " + threads + ")");
            }
        }
    }

    /** SERVE_ASYNC: vòng yêu cầu–phản hồi chạy trên luồng ảo. */
    static void serveAsync(Socket conn, String addr) {
        String reason;
        try (conn) {                                                    // FINALLY CLOSE(conn)
            conn.setSoTimeout(300000);                                  // WITH_TIMEOUT(300000, …)
            DataInputStream in = new DataInputStream(conn.getInputStream());
            OutputStream out = conn.getOutputStream();
            while (true) {
                Map<String, Object> req = NetUtil.recvJson(in);
                NetUtil.sendJson(out, LookupService.processRequest(req));
            }
        } catch (NetUtil.ConnectionClosed e) {
            reason = e.received == 0 ? "client đóng kết nối" : e.getMessage();
        } catch (SocketTimeoutException e) {                            // Timeout
            reason = "quá thời gian chờ nhận";
        } catch (NetUtil.ProtocolError | Json.JsonError e) {
            reason = "lỗi giao thức: " + e.getMessage();
        } catch (IOException e) {                                       // NetError
            reason = "lỗi mạng: " + e.getMessage();
        } finally {
            TASKS.decrementAndGet();
        }
        System.out.println("[server] Kết thúc " + addr + ": " + reason);
    }
}
