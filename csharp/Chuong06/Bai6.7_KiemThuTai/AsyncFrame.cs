using System.Buffers.Binary;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Unicode;

namespace Bai6_7;

// Mã giả 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mục 5.2.4).
// NetworkStream.ReadExactlyAsync thay cho vòng lặp RECV_EXACT_ASYNC; ct là tín hiệu hủy.
static class AsyncFrame
{
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
        if (n > NetUtil.MaxFrame) throw new ProtocolException("Khung quá lớn");
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
}
