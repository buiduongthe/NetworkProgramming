import java.io.ByteArrayOutputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài 8.3 – MCP server công cụ mạng qua kênh stdio (đặc tả MCP 2026-07-28 như Mục 8.3 trình bày).
 * Mã giả 8.4 (MCP_STDIO_SERVER, DISPATCH, WRITE_MSG), 8.5 (CALL_TOOL), 8.7 (Validate.java).
 * JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
 * Biến môi trường ALLOWED_NETS: các mạng được phép (CIDR, phân cách dấu phẩy).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out McpServer      (thử nhanh: python3 mcp_test_client.py 9000 9001 java -cp out McpServer)
 */
public class McpServer {
    static final List<String> SUPPORTED = List.of("2026-07-28");                  // dòng 1
    static final OutputStream STDOUT = new FileOutputStream(FileDescriptor.out);
    static final InputStream STDIN = System.in;
    static byte[] pending = new byte[0];

    static class RpcError extends Exception {
        final int code;
        RpcError(int code, String message) { super(message); this.code = code; }
    }

    /** TOOL(tên, mô tả, tham số) -> mô tả công cụ kèm lược đồ JSON Schema của đầu vào. */
    static Map<String, Object> tool(String name, String description, String... nameTypePairs) {
        Map<String, Object> props = new LinkedHashMap<>();
        List<Object> required = new ArrayList<>();
        for (int i = 0; i < nameTypePairs.length; i += 2) {
            props.put(nameTypePairs[i], Json.obj("type", nameTypePairs[i + 1]));
            required.add(nameTypePairs[i]);
        }
        return Json.obj("name", name, "description", description,
                "inputSchema", Json.obj("type", "object", "properties", props, "required", required));
    }

    static final List<Object> TOOLS = List.of(                                     // dòng 2–6
            tool("resolve_host", "Phân giải tên miền", "host", "string"),
            tool("check_port", "Kiểm tra cổng TCP", "host", "string", "port", "integer"),
            tool("ping", "Đo thời gian khứ hồi ICMP", "host", "string"));

    static Map<String, Object> errorMsg(Object id, int code, String message) {
        return Json.obj("jsonrpc", "2.0", "id", id, "error", Json.obj("code", code, "message", message));
    }

    /** WRITE_MSG: một thông điệp một dòng, FLUSH ngay (dòng 42–44). */
    static void writeMsg(Map<String, Object> obj) throws IOException {
        STDOUT.write((Json.stringify(obj) + "\n").getBytes(StandardCharsets.UTF_8));
        STDOUT.flush();
    }

    /** READ_LINE (Mã giả 3.5) trên stdin; null khi host đóng stdin. */
    static String readLine(int maxLen) throws IOException {
        byte[] chunk = new byte[4096];
        while (true) {
            for (int i = 0; i < pending.length; i++) {
                if (pending[i] == '\n') {
                    if (i > maxLen) throw new IOException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng quá dài
                    String line = new String(pending, 0, i, StandardCharsets.UTF_8).replaceAll("\r$", "");
                    pending = java.util.Arrays.copyOfRange(pending, i + 1, pending.length);
                    return line;
                }
            }
            if (pending.length > maxLen) throw new IOException("Dòng quá dài");      // ProtocolError
            int n = STDIN.read(chunk);
            if (n <= 0) return null;
            ByteArrayOutputStream b = new ByteArrayOutputStream();
            b.write(pending); b.write(chunk, 0, n);
            pending = b.toByteArray();
        }
    }

    static Map<String, Object> result(boolean isError, String text) {
        return Json.obj("resultType", "complete", "isError", isError,
                "content", List.of(Json.obj("type", "text", "text", text)));
    }

    /** Mã giả 8.5 – CALL_TOOL(name, a): kiểm tra tham số trước khi thực thi công cụ. */
    static Map<String, Object> callTool(Object name, Map<String, Object> a) throws RpcError {
        boolean known = TOOLS.stream().anyMatch(t -> ((Map<?, ?>) t).get("name").equals(name));
        if (!(name instanceof String) || !known)
            throw new RpcError(-32602, "Unknown tool: " + (name instanceof String s ? s : "null"));
        NetTools.log("  gọi công cụ " + name + " " + Json.stringify(a));
        try {
            String text;
            switch ((String) name) {
                case "resolve_host" -> {
                    InetAddress[] out = NetTools.resolveHost(Validate.validateName(a.get("host")));
                    List<String> parts = new ArrayList<>();                                  // TO_TEXT
                    for (InetAddress x : out) parts.add(NetTools.familyName(x) + " " + x.getHostAddress());
                    text = parts.isEmpty() ? "Không có địa chỉ nào" : String.join("; ", parts);
                }
                case "check_port" -> {
                    InetAddress ip = Validate.validateTarget(a.get("host"));
                    int port = Validate.validatePort(a.get("port"));
                    text = ip.getHostAddress() + ":" + port + " " + NetTools.portCheck(ip.getHostAddress(), port, 2000);
                }
                default -> text = NetTools.ping(Validate.validateTarget(a.get("host")), 1, 2000);   // "ping"
            }
            return result(false, text);
        } catch (NetTools.NetError | NetTools.PolicyError | NetTools.ResolveError
                 | java.util.concurrent.TimeoutException e) {                                  // dòng 17: báo AI
            NetTools.log("  -> " + e.getClass().getSimpleName() + ": " + e.getMessage());
            return result(true, e.getMessage());
        }
    }

    /** Mã giả 8.4 – DISPATCH(method, p). */
    @SuppressWarnings("unchecked")
    static Map<String, Object> dispatch(Object method, Object pObj) throws RpcError {
        Map<String, Object> p = pObj instanceof Map ? (Map<String, Object>) pObj : Map.of();   // params vắng: {}
        Map<String, Object> m = p.get("_meta") instanceof Map ? (Map<String, Object>) p.get("_meta") : Map.of();
        Object v = m.get("io.modelcontextprotocol/protocolVersion");
        Object caps = m.get("io.modelcontextprotocol/clientCapabilities");
        if (v == null || caps == null) throw new RpcError(-32602, "Invalid params: thiếu _meta");
        if (!SUPPORTED.contains(v)) throw new RpcError(-32022, "UnsupportedProtocolVersion");
        if ("server/discover".equals(method))                                           // DISCOVER_RESULT
            return Json.obj("resultType", "complete", "supportedVersions", SUPPORTED,
                    "capabilities", Json.obj("tools", Json.obj()),
                    "serverInfo", Json.obj("name", "nettools-mcp", "version", "1.0.0"));
        if ("tools/list".equals(method))
            return Json.obj("resultType", "complete", "tools", TOOLS, "ttlMs", 3600000, "cacheScope", "public");
        if ("tools/call".equals(method))
            return callTool(p.get("name"), p.get("arguments") instanceof Map ? (Map<String, Object>) p.get("arguments") : Map.of());
        throw new RpcError(-32601, "Method not found");
    }

    /** Mã giả 8.4 – MCP_STDIO_SERVER(). */
    public static void main(String[] args) throws IOException {
        String nets = Validate.ALLOWED_NETS.isEmpty() ? "(rỗng: chặn mọi đích)" : Validate.ALLOWED_NETS.toString().replaceAll("[\\[\\]]", "");
        NetTools.log("MCP server công cụ mạng (stdio), phiên bản " + String.join(", ", SUPPORTED) + " | ALLOWED_NETS = " + nets);
        String line;
        while ((line = readLine(1048576)) != null) {                                     // dòng 8
            Object msg;
            try {
                msg = Json.parse(line);
            } catch (IllegalArgumentException e) {                                       // JsonError
                NetTools.log("<- dòng không phải JSON -> -32700");
                writeMsg(errorMsg(null, -32700, "Parse error"));
                continue;
            }
            Map<?, ?> o = msg instanceof Map<?, ?> mm ? mm : Map.of();
            Object id = o.get("id"), method = o.get("method");
            if (id == null) { NetTools.log("<- thông báo " + method + ": không trả lời"); continue; }   // thông báo
            NetTools.log("<- id=" + Json.stringify(id) + " " + method);
            try {
                writeMsg(Json.obj("jsonrpc", "2.0", "id", id, "result", dispatch(method, o.get("params"))));
                NetTools.log("-> id=" + Json.stringify(id) + " result");
            } catch (RpcError e) {
                writeMsg(errorMsg(id, e.code, e.getMessage()));
                NetTools.log("-> id=" + Json.stringify(id) + " error " + e.code);
            } catch (RuntimeException e) {                                               // bổ sung: lỗi ngoài dự kiến
                writeMsg(errorMsg(id, -32603, "Internal error"));
                NetTools.log("-> id=" + Json.stringify(id) + " error -32603 (" + e + ")");
            }
        }
        NetTools.log("Host đã đóng stdin, kết thúc");
    }
}
