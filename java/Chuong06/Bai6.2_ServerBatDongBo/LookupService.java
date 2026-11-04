import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Map;

/** PROCESS_REQUEST của Mã giả 3.7 (Mục 3.5.1), giữ nguyên: nghiệp vụ nhanh, không chặn, độc lập với socket. */
final class LookupService {
    private LookupService() {}

    static final Map<String, Object> DICT = load();                     // LOAD_JSON_FILE("glossary.json")

    private static Map<String, Object> load() {
        try {
            return Json.decodeObject(Files.readString(Path.of("glossary.json"), StandardCharsets.UTF_8));
        } catch (IOException e) {
            throw new IllegalStateException("Không đọc được glossary.json: " + e.getMessage());
        }
    }

    static Map<String, Object> processRequest(Map<String, Object> req) {
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
}
