using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai4_2;

// Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định
static class Endpoint
{
    public static (string Host, int Port) Parse(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p))
            throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535)
            throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }
}

// JSON_ENCODE/JSON_DECODE + ENCODE/DECODE UTF-8; giữ nguyên chữ có dấu, dạng gọn (giống bản Python, Java)
static class Json
{
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static byte[] ToBytes(JsonObject obj) => Encoding.UTF8.GetBytes(obj.ToJsonString(Opt));

    // Ném JsonException nếu không phải đối tượng JSON hợp lệ
    public static JsonObject FromBytes(byte[] buf, int n) =>
        JsonNode.Parse(Encoding.UTF8.GetString(buf, 0, n)) as JsonObject
        ?? throw new JsonException("không phải đối tượng JSON");

    public static string? Str(JsonNode? n) => n is JsonValue v && v.TryGetValue(out string? s) ? s : null;

    public static string Text(JsonNode? n) => n?.ToJsonString(Opt) ?? "null";
}
