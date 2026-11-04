using System.Buffers.Binary;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai8_4;

class ProtocolError(string message) : Exception(message);

// Mã giả 5.1 – Gửi, nhận khung JSON bất đồng bộ (kế thừa Mã giả 3.6): 4 byte độ dài big-endian + JSON UTF-8.
// Chạy trên SslStream giống hệt trên NetworkStream: logic định khung không đổi khi thêm TLS (Mục 8.4.5).
static class Frame
{
    public const int MaxFrame = 1048576;                                     // 1 MiB
    public static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static async Task<JsonObject> RecvJsonAsync(Stream s, CancellationToken ct = default)
    {
        var head = new byte[4];
        await s.ReadExactlyAsync(head, ct);                                  // RECV_EXACT_ASYNC; đóng -> EndOfStreamException
        uint n = BinaryPrimitives.ReadUInt32BigEndian(head);
        if (n > MaxFrame) throw new ProtocolError("Khung quá lớn");
        var data = new byte[n];
        await s.ReadExactlyAsync(data, ct);
        return JsonNode.Parse(Encoding.UTF8.GetString(data)) as JsonObject ?? throw new ProtocolError("Không phải đối tượng JSON");
    }

    public static async Task SendJsonAsync(Stream s, JsonObject obj, CancellationToken ct = default)
    {
        byte[] p = Encoding.UTF8.GetBytes(obj.ToJsonString(Opts));
        var frame = new byte[4 + p.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)p.Length);       // UINT32_BE(LENGTH(p))
        p.CopyTo(frame, 4);
        await s.WriteAsync(frame, ct);                                       // SEND_ALL_ASYNC
        await s.FlushAsync(ct);
    }

    public static string? Str(JsonNode? n) =>
        n is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;
}
