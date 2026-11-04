using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai6_2;

// PROCESS_REQUEST của Mã giả 3.7 (Mục 3.5.1), giữ nguyên: nghiệp vụ nhanh, không chặn, độc lập với socket.
static class LookupService
{
    // LOAD_JSON_FILE("glossary.json") – nạp một lần khi khởi động
    static readonly Dictionary<string, string> Dict =
        JsonSerializer.Deserialize<Dictionary<string, string>>(
            File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "glossary.json")))!;

    public static JsonObject ProcessRequest(JsonObject req)
    {
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
    static string? Str(JsonObject o, string key) =>
        o[key] is JsonValue v && v.TryGetValue(out string? s) ? s : null;
}
