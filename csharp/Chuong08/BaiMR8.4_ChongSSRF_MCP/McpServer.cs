using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR8_4;

// Bài mở rộng MR8.4 – MCP server có công cụ fetch_url, so sánh bản NGÂY THƠ và bản AN TOÀN.
//
// Nối tiếp Bài 8.3 (MCP server công cụ mạng qua stdio, JSON-RPC 2.0 – giữ nguyên khung thông điệp).
// Bài mở rộng, không có trong giáo trình. Hai công cụ:
//   - fetch_url_naive(url): tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF – không dùng trong thực tế);
//   - fetch_url_safe(url):  tải URL có đầy đủ lớp phòng thủ SSRF.
// JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.

sealed class RpcException(int code, string message) : Exception(message)
{
    public int Code => code;
    public string Msg => Message;
}

static class McpServer
{
    static readonly string[] Supported = ["2026-07-28"];
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static Stream _out = null!;

    static JsonObject Tool(string name, string description) => new()
    {
        ["name"] = name,
        ["description"] = description,
        ["inputSchema"] = new JsonObject
        {
            ["type"] = "object",
            ["properties"] = new JsonObject { ["url"] = new JsonObject { ["type"] = "string" } },
            ["required"] = new JsonArray("url"),
        },
    };

    static JsonArray Tools() =>
    [
        Tool("fetch_url_naive", "Tải URL KHÔNG kiểm tra (minh họa lỗ hổng SSRF)"),
        Tool("fetch_url_safe", "Tải URL có kiểm soát chống SSRF"),
    ];

    static readonly string[] ToolNames = ["fetch_url_naive", "fetch_url_safe"];

    static JsonObject ErrorMsg(JsonNode? id, int code, string message) => new()
    {
        ["jsonrpc"] = "2.0",
        ["id"] = id?.DeepClone(),
        ["error"] = new JsonObject { ["code"] = code, ["message"] = message },
    };

    static void WriteMsg(JsonObject obj)
    {
        var bytes = Encoding.UTF8.GetBytes(obj.ToJsonString(Opt) + "\n");
        _out.Write(bytes);
        _out.Flush();
    }

    /// <summary>Rút gọn phần thân phản hồi để in (một dòng, tối đa 120 ký tự).</summary>
    static string TomTatThan(byte[] body)
    {
        string text = Encoding.UTF8.GetString(body).Replace("\n", " ").Trim();
        return text.Length > 120 ? text[..120] + "…" : text;
    }

    static JsonObject CallTool(string? name, JsonObject args)
    {
        if (name is null || !ToolNames.Contains(name))
            throw new RpcException(-32602, "Unknown tool: " + (name ?? "null"));
        if (args["url"]?.GetValueKind() != JsonValueKind.String)
            return Result(true, "Tham số 'url' phải là chuỗi");
        string url = args["url"]!.GetValue<string>();
        FetchTools.LogErr("gọi", name, url);
        try
        {
            var (status, body, steps) = name == "fetch_url_naive" ? FetchTools.FetchNaive(url) : FetchTools.FetchSafe(url);
            foreach (var s in steps) FetchTools.LogErr("   ", s);
            string text = $"HTTP {status} | " + string.Join(" | ", steps.TakeLast(2)) + " | thân: " + TomTatThan(body);
            return Result(false, text);
        }
        catch (Exception e) when (e is SsrfException || e is FetchException)
        {
            FetchTools.LogErr("   TỪ CHỐI/LỖI:", e is SsrfException ? "SsrfError" : "FetchError", e.Message);
            return Result(true, e.Message);
        }
    }

    static JsonObject Result(bool isError, string text) => new()
    {
        ["resultType"] = "complete",
        ["isError"] = isError,
        ["content"] = new JsonArray(new JsonObject { ["type"] = "text", ["text"] = text }),
    };

    static JsonNode Dispatch(string? method, JsonNode? paramsNode)
    {
        var p = paramsNode as JsonObject ?? [];
        var m = p["_meta"] as JsonObject ?? [];
        var v = m["io.modelcontextprotocol/protocolVersion"];
        var caps = m["io.modelcontextprotocol/clientCapabilities"];
        if (v is null || caps is null) throw new RpcException(-32602, "Invalid params: thiếu _meta");
        if (!Supported.Contains(v.GetValue<string>())) throw new RpcException(-32022, "UnsupportedProtocolVersion");
        switch (method)
        {
            case "server/discover":
                return new JsonObject
                {
                    ["resultType"] = "complete",
                    ["supportedVersions"] = new JsonArray(Supported.Select(s => (JsonNode)s).ToArray()),
                    ["capabilities"] = new JsonObject { ["tools"] = new JsonObject() },
                    ["serverInfo"] = new JsonObject { ["name"] = "ssrf-guard-mcp", ["version"] = "1.0.0" },
                };
            case "tools/list":
                return new JsonObject
                {
                    ["resultType"] = "complete",
                    ["tools"] = Tools(),
                    ["ttlMs"] = 3600000L,
                    ["cacheScope"] = "public",
                };
            case "tools/call":
                return CallTool((p["name"] as JsonValue)?.GetValue<string>(), p["arguments"] as JsonObject ?? []);
            default:
                throw new RpcException(-32601, "Method not found");
        }
    }

    public static int Run()
    {
        _out = Console.OpenStandardOutput();
        using var stdin = new StreamReader(Console.OpenStandardInput(), Encoding.UTF8);
        FetchTools.LogErr("MCP server chống SSRF (stdio), phiên bản", string.Join(", ", Supported));
        string? line;
        while ((line = stdin.ReadLine()) is not null)
        {
            JsonNode? msg;
            try { msg = JsonNode.Parse(line); }
            catch (JsonException) { WriteMsg(ErrorMsg(null, -32700, "Parse error")); continue; }
            var obj = msg as JsonObject;
            var id = obj?["id"];
            string? method = (obj?["method"] as JsonValue)?.GetValue<string>();
            if (id is null) { FetchTools.LogErr($"<- thông báo {method}: không trả lời"); continue; }
            FetchTools.LogErr($"<- id={id.GetValue<long>()} {method}");
            try
            {
                WriteMsg(new JsonObject { ["jsonrpc"] = "2.0", ["id"] = id.DeepClone(), ["result"] = Dispatch(method, obj!["params"]) });
            }
            catch (RpcException e) { WriteMsg(ErrorMsg(id, e.Code, e.Msg)); }
            catch (Exception e)
            {
                WriteMsg(ErrorMsg(id, -32603, "Internal error"));
                FetchTools.LogErr($"-> id={id.GetValue<long>()} error -32603 ({e.GetType().Name}: {e.Message})");
            }
        }
        FetchTools.LogErr("Host đã đóng stdin, kết thúc");
        return 0;
    }
}
