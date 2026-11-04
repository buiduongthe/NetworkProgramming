using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai8_3;

class RpcError(int code, string message) : Exception(message)
{
    public int Code { get; } = code;
}

// Mã giả 8.4 – MCP server công cụ mạng qua kênh stdio; Mã giả 8.5 – CALL_TOOL (Mục 8.3.5)
static class McpServer
{
    static readonly string[] Supported = ["2026-07-28"];                         // dòng 1
    static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly Stream Stdout = Console.OpenStandardOutput();
    static readonly Stream Stdin = Console.OpenStandardInput();
    static byte[] pending = [];

    // TOOL(tên, mô tả, tham số) -> mô tả công cụ kèm lược đồ JSON Schema của đầu vào
    static JsonObject Tool(string name, string description, params (string Name, string Type)[] ps)
    {
        var props = new JsonObject();
        foreach (var (n, t) in ps) props[n] = new JsonObject { ["type"] = t };
        return new JsonObject
        {
            ["name"] = name, ["description"] = description,
            ["inputSchema"] = new JsonObject
            { ["type"] = "object", ["properties"] = props, ["required"] = new JsonArray(ps.Select(x => (JsonNode)x.Name).ToArray()) }
        };
    }

    static readonly JsonArray Tools = [                                          // dòng 2–6
        Tool("resolve_host", "Phân giải tên miền", ("host", "string")),
        Tool("check_port", "Kiểm tra cổng TCP", ("host", "string"), ("port", "integer")),
        Tool("ping", "Đo thời gian khứ hồi ICMP", ("host", "string"))];

    static JsonObject ErrorMsg(JsonNode? id, int code, string message) => new()
    { ["jsonrpc"] = "2.0", ["id"] = id, ["error"] = new JsonObject { ["code"] = code, ["message"] = message } };

    // WRITE_MSG: một thông điệp một dòng, FLUSH ngay (dòng 42–44)
    static void WriteMsg(JsonObject obj)
    {
        Stdout.Write(Encoding.UTF8.GetBytes(obj.ToJsonString(Opts) + "\n"));
        Stdout.Flush();
    }

    // READ_LINE (Mã giả 3.5) trên stdin; null khi host đóng stdin
    static string? ReadLine(int maxLen)
    {
        var chunk = new byte[4096];
        while (true)
        {
            int i = Array.IndexOf(pending, (byte)'\n');
            if (i > maxLen) throw new InvalidDataException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0)
            {
                string line = Encoding.UTF8.GetString(pending, 0, i).TrimEnd('\r');
                pending = pending[(i + 1)..];
                return line;
            }
            if (pending.Length > maxLen) throw new InvalidDataException("Dòng quá dài");   // ProtocolError
            int n = Stdin.Read(chunk, 0, chunk.Length);
            if (n == 0) return null;
            pending = [.. pending, .. chunk.AsSpan(0, n)];
        }
    }

    static JsonObject Result(bool isError, string text) => new()
    {
        ["resultType"] = "complete", ["isError"] = isError,
        ["content"] = new JsonArray(new JsonObject { ["type"] = "text", ["text"] = text })
    };

    // Mã giả 8.5 – CALL_TOOL(name, a): kiểm tra tham số trước khi thực thi công cụ
    static JsonObject CallTool(string? name, JsonObject a)
    {
        if (name == null || !Tools.Any(t => (string)t!["name"]! == name))
            throw new RpcError(-32602, "Unknown tool: " + (name ?? "null"));
        NetTools.Log($"  gọi công cụ {name} {a.ToJsonString(Opts)}");
        try
        {
            string text;
            if (name == "resolve_host")
            {
                var outList = NetTools.ResolveHost(Validate.ValidateName(a["host"]));
                text = outList.Length == 0 ? "Không có địa chỉ nào"                             // TO_TEXT
                    : string.Join("; ", outList.Select(x => $"{NetTools.FamilyName(x.AddressFamily)} {x}"));
            }
            else if (name == "check_port")
            {
                var ip = Validate.ValidateTarget(a["host"]);
                int port = Validate.ValidatePort(a["port"]);
                text = $"{ip}:{port} {NetTools.PortCheck(ip.ToString(), port, 2000)}";
            }
            else                                                                                 // "ping"
            {
                var (addr, rtt, ttl) = NetTools.Ping(Validate.ValidateTarget(a["host"]), 1, 2000);
                text = $"{addr}: thời gian khứ hồi {rtt:F3} ms, TTL {ttl}";
            }
            return Result(false, text);
        }
        catch (Exception e) when (e is NetError or PolicyError or ResolveError or TimeoutException)   // dòng 17: báo AI
        {
            NetTools.Log($"  -> {e.GetType().Name}: {e.Message}");
            return Result(true, e.Message);
        }
    }

    static string? Str(JsonNode? n) => n is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;

    // Mã giả 8.4 – DISPATCH(method, p)
    static JsonObject Dispatch(string? method, JsonNode? pNode)
    {
        var p = pNode as JsonObject ?? new JsonObject();                    // params vắng: {}
        var m = p["_meta"] as JsonObject ?? new JsonObject();               // _meta vắng: {}
        JsonNode? v = m["io.modelcontextprotocol/protocolVersion"];
        JsonNode? caps = m["io.modelcontextprotocol/clientCapabilities"];
        if (v == null || caps == null) throw new RpcError(-32602, "Invalid params: thiếu _meta");
        if (!Supported.Contains(Str(v))) throw new RpcError(-32022, "UnsupportedProtocolVersion");
        return method switch
        {
            "server/discover" => new JsonObject                            // DISCOVER_RESULT(SUPPORTED)
            {
                ["resultType"] = "complete", ["supportedVersions"] = new JsonArray(Supported.Select(x => (JsonNode)x).ToArray()),
                ["capabilities"] = new JsonObject { ["tools"] = new JsonObject() },
                ["serverInfo"] = new JsonObject { ["name"] = "nettools-mcp", ["version"] = "1.0.0" }
            },
            "tools/list" => new JsonObject
            { ["resultType"] = "complete", ["tools"] = Tools.DeepClone(), ["ttlMs"] = 3600000, ["cacheScope"] = "public" },
            "tools/call" => CallTool(Str(p["name"]), p["arguments"] as JsonObject ?? new JsonObject()),
            _ => throw new RpcError(-32601, "Method not found")
        };
    }

    // Mã giả 8.4 – MCP_STDIO_SERVER()
    public static void Run()
    {
        string nets = Validate.AllowedNets.Count == 0 ? "(rỗng: chặn mọi đích)" : string.Join(", ", Validate.AllowedNets);
        NetTools.Log($"MCP server công cụ mạng (stdio), phiên bản {string.Join(", ", Supported)} | ALLOWED_NETS = {nets}");
        string? line;
        while ((line = ReadLine(1048576)) != null)                                     // dòng 8
        {
            JsonNode? msg;
            try { msg = JsonNode.Parse(line); }
            catch (JsonException)                                                      // JsonError
            {
                NetTools.Log("<- dòng không phải JSON -> -32700");
                WriteMsg(ErrorMsg(null, -32700, "Parse error"));
                continue;
            }
            JsonNode? id = (msg as JsonObject)?["id"];
            string? method = Str((msg as JsonObject)?["method"]);
            if (id == null) { NetTools.Log($"<- thông báo {method}: không trả lời"); continue; }   // thông báo
            NetTools.Log($"<- id={id.ToJsonString()} {method}");
            try
            {
                WriteMsg(new JsonObject { ["jsonrpc"] = "2.0", ["id"] = id.DeepClone(), ["result"] = Dispatch(method, msg!["params"]) });
                NetTools.Log($"-> id={id.ToJsonString()} result");
            }
            catch (RpcError e)
            {
                WriteMsg(ErrorMsg(id.DeepClone(), e.Code, e.Message));
                NetTools.Log($"-> id={id.ToJsonString()} error {e.Code}");
            }
            catch (Exception e)                                                       // bổ sung: lỗi ngoài dự kiến -> -32603
            {
                WriteMsg(ErrorMsg(id.DeepClone(), -32603, "Internal error"));
                NetTools.Log($"-> id={id.ToJsonString()} error -32603 ({e.GetType().Name}: {e.Message})");
            }
        }
        NetTools.Log("Host đã đóng stdin, kết thúc");
    }
}
