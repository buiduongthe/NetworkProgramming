import java.io.ByteArrayOutputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR8.4 – MCP server có công cụ fetch_url, so sánh bản NGÂY THƠ và bản AN TOÀN.
 *
 * Nối tiếp Bài 8.3 (MCP server công cụ mạng qua stdio, JSON-RPC 2.0 – giữ nguyên khung thông điệp).
 * Bài mở rộng, không có trong giáo trình. Hai công cụ:
 *   - fetch_url_naive(url): tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF – không dùng trong thực tế);
 *   - fetch_url_safe(url):  tải URL có đầy đủ lớp phòng thủ SSRF.
 * JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
 *
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy (do host khởi chạy như tiến trình con): java -cp out McpServerSsrf
 */
public class McpServerSsrf {
    static final List<String> SUPPORTED = List.of("2026-07-28");
    static final OutputStream STDOUT = new FileOutputStream(FileDescriptor.out);
    static final InputStream STDIN = System.in;
    static byte[] pending = new byte[0];

    static class RpcError extends Exception {
        final int code;
        RpcError(int code, String message) { super(message); this.code = code; }
    }

    static Map<String, Object> tool(String name, String description) {
        return Json.obj("name", name, "description", description,
                "inputSchema", Json.obj("type", "object", "properties", Json.obj("url", Json.obj("type", "string")),
                        "required", List.of("url")));
    }

    static final List<Object> TOOLS = List.of(
            tool("fetch_url_naive", "Tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF)"),
            tool("fetch_url_safe", "Tải URL có kiểm soát chống SSRF"));

    static Map<String, Object> errorMsg(Object id, int code, String message) {
        return Json.obj("jsonrpc", "2.0", "id", id, "error", Json.obj("code", code, "message", message));
    }

    static void writeMsg(Map<String, Object> obj) throws IOException {
        STDOUT.write((Json.stringify(obj) + "\n").getBytes(StandardCharsets.UTF_8));
        STDOUT.flush();
    }

    /** READ_LINE trên stdin (giới hạn độ dài dòng); null khi host đóng stdin. */
    static String readLine(int maxLen) throws IOException {
        byte[] chunk = new byte[4096];
        while (true) {
            for (int i = 0; i < pending.length; i++) {
                if (pending[i] == '\n') {
                    if (i > maxLen) throw new IOException("Dòng quá dài");
                    String line = new String(pending, 0, i, StandardCharsets.UTF_8).replaceAll("\r$", "");
                    pending = java.util.Arrays.copyOfRange(pending, i + 1, pending.length);
                    return line;
                }
            }
            if (pending.length > maxLen) throw new IOException("Dòng quá dài");
            int n = STDIN.read(chunk);
            if (n <= 0) return null;
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            b.write(pending); b.write(chunk, 0, n);
            pending = b.toByteArray();
        }
    }

    /** Rút gọn phần thân phản hồi để in (một dòng, tối đa 120 ký tự). */
    static String tomTatThan(byte[] body) {
        String text = new String(body, StandardCharsets.UTF_8).replace("\n", " ").strip();
        int n = text.codePointCount(0, text.length());
        if (n <= 120) return text;
        return text.substring(0, text.offsetByCodePoints(0, 120)) + "…";
    }

    static Map<String, Object> result(boolean isError, String text) {
        return Json.obj("resultType", "complete", "isError", isError,
                "content", List.of(Json.obj("type", "text", "text", text)));
    }

    static Map<String, Object> callTool(Object name, Map<String, Object> a) throws RpcError, IOException {
        boolean known = TOOLS.stream().anyMatch(t -> ((Map<?, ?>) t).get("name").equals(name));
        if (!(name instanceof String) || !known)
            throw new RpcError(-32602, "Unknown tool: " + (name instanceof String s ? s : "null"));
        if (!(a.get("url") instanceof String url)) return result(true, "Tham số 'url' phải là chuỗi");
        FetchTools.logErr("gọi", (String) name, url);
        try {
            FetchTools.Ket k = name.equals("fetch_url_naive") ? FetchTools.fetchNaive(url) : FetchTools.fetchSafe(url);
            for (String s : k.log()) FetchTools.logErr("   ", s);
            List<String> cuoi = k.log().subList(Math.max(0, k.log().size() - 2), k.log().size());
            return result(false, "HTTP " + k.status() + " | " + String.join(" | ", cuoi) + " | thân: " + tomTatThan(k.body()));
        } catch (FetchTools.SsrfError | FetchTools.FetchError e) {
            FetchTools.logErr("   TỪ CHỐI/LỖI:", e.getClass().getSimpleName(), e.getMessage());
            return result(true, e.getMessage());
        }
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> dispatch(Object method, Object pObj) throws RpcError, IOException {
        Map<String, Object> p = pObj instanceof Map ? (Map<String, Object>) pObj : Map.of();
        Map<String, Object> m = p.get("_meta") instanceof Map ? (Map<String, Object>) p.get("_meta") : Map.of();
        Object v = m.get("io.modelcontextprotocol/protocolVersion");
        Object caps = m.get("io.modelcontextprotocol/clientCapabilities");
        if (v == null || caps == null) throw new RpcError(-32602, "Invalid params: thiếu _meta");
        if (!SUPPORTED.contains(v)) throw new RpcError(-32022, "UnsupportedProtocolVersion");
        if ("server/discover".equals(method))
            return Json.obj("resultType", "complete", "supportedVersions", SUPPORTED,
                    "capabilities", Json.obj("tools", Json.obj()),
                    "serverInfo", Json.obj("name", "ssrf-guard-mcp", "version", "1.0.0"));
        if ("tools/list".equals(method))
            return Json.obj("resultType", "complete", "tools", TOOLS, "ttlMs", 3600000, "cacheScope", "public");
        if ("tools/call".equals(method))
            return callTool(p.get("name"), p.get("arguments") instanceof Map ? (Map<String, Object>) p.get("arguments") : Map.of());
        throw new RpcError(-32601, "Method not found");
    }

    public static void main(String[] args) throws IOException {
        FetchTools.logErr("MCP server chống SSRF (stdio), phiên bản", String.join(", ", SUPPORTED));
        String line;
        while ((line = readLine(1048576)) != null) {
            Object msg;
            try {
                msg = Json.parse(line);
            } catch (IllegalArgumentException e) {
                writeMsg(errorMsg(null, -32700, "Parse error"));
                continue;
            }
            Map<?, ?> o = msg instanceof Map<?, ?> mm ? mm : Map.of();
            Object id = o.get("id"), method = o.get("method");
            if (id == null) { FetchTools.logErr("<- thông báo " + method + ": không trả lời"); continue; }
            FetchTools.logErr("<- id=" + Json.stringify(id) + " " + method);
            try {
                writeMsg(Json.obj("jsonrpc", "2.0", "id", id, "result", dispatch(method, o.get("params"))));
            } catch (RpcError e) {
                writeMsg(errorMsg(id, e.code, e.getMessage()));
            } catch (IOException | RuntimeException e) {          // lỗi nối mạng/ngoài dự kiến -> -32603
                writeMsg(errorMsg(id, -32603, "Internal error"));
                FetchTools.logErr("-> id=" + Json.stringify(id) + " error -32603 (" + e.getClass().getSimpleName() + ": " + e.getMessage() + ")");
            }
        }
        FetchTools.logErr("Host đã đóng stdin, kết thúc");
    }
}
