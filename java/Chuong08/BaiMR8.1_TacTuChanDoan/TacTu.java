import java.io.IOException;
import java.net.ConnectException;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.URI;
import java.net.UnknownHostException;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ (tool calling), có lớp kiểm soát.
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM) và Bài 8.3 (MCP server): Bài 8.3 viết
 * phía server công cụ, bài này viết phía "chủ" (host/agent) – vòng lặp
 *     hỏi LLM -> LLM yêu cầu công cụ -> chương trình KIỂM TRA rồi mới chạy -> gửi kết quả -> ...
 * cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh; mọi thao tác mạng do mã của ta thực hiện,
 * sau khi qua các lớp kiểm soát:
 *  1. Chỉ chạy công cụ có trong danh mục TOOLS (chặn công cụ "bịa" do ảo giác).
 *  2. Tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền giá trị.
 *  3. Chính sách đích: chỉ máy thuộc mạng phòng thí nghiệm (loopback, mạng riêng RFC 1918).
 *  4. Giới hạn số vòng lặp; ghi nhật ký kiểm toán từng lời gọi.
 * Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
 *
 * Chạy: java -cp out TacTu "<câu hỏi>"
 */
public class TacTu {
    static final String BASE_URL = System.getenv().getOrDefault("LLM_BASE_URL", "http://127.0.0.1:9000/v1").replaceAll("/+$", "");
    static final String MODEL = System.getenv().getOrDefault("LLM_MODEL", "mock-model");
    static final int MAX_ROUNDS = 4;
    /** Mạng phòng thí nghiệm: {địa chỉ mạng, độ dài tiền tố} – loopback và ba khối RFC 1918. */
    static final int[][] ALLOWED_NETS = {{0x7F000000, 8}, {0x0A000000, 8}, {0xAC100000, 12}, {0xC0A80000, 16}};

    /** Lời gọi công cụ bị từ chối hoặc thất bại – kết quả gửi lại LLM dưới dạng {"loi": ...}. */
    static final class ToolError extends Exception {
        ToolError(String m) { super(m); }
    }

    // ---------- Thân công cụ (chạy bằng socket API, không gọi shell) ----------
    static List<String> resolve(String name) {
        TreeSet<String> s = new TreeSet<>();                     // sắp xếp, bỏ trùng như sorted(set(...)) của Python
        try {
            for (InetAddress a : InetAddress.getAllByName(name)) s.add(a.getHostAddress());
        } catch (UnknownHostException e) {
            return List.of();
        }
        return new ArrayList<>(s);
    }

    static boolean allowed(InetAddress a) {
        if (!(a instanceof Inet4Address)) return false;          // chỉ xét IPv4 cho gọn
        byte[] b = a.getAddress();
        int v = ((b[0] & 255) << 24) | ((b[1] & 255) << 16) | ((b[2] & 255) << 8) | (b[3] & 255);
        for (int[] n : ALLOWED_NETS) {
            int mask = n[1] == 0 ? 0 : -1 << (32 - n[1]);
            if ((v & mask) == n[0]) return true;
        }
        return false;
    }

    /** Chính sách đích: mọi địa chỉ của host phải thuộc ALLOWED_NETS. */
    static List<String> checkTarget(String host) throws ToolError {
        List<String> addrs = resolve(host);
        if (addrs.isEmpty()) throw new ToolError("không phân giải được " + host);
        for (String a : addrs) {
            try {
                if (!allowed(InetAddress.getByName(a))) throw new ToolError(a + " nằm ngoài mạng phòng thí nghiệm");
            } catch (UnknownHostException e) {
                throw new ToolError("địa chỉ sai " + a);
            }
        }
        return addrs;
    }

    static Map<String, Object> toolResolve(Map<String, Object> args) {
        return Json.obj("dia_chi", new ArrayList<Object>(resolve((String) args.get("ten"))));
    }

    static Map<String, Object> toolPort(Map<String, Object> args) throws ToolError {
        List<String> addrs = checkTarget((String) args.get("host"));
        int port = ((Long) args.get("cong")).intValue();
        int timeout = args.containsKey("thoi_gian_cho_ms") ? ((Long) args.get("thoi_gian_cho_ms")).intValue() : 1000;
        String state = "FILTERED";
        for (String a : addrs) {
            try (Socket s = new Socket()) {
                s.connect(new InetSocketAddress(a, port), timeout);
                return Json.obj("trang_thai", "OPEN", "dia_chi", a);
            } catch (ConnectException e) {                       // RST: không có chương trình lắng nghe
                state = "CLOSED";
            } catch (SocketTimeoutException e) {
                // hết thời gian chờ: có thể bị tường lửa lọc
            } catch (IOException e) {
                // lỗi khác: thử địa chỉ tiếp theo
            }
        }
        return Json.obj("trang_thai", state);
    }

    interface Tool { Map<String, Object> run(Map<String, Object> args) throws ToolError; }

    /** Danh mục công cụ: tên -> (hàm, lược đồ tham số JSON Schema gửi cho LLM, mô tả). */
    record Entry(Tool fn, Map<String, Object> schema, String description) {}

    static final Map<String, Entry> TOOLS = new LinkedHashMap<>();
    static {
        TOOLS.put("phan_giai_ten", new Entry(TacTu::toolResolve, Json.obj("type", "object", "required", List.of("ten"),
                "properties", Json.obj("ten", Json.obj("type", "string", "description", "tên máy cần phân giải"))),
                "Phân giải tên máy thành danh sách địa chỉ IP"));
        TOOLS.put("kiem_tra_cong", new Entry(TacTu::toolPort, Json.obj("type", "object", "required", List.of("host", "cong"),
                "properties", Json.obj("host", Json.obj("type", "string"),
                        "cong", Json.obj("type", "integer", "minimum", 1, "maximum", 65535),
                        "thoi_gian_cho_ms", Json.obj("type", "integer", "minimum", 100, "maximum", 5000))),
                "Kiểm tra cổng TCP: OPEN, CLOSED hoặc FILTERED"));
    }

    /** Kiểm tra tham số theo lược đồ (tập con nhỏ của JSON Schema: required, type, minimum, maximum). */
    @SuppressWarnings("unchecked")
    static void validate(Map<String, Object> schema, Object argsObj) throws ToolError {
        if (!(argsObj instanceof Map<?, ?>)) throw new ToolError("tham số phải là đối tượng JSON");
        Map<String, Object> args = (Map<String, Object>) argsObj;
        for (Object key : (List<Object>) schema.get("required"))
            if (!args.containsKey(key)) throw new ToolError("thiếu tham số " + key);
        Map<String, Object> props = (Map<String, Object>) schema.get("properties");
        for (Map.Entry<String, Object> e : args.entrySet()) {
            Map<String, Object> spec = (Map<String, Object>) props.get(e.getKey());
            if (spec == null) throw new ToolError("tham số lạ " + e.getKey());
            Object value = e.getValue();
            if ("integer".equals(spec.get("type")) && !(value instanceof Long)) throw new ToolError(e.getKey() + " phải là số nguyên");
            if ("string".equals(spec.get("type")) && !(value instanceof String)) throw new ToolError(e.getKey() + " phải là chuỗi");
            if (spec.containsKey("minimum")) {
                long v = (Long) value, lo = ((Number) spec.get("minimum")).longValue(), hi = ((Number) spec.get("maximum")).longValue();
                if (v < lo || v > hi) throw new ToolError(e.getKey() + " ngoài miền [" + lo + ", " + hi + "]");
            }
        }
    }

    /** Ba lớp kiểm soát trước khi chạy: danh mục, tham số, chính sách đích (trong thân công cụ). */
    @SuppressWarnings("unchecked")
    static Map<String, Object> runTool(String name, String arguments) throws ToolError {
        Entry t = TOOLS.get(name);
        if (t == null) throw new ToolError("công cụ không có trong danh mục");
        Object args;
        try {
            args = Json.parse(arguments);
        } catch (RuntimeException e) {
            throw new ToolError("tham số không phải JSON");
        }
        validate(t.schema(), args);
        return t.fn().run((Map<String, Object>) args);
    }

    /** JSON gọn, khóa theo thứ tự chữ cái (giống json.dumps(sort_keys=True) của Python, json.Marshal của Go với map). */
    @SuppressWarnings("unchecked")
    static Object sortKeys(Object v) {
        if (v instanceof Map<?, ?> m) {
            TreeMap<String, Object> t = new TreeMap<>();
            for (Map.Entry<?, ?> e : m.entrySet()) t.put(String.valueOf(e.getKey()), sortKeys(e.getValue()));
            return t;
        }
        if (v instanceof List<?> l) {
            List<Object> r = new ArrayList<>();
            for (Object x : l) r.add(sortKeys(x));
            return r;
        }
        return v;
    }

    static String compact(Object obj) { return Json.stringify(sortKeys(obj)); }

    // ---------- Gọi API LLM (không truyền luồng) ----------
    static final HttpClient HTTP = HttpClient.newBuilder().version(HttpClient.Version.HTTP_1_1).connectTimeout(Duration.ofSeconds(10)).build();

    @SuppressWarnings("unchecked")
    static Map<String, Object> chat(List<Object> messages) throws IOException, InterruptedException {
        String key = System.getenv("LLM_API_KEY");
        if (key == null || key.isEmpty()) {
            System.err.println("Thiếu biến môi trường LLM_API_KEY");
            System.exit(1);
        }
        List<Object> tools = new ArrayList<>();
        for (Map.Entry<String, Entry> e : TOOLS.entrySet())
            tools.add(Json.obj("type", "function", "function", Json.obj("name", e.getKey(), "description", e.getValue().description(),
                    "parameters", e.getValue().schema())));
        Map<String, Object> req = Json.obj("model", MODEL, "messages", messages, "temperature", 0, "tools", tools);
        HttpRequest rq = HttpRequest.newBuilder(URI.create(BASE_URL + "/chat/completions")).timeout(Duration.ofSeconds(10))
                .header("Content-Type", "application/json").header("Authorization", "Bearer " + key)
                .POST(HttpRequest.BodyPublishers.ofString(Json.stringify(req), StandardCharsets.UTF_8)).build();
        HttpResponse<String> resp = HTTP.send(rq, HttpResponse.BodyHandlers.ofString(StandardCharsets.UTF_8));
        if (resp.statusCode() != 200) throw new IOException("HTTP " + resp.statusCode());
        Map<String, Object> body = (Map<String, Object>) Json.parse(resp.body());
        return (Map<String, Object>) ((Map<String, Object>) ((List<Object>) body.get("choices")).get(0)).get("message");
    }

    /** Ngắt dòng theo từ, mỗi dòng tối đa width ký tự (như textwrap.wrap của Python). */
    static List<String> wrap(String text, int width) {
        List<String> lines = new ArrayList<>();
        StringBuilder cur = new StringBuilder();
        for (String w : text.strip().split("\\s+")) {
            if (w.isEmpty()) continue;
            while (w.length() > width) {                         // từ dài hơn một dòng: cắt
                if (cur.length() > 0) { lines.add(cur.toString()); cur.setLength(0); }
                lines.add(w.substring(0, width));
                w = w.substring(width);
            }
            if (cur.length() == 0) cur.append(w);
            else if (cur.length() + 1 + w.length() <= width) cur.append(' ').append(w);
            else { lines.add(cur.toString()); cur.setLength(0); cur.append(w); }
        }
        if (cur.length() > 0) lines.add(cur.toString());
        return lines;
    }

    /** Windows: JVM đổi tham số dòng lệnh sang bảng mã ANSI của hệ thống (thường cp1252) nên chữ Việt như "ở", "ượ"
     *  thành "?". Đọc lại dòng lệnh gốc (UTF-16) của tiến trình, tách theo quy tắc của Windows và lấy args.length
     *  tham số cuối. Hệ điều hành khác (hoặc không đọc được) giữ nguyên args. */
    static String[] thamSoUnicode(String[] args) {
        if (args.length == 0 || !System.getProperty("os.name", "").startsWith("Windows")) return args;
        String dong = ProcessHandle.current().info().commandLine().orElse(null);
        if (dong == null) return args;
        java.util.List<String> ds = new java.util.ArrayList<>();
        StringBuilder sb = new StringBuilder();
        boolean trongNhay = false, co = false;
        for (int i = 0; i < dong.length(); ) {
            char c = dong.charAt(i);
            if (c == '\\') {                                    // n dấu \ trước " -> n/2 dấu \ (+ " nếu n lẻ)
                int n = 0;
                while (i < dong.length() && dong.charAt(i) == '\\') { n++; i++; }
                boolean nhay = i < dong.length() && dong.charAt(i) == '"';
                sb.append("\\".repeat(nhay ? n / 2 : n));
                if (nhay && n % 2 == 1) { sb.append('"'); i++; }
                co = true;
            } else if (c == '"') {
                if (trongNhay && i + 1 < dong.length() && dong.charAt(i + 1) == '"') { sb.append('"'); i += 2; }
                else { trongNhay = !trongNhay; i++; }
                co = true;
            } else if ((c == ' ' || c == '\t') && !trongNhay) {
                if (co) { ds.add(sb.toString()); sb.setLength(0); co = false; }
                i++;
            } else { sb.append(c); co = true; i++; }
        }
        if (co) ds.add(sb.toString());
        if (ds.size() < args.length) return args;
        String[] kq = ds.subList(ds.size() - args.length, ds.size()).toArray(new String[0]);
        for (int k = 0; k < args.length; k++)                      // tách lệch (tham số không hỏng mà khác) -> giữ nguyên
            if (!args[k].equals(kq[k]) && !args[k].contains("?")) return args;
        return kq;
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        args = thamSoUnicode(args);
        String question = args.length > 0 ? args[0] : "Kiểm tra localhost cổng 80";
        System.out.println("[tac-tu] Hỏi: " + question);
        List<Object> messages = new ArrayList<>(List.of(
                Json.obj("role", "system", "content", "Bạn là trợ lý chẩn đoán mạng. Chỉ dùng công cụ được cung cấp."),
                Json.obj("role", "user", "content", question)));
        for (int rnd = 1; rnd <= MAX_ROUNDS; rnd++) {
            Map<String, Object> msg;
            try {
                msg = chat(messages);
            } catch (IOException e) {
                System.out.println("[tac-tu] Lỗi gọi LLM: " + e);
                System.exit(1);
                return;
            }
            messages.add(msg);
            List<Object> calls = msg.get("tool_calls") instanceof List<?> l ? (List<Object>) l : List.of();
            if (calls.isEmpty()) {
                System.out.println("[tac-tu] Vòng " + rnd + ": LLM trả lời");
                Object content = msg.get("content");
                for (String line : wrap(content instanceof String s ? s : "", 58)) System.out.println("  " + line);
                return;
            }
            System.out.println("[tac-tu] Vòng " + rnd + ": LLM yêu cầu " + calls.size() + " công cụ");
            for (Object co : calls) {
                Map<String, Object> c = (Map<String, Object>) co;
                Map<String, Object> fn = (Map<String, Object>) c.get("function");
                String name = (String) fn.get("name"), arguments = (String) fn.get("arguments");
                Map<String, Object> result;
                try {
                    result = runTool(name, arguments);
                    System.out.println("  CHẠY  " + name + arguments + " -> " + compact(result));
                } catch (ToolError e) {
                    result = Json.obj("loi", e.getMessage());
                    System.out.println("  CHẶN  " + name + arguments + ": " + e.getMessage());
                }
                messages.add(Json.obj("role", "tool", "tool_call_id", c.get("id"), "content", compact(result)));
            }
        }
        System.out.println("[tac-tu] Dừng: quá " + MAX_ROUNDS + " vòng mà chưa có câu trả lời");
    }
}
