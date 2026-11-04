import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.URLDecoder;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Bài 7.2 – Server REST tối giản trên HTTP/1.1 (Mã giả 7.2, Mục 7.2.3 Giáo trình Lập trình mạng).
 * Kế thừa: Mã giả 6.3 (mỗi kết nối một tác vụ – Java dùng luồng ảo), 7.1 (READ_HEADERS), 3.7 (DICT).
 * Bổ sung theo bài tập: POST /api/v1/terms (201 + Location), DELETE /api/v1/terms/{term} (204).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out RestServer [host] [port]      (mặc định 0.0.0.0 8080)
 */
public class RestServer {
    static final long MAX_BODY = 1048576;                               // 1 MiB, như MAX_FRAME (Mã giả 3.6)
    static final Map<String, String> DICT = new LinkedHashMap<>();
    static final Map<Integer, String> REASON = Map.of(200, "OK", 201, "Created", 204, "No Content",
            400, "Bad Request", 404, "Not Found", 405, "Method Not Allowed", 409, "Conflict",
            413, "Content Too Large", 501, "Not Implemented");

    record Result(int status, Object obj, String location) {}

    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "0.0.0.0";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        DICT.putAll(Json.parseObject(Files.readString(Path.of("glossary.json"))));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT
                Thread.startVirtualThread(() -> httpSession(conn));     // RUN_BACKGROUND trên luồng ảo
            }
        }
    }

    static Map<String, Object> problem(int status, String title) {     // PROBLEM theo RFC 9457
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("type", "about:blank"); m.put("title", title); m.put("status", status);
        return m;
    }

    /** Mã giả 7.2, dòng 1–22: phiên keep-alive, nhiều yêu cầu trên một kết nối. */
    static void httpSession(Socket conn) {
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        try (conn) {
            Reader r = new Reader(conn.getInputStream());
            OutputStream out = conn.getOutputStream();
            while (true) {
                String line = r.readLine(8192);
                if (line == null) return;                               // client đóng kết nối
                String[] parts = line.split(" ");
                if (parts.length != 3) { respond(out, new Result(400, problem(400, "Dòng yêu cầu sai"), null)); return; }
                String method = parts[0], target = parts[1];
                Map<String, String> hdr = r.readHeaders();
                long n = Long.parseLong(hdr.getOrDefault("content-length", "0"));   // dòng 8: TO_INT64, có thể > 4 GB
                if (hdr.containsKey("transfer-encoding")) {             // chưa hỗ trợ chunked
                    System.out.println("[server] " + addr + " " + method + " " + target + " -> 501");
                    respond(out, new Result(501, problem(501, "Chưa hỗ trợ"), null));
                    return;                                             // độ dài thân không xác định: đóng
                }
                if (n < 0 || n > MAX_BODY) {                            // dòng 13
                    System.out.println("[server] " + addr + " " + method + " " + target + " -> 413");
                    respond(out, new Result(413, problem(413, "Thân quá lớn"), null));
                    return;
                }
                byte[] body = r.readExact((int) n);
                Result res = route(method, target.split("\\?")[0], body);   // PATH(target): bỏ query
                System.out.println("[server] " + addr + " " + method + " " + target + " -> " + res.status());
                respond(out, res);
                if (hdr.getOrDefault("connection", "").equalsIgnoreCase("close")) return;
            }
        } catch (IOException | IllegalArgumentException e) {
            System.out.println("[server] Kết thúc phiên " + addr + ": " + e.getMessage());
        } finally {
            System.out.println("[server] Đóng kết nối " + addr);      // try-with-resources đã đóng conn
        }
    }

    /** Mã giả 7.2, dòng 23–37 (thêm POST, DELETE); synchronized vì nhiều luồng ảo dùng chung DICT. */
    static synchronized Result route(String method, String path, byte[] body) {
        String[] p = path.replaceAll("^/+|/+$", "").split("/");         // ["api","v1","terms",…]
        if (p.length < 3 || !p[0].equals("api") || !p[1].equals("v1") || !p[2].equals("terms") || p.length > 4)
            return new Result(404, problem(404, "Không có tài nguyên"), null);
        if (method.equals("GET") && p.length == 3)
            return new Result(200, Map.of("terms", new ArrayList<>(DICT.keySet())), null);
        if (method.equals("GET") && p.length == 4) {
            String t = URLDecoder.decode(p[3], StandardCharsets.UTF_8);    // URL_DECODE: %20 -> dấu cách
            if (DICT.containsKey(t)) return new Result(200, termObj(t, DICT.get(t)), null);
            return new Result(404, problem(404, "Không có thuật ngữ"), null);
        }
        if (method.equals("POST") && p.length == 3) {                   // mở rộng: thêm thuật ngữ
            Map<String, String> obj;
            try { obj = Json.parseObject(new String(body, StandardCharsets.UTF_8)); }
            catch (RuntimeException e) { obj = Map.of(); }
            String t = obj.get("term"), text = obj.get("text");
            if (t == null || text == null) return new Result(400, problem(400, "JSON sai hoặc thiếu trường term, text"), null);
            if (DICT.containsKey(t)) return new Result(409, problem(409, "Thuật ngữ đã tồn tại"), null);
            DICT.put(t, text);
            String loc = "/api/v1/terms/" + URLEncoder.encode(t, StandardCharsets.UTF_8).replace("+", "%20");
            return new Result(201, termObj(t, text), loc);
        }
        if (method.equals("DELETE") && p.length == 4) {                 // mở rộng: xóa thuật ngữ
            if (DICT.remove(URLDecoder.decode(p[3], StandardCharsets.UTF_8)) == null)
                return new Result(404, problem(404, "Không có thuật ngữ"), null);
            return new Result(204, null, null);
        }
        return new Result(405, problem(405, "Phương thức không hỗ trợ"), null);
    }

    static Map<String, Object> termObj(String t, String text) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("term", t); m.put("text", text);
        return m;
    }

    /** Mã giả 7.2, dòng 38–48: Content-Length là số byte sau khi mã hóa UTF-8. */
    static void respond(OutputStream out, Result res) throws IOException {
        StringBuilder h = new StringBuilder("HTTP/1.1 " + res.status() + " " + REASON.get(res.status()) + "\r\n");
        if (res.location() != null) h.append("Location: ").append(res.location()).append("\r\n");
        byte[] b = new byte[0];
        if (res.status() != 204) {                                      // dòng 40–41: 204 không thân, không Content-Length
            b = Json.encode(res.obj()).getBytes(StandardCharsets.UTF_8);
            String ct = res.status() >= 400 ? "application/problem+json" : "application/json";
            h.append("Content-Type: ").append(ct).append("; charset=utf-8\r\n");
            h.append("Content-Length: ").append(b.length).append("\r\n");
        }
        h.append("\r\n");
        out.write(h.toString().getBytes(StandardCharsets.US_ASCII));    // SEND_ALL
        out.write(b);
        out.flush();
    }
}
