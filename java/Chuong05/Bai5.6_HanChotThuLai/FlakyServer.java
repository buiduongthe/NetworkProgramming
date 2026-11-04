import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 5.6 – Server tra cứu "khó tính" để chạy thử hạn chót và thử lại (khung JSON của Mã giả 5.1):
 *   yêu cầu #1: trả lời chậm 2000 ms (vượt hạn chót của client);
 *   yêu cầu #2: trả lời {status: 503, retry_after_ms: 300} – quá tải tạm thời;
 *   từ yêu cầu #3: trả lời bình thường; trường delay_ms (nếu có) là thời gian xử lý giả lập.
 * Chạy: java -cp out FlakyServer [host] [port]      (mặc định 0.0.0.0 9100)
 */
public class FlakyServer {
    static final Map<String, String> DICT = Map.of("TCP", "Giao thức truyền tin cậy, hướng kết nối",
            "UDP", "Giao thức datagram, không kết nối");
    static final AtomicInteger COUNT = new AtomicInteger();          // số yêu cầu đã nhận
    static final CompletableFuture<Void> NO_CANCEL = Khung.newCancellation();

    static Map<String, Object> lookup(Object term) {
        return DICT.containsKey(term) ? Json.obj("status", 200, "term", term, "text", DICT.get(term))
                : Json.obj("status", 404, "error", "Không có thuật ngữ");
    }

    static Map<String, Object> answer(Map<?, ?> req) throws InterruptedException {
        int n = COUNT.incrementAndGet();
        if (n == 1) {
            System.out.println("[server] Yêu cầu #" + n + ": cố ý trả lời chậm 2000 ms");
            Thread.sleep(2000);
            return lookup(req.get("term"));
        }
        if (n == 2) {
            System.out.println("[server] Yêu cầu #" + n + ": trả lời 503, đề nghị chờ 300 ms");
            return Json.obj("status", 503, "error", "Quá tải tạm thời", "retry_after_ms", 300);
        }
        long delay = req.get("delay_ms") instanceof Long d ? d : 0;
        System.out.println("[server] Yêu cầu #" + n + ": xử lý " + delay + " ms rồi trả lời");
        Thread.sleep(delay);
        return lookup(req.get("term"));
    }

    static void session(Socket conn) {
        try (conn) {
            while (true) {
                Map<?, ?> req = (Map<?, ?>) Khung.recvJson(conn, NO_CANCEL);
                Map<String, Object> resp = answer(req);
                Khung.sendJson(conn, resp, NO_CANCEL);
            }
        } catch (Khung.ConnectionClosed e) {
            if (e.received > 0) System.out.println("[server] Kết nối đóng giữa khung: " + e.getMessage());
        } catch (IOException e) {
            System.out.println("[server] Không gửi được phản hồi, client đã đóng kết nối (" + e.getClass().getSimpleName() + ")");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = Khung.parseEndpoint(args, "0.0.0.0", 9100);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress((String) ep[0], (Integer) ep[1]), 16);
            System.out.println("[server] Đang lắng nghe tại " + ep[0] + ":" + ep[1]);
            while (true) {
                Socket conn = srv.accept();
                Thread.ofVirtual().start(() -> session(conn));
            }
        }
    }
}
