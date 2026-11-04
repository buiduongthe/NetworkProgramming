using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai6_7;

// Nghiệp vụ tra cứu thuật ngữ (bản sao từ Bài 6.1, Mã giả 3.7) có độ trễ giả lập DelayMs, mô phỏng
// thời gian chờ cơ sở dữ liệu hoặc dịch vụ phía sau (bài tập Chương 6: 50 ms).
static class LookupService
{
    public static int DelayMs = 50;                                      // độ trễ giả lập mỗi yêu cầu

    // LOAD_JSON_FILE("glossary.json") – nạp một lần khi khởi động
    static readonly Dictionary<string, string> Dict =
        JsonSerializer.Deserialize<Dictionary<string, string>>(
            File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "glossary.json")))!;

    // Mã giả 3.7, dòng 2–12
    public static JsonObject ProcessRequest(JsonObject req)
    {
        Thread.Sleep(DelayMs);                                           // giả lập chờ dịch vụ phía sau
        switch (Str(req, "op"))
        {
            case "lookup":
                string? term = Str(req, "term");
                if (term != null && Dict.TryGetValue(term, out var text))
                    return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text };
                return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
            case "list":
                return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray(Dict.Keys.Select(k => (JsonNode)k!).ToArray()) };
            default:
                return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };
        }
    }

    /// <summary>Lấy trường chuỗi của đối tượng JSON; null nếu thiếu hoặc không phải chuỗi.</summary>
    public static string? Str(JsonObject o, string key) =>
        o[key] is JsonValue v && v.TryGetValue(out string? s) ? s : null;

    // Mã giả 3.7, dòng 14–27
    public static void HandleLookup(Socket conn)
    {
        while (true)
        {
            JsonObject req;
            try
            {
                req = NetUtil.RecvJson(conn);
            }
            catch (ConnectionClosedException e) when (e.Received == 0)
            {
                return;                                                  // đóng tại ranh giới khung
            }                                                            // đóng giữa khung: lỗi đi lên SERVE
            catch (JsonException)                                        // JsonError
            {
                NetUtil.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                continue;
            }
            if (Str(req, "op") == "bye") return;
            NetUtil.SendJson(conn, ProcessRequest(req));
        }
    }
}
