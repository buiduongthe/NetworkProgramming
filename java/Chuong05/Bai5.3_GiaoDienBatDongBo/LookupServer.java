import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.CompletableFuture;

/**
 * Bài 5.3 – Server tra cứu tối giản phục vụ ứng dụng có giao diện: dùng lại server của Bài 5.1
 * (nghiệp vụ Mã giả 3.7, khung JSON Mã giả 5.1), thêm độ trễ giả lập delayMs trước mỗi phản hồi.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out LookupServer [host] [port] [delay_ms]      (mặc định 0.0.0.0 9100 0)
 */
public class LookupServer {
    static int delayMs;
    static final Map<String, String> DICT = new LinkedHashMap<>();    // từ điển nhỏ thay cho glossary.json
    static {
        DICT.put("TCP", "Giao thức truyền tin cậy, hướng kết nối");
        DICT.put("UDP", "Giao thức datagram, không kết nối");
        DICT.put("Socket", "Điểm cuối truyền thông của một chương trình");
        DICT.put("Bất đồng bộ", "Khởi động vào/ra rồi làm việc khác, quay lại khi xong");
    }

    static Map<String, Object> processRequest(Map<?, ?> req) {        // Mã giả 3.7: độc lập với socket
        Object op = req.get("op");
        if ("lookup".equals(op)) {
            Object term = req.get("term");
            if (DICT.containsKey(term)) return Json.obj("status", 200, "term", term, "text", DICT.get(term));
            return Json.obj("status", 404, "error", "Không có thuật ngữ");
        }
        if ("list".equals(op)) return Json.obj("status", 200, "terms", new ArrayList<>(DICT.keySet()));
        return Json.obj("status", 400, "error", "Sai thao tác");
    }

    static void handleLookup(Socket conn) {                            // HANDLE_LOOKUP trên luồng ảo
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        System.out.println("[server] Client kết nối: " + addr);
        CompletableFuture<Void> ct = Khung.newCancellation();
        try (conn) {
            while (true) {
                Map<?, ?> req;
                try {
                    req = (Map<?, ?>) Khung.recvJson(conn, ct);        // luồng ảo chờ khung, không chiếm luồng HĐH
                } catch (Khung.ConnectionClosed e) {
                    if (e.received == 0) break;                        // đóng tại ranh giới khung: bình thường
                    throw e;                                           // đóng giữa khung: lỗi
                } catch (IllegalArgumentException | ClassCastException e) {
                    Khung.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"), ct);
                    continue;
                }
                System.out.println("[server] Nhận từ " + addr + ": " + Json.stringify(req));
                if ("bye".equals(req.get("op"))) break;
                try { Thread.sleep(delayMs); } catch (InterruptedException e) { return; }   // độ trễ giả lập
                Khung.sendJson(conn, processRequest(req), ct);
            }
        } catch (IOException e) {                                      // lỗi một client không dừng server
            System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
        }
        System.out.println("[server] Client " + addr + " đã đóng kết nối");
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = Khung.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "0.0.0.0", 9100);
        delayMs = args.length > 2 ? Integer.parseInt(args[2]) : 0;
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);           // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port + ", độ trễ giả lập " + delayMs + " ms");
            while (true) {
                Socket conn = srv.accept();
                Thread.ofVirtual().start(() -> handleLookup(conn));    // RUN_BACKGROUND: mỗi kết nối một luồng ảo
            }
        }
    }
}
