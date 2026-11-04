import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Map;

/**
 * Bài 3.3 – Server tra cứu thuật ngữ dùng khung JSON (Mã giả 3.7, Mục 3.5.1), cổng mặc định 9100.
 * Kế thừa: khung sườn ECHO_SERVER (Mã giả 3.1) với HANDLE_ECHO thay bằng handleLookup;
 * SEND_JSON/RECV_JSON (Mã giả 3.6, Framing.java).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out LookupServer [host] [port]   (mặc định 0.0.0.0 9100; từ điển glossary.json)
 */
public class LookupServer {
    static Map<?, ?> dict = Map.of();                                     // DICT

    /** Nghiệp vụ, độc lập với socket: nhận đối tượng yêu cầu, trả đối tượng phản hồi. */
    static Map<String, Object> processRequest(Object req) {
        String op = opOf(req);
        if ("lookup".equals(op)) {
            Object term = ((Map<?, ?>) req).get("term");
            if (term instanceof String t && dict.containsKey(t))
                return Json.obj("status", 200, "term", t, "text", dict.get(t));
            return Json.obj("status", 404, "error", "Không có thuật ngữ");
        }
        if ("list".equals(op)) return Json.obj("status", 200, "terms", new ArrayList<>(dict.keySet()));
        return Json.obj("status", 400, "error", "Sai thao tác");           // OTHERWISE
    }

    static String opOf(Object req) {
        return req instanceof Map<?, ?> m && m.get("op") instanceof String op ? op : null;
    }

    static void handleLookup(Socket conn, String addr) throws IOException {
        while (true) {
            Object req;
            try {
                req = Framing.recvJson(conn);
            } catch (Framing.ConnectionClosed e) {
                if (e.received == 0) return;                                // đóng tại ranh giới
                throw e;                                                    // đóng giữa khung: lỗi
            } catch (Json.JsonError e) {
                Framing.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"));
                System.out.println("[server] " + addr + ": (JSON sai) -> 400");
                continue;
            }
            if (!(req instanceof Map)) {                                   // dòng 24–27: vd. [1, 2] hoặc "abc"
                Framing.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"));
                System.out.println("[server] " + addr + ": (không phải đối tượng) -> 400");
                continue;
            }
            String op = opOf(req) == null ? "?" : opOf(req);
            if (op.equals("bye")) return;
            Map<String, Object> resp = processRequest(req);
            Framing.sendJson(conn, resp);
            String term = op.equals("lookup") ? " '" + ((Map<?, ?>) req).get("term") + "'" : "";
            System.out.println("[server] " + addr + ": " + op + term + " -> " + resp.get("status"));
        }
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        dict = (Map<?, ?>) Json.parse(Files.readString(Path.of("glossary.json")));  // LOAD_JSON_FILE
        try (ServerSocket srv = new ServerSocket()) {                      // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                      // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 16);                // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port + " (" + dict.size() + " thuật ngữ)");
            while (true) {                                                  // vòng chấp nhận kết nối
                try (Socket conn = srv.accept()) {                          // ACCEPT; try-with-resources = CLOSE
                    String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
                    System.out.println("[server] Client kết nối: " + addr);
                    try {
                        handleLookup(conn, addr);
                    } catch (IOException e) {      // NetError, ConnectionClosed, ProtocolError: không dừng server
                        System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
                    }
                    System.out.println("[server] Client " + addr + " đã đóng kết nối");
                }
            }
        }
    }
}
