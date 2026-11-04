using System.Buffers.Binary;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Unicode;

namespace Bai6_4;

/// <summary>Đối tác đóng kết nối; Received = số byte của khung đang nhận dở.</summary>
class ConnectionClosedException(int received) : IOException($"Kết nối bị đóng sau {received} byte")
{
    public int Received { get; } = received;
}

/// <summary>Vi phạm định dạng khung (ví dụ khung quá lớn).</summary>
class ProtocolException(string message) : IOException(message);

// Mã giả 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mục 5.2.4).
// NetworkStream.ReadExactlyAsync thay cho vòng lặp RECV_EXACT_ASYNC; ct là tín hiệu hủy.
static class AsyncFrame
{
    public const int MaxFrame = 1048576;                                 // Mã giả 3.6: 1 MiB
    static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.Create(UnicodeRanges.All) };

    // RECV_EXACT_ASYNC: nhận đủ n byte; đóng giữa chừng → ConnectionClosedException(số byte đã nhận)
    static async Task<byte[]> RecvExactAsync(Stream s, int n, CancellationToken ct)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = await s.ReadAsync(buf.AsMemory(got, n - got), ct);  // AWAIT RECV_ASYNC
            if (k == 0) throw new ConnectionClosedException(got);
            got += k;
        }
        return buf;
    }

    // RECV_JSON_ASYNC (Mã giả 5.1, dòng 12–20). JSON sai gây JsonException
    public static async Task<JsonObject> RecvJsonAsync(Stream s, CancellationToken ct)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(await RecvExactAsync(s, 4, ct));
        if (n > MaxFrame) throw new ProtocolException("Khung quá lớn");
        byte[] data;
        try { data = await RecvExactAsync(s, (int)n, ct); }
        catch (ConnectionClosedException e) { throw new ConnectionClosedException(4 + e.Received); }
        return JsonNode.Parse(Encoding.UTF8.GetString(data)) as JsonObject
               ?? throw new JsonException("Thông điệp không phải đối tượng JSON");
    }

    // SEND_JSON_ASYNC (Mã giả 5.1, dòng 22–24): ghép 4 byte độ dài rồi gửi cả khung
    public static async Task SendJsonAsync(Stream s, JsonObject obj, CancellationToken ct)
    {
        byte[] p = Encoding.UTF8.GetBytes(obj.ToJsonString(Opts));
        var frame = new byte[4 + p.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)p.Length);
        p.CopyTo(frame, 4);
        await s.WriteAsync(frame, ct);                                   // SEND_ALL_ASYNC
    }

    // Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định
    public static (string Host, int Port) ParseEndpoint(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p)) throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535) throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }

    /// <summary>Lấy trường chuỗi của đối tượng JSON; null nếu thiếu hoặc không phải chuỗi.</summary>
    public static string? Str(JsonObject o, string key) =>
        o[key] is JsonValue v && v.TryGetValue(out string? s) ? s : null;
}
