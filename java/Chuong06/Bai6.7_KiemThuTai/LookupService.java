import java.io.DataInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Map;

/**
 * Nghiệp vụ tra cứu thuật ngữ (bản sao từ Bài 6.1, Mã giả 3.7) có độ trễ giả lập DELAY_MS, mô phỏng
 * thời gian chờ cơ sở dữ liệu hoặc dịch vụ phía sau (bài tập Chương 6: 50 ms).
 */
final class LookupService {
    private LookupService() {}

    static volatile int delayMs = 50;                                   // độ trễ giả lập mỗi yêu cầu

    static final Map<String, Object> DICT = load();                     // LOAD_JSON_FILE("glossary.json")

    private static Map<String, Object> load() {
        try {
            return Json.decodeObject(Files.readString(Path.of("glossary.json"), StandardCharsets.UTF_8));
        } catch (IOException e) {
            throw new IllegalStateException("Không đọc được glossary.json: " + e.getMessage());
        }
    }

    /** Mã giả 3.7, dòng 2–12. */
    static Map<String, Object> processRequest(Map<String, Object> req) {
        try {
            Thread.sleep(delayMs);                                      // giả lập chờ dịch vụ phía sau
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        Object op = req.get("op");
        if ("lookup".equals(op)) {
            Object term = req.get("term");
            if (term instanceof String t && DICT.containsKey(t))
                return Json.obj("status", 200, "term", t, "text", DICT.get(t));
            return Json.obj("status", 404, "error", "Không có thuật ngữ");
        }
        if ("list".equals(op)) return Json.obj("status", 200, "terms", new ArrayList<>(DICT.keySet()));
        return Json.obj("status", 400, "error", "Sai thao tác");
    }

    /** Mã giả 3.7, dòng 14–27. */
    static void handleLookup(Socket conn) throws IOException {
        DataInputStream in = new DataInputStream(conn.getInputStream());
        OutputStream out = conn.getOutputStream();
        while (true) {
            Map<String, Object> req;
            try {
                req = NetUtil.recvJson(in);
            } catch (NetUtil.ConnectionClosed e) {
                if (e.received == 0) return;                             // đóng tại ranh giới khung
                throw e;                                                 // đóng giữa khung: lỗi
            } catch (Json.JsonError e) {
                NetUtil.sendJson(out, Json.obj("status", 400, "error", "JSON sai"));
                continue;
            }
            if ("bye".equals(req.get("op"))) return;
            NetUtil.sendJson(out, processRequest(req));
        }
    }
}
