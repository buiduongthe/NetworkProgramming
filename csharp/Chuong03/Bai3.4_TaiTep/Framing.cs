using System.Buffers.Binary;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai3_4;

// Đối tác đóng kết nối; Received = số byte đã nhận của khung (hoặc tệp) dở dang
class ConnectionClosed(int received, string? message = null)
    : Exception(message ?? $"Kết nối đóng giữa chừng (đã nhận {received} byte)")
{
    public int Received { get; } = received;
}
class ProtocolError(string message) : Exception(message);   // vi phạm định dạng giao thức
class JsonError(string message) : Exception(message);       // nội dung khung không phải JSON UTF-8 hợp lệ

// Mã giả 3.5 – STRUCT Reader { sock, pending }: bộ đệm tồn tại giữa các lần READ_LINE
class Reader(Socket sock)
{
    public Socket Sock { get; } = sock;
    public byte[] Pending = [];
}

// Thư viện định khung kế thừa từ Bài 3.2 (chép nguyên để bài tự chứa): Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.5 (READ_LINE), 3.6 (khung JSON)
static class Framing
{
    public const int MaxFrame = 1048576;                                      // 1 MiB
    static readonly JsonSerializerOptions JsonOpts =                          // giữ nguyên chữ tiếng Việt
        new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly UTF8Encoding StrictUtf8 = new(false, true);               // báo lỗi khi gặp byte sai

    // Mã giả 3.3 – SEND_ALL: Send() có thể chỉ gửi được một phần nên phải lặp đến khi hết
    public static void SendAll(Socket s, byte[] data, int count = -1)
    {
        if (count < 0) count = data.Length;
        int sent = 0;
        while (sent < count)
            sent += s.Send(data, sent, count - sent, SocketFlags.None);       // có thể gửi ít hơn yêu cầu
    }

    // Mã giả 3.4 – RECV_EXACT: nhận đúng n byte, báo ConnectionClosed kèm số byte đã nhận
    public static byte[] RecvExact(Socket s, int n)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = s.Receive(buf, got, n - got, SocketFlags.None);
            if (k == 0) throw new ConnectionClosed(got);
            got += k;
        }
        return buf;
    }

    // Mã giả 3.5 – READ_LINE: đọc một dòng kết thúc bằng LF hoặc CRLF; trả null khi đối tác đã đóng
    public static string? ReadLine(Reader r, int maxLen)
    {
        var chunk = new byte[4096];
        while (true)
        {
            int i = Array.IndexOf(r.Pending, (byte)0x0A);                     // vị trí '\n'
            if (i > maxLen) throw new ProtocolError("Dòng quá dài");          // dòng 5: có LF nhưng quá dài
            if (i >= 0)
            {
                int len = (i > 0 && r.Pending[i - 1] == 0x0D) ? i - 1 : i;    // STRIP_CR
                string line = StrictUtf8.GetString(r.Pending, 0, len);
                r.Pending = r.Pending[(i + 1)..];
                return line;
            }
            if (r.Pending.Length > maxLen) throw new ProtocolError("Dòng quá dài");
            int k = r.Sock.Receive(chunk);
            if (k == 0) return null;                                          // đối tác đã đóng
            r.Pending = [.. r.Pending, .. chunk.AsSpan(0, k)];
        }
    }

    // Mã giả 3.6 – SEND_FRAME: ghép 4 byte độ dài (big-endian) với nội dung rồi gửi một lần
    public static void SendFrame(Socket s, byte[] payload)
    {
        var frame = new byte[4 + payload.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)payload.Length);   // UINT32_BE
        payload.CopyTo(frame, 4);
        SendAll(s, frame);
    }

    // Mã giả 3.6 – RECV_FRAME: nhận đủ tiêu đề, kiểm tra MAX_FRAME, nhận đúng n byte nội dung
    public static byte[] RecvFrame(Socket s)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(RecvExact(s, 4));      // FROM_UINT32_BE
        if (n > MaxFrame) throw new ProtocolError($"Khung quá lớn: {n}");
        try { return RecvExact(s, (int)n); }
        catch (ConnectionClosed e) { throw new ConnectionClosed(4 + e.Received); }  // đã nhận xong tiêu đề
    }

    public static string JsonEncode(JsonNode obj) => obj.ToJsonString(JsonOpts);

    // DECODE(…, "UTF-8") rồi JSON_DECODE; mọi lỗi giải mã đều quy về JsonError
    public static JsonNode? JsonDecode(byte[] payload)
    {
        try { return JsonNode.Parse(StrictUtf8.GetString(payload)); }
        catch (Exception e) when (e is JsonException || e is DecoderFallbackException)
        { throw new JsonError(e.Message); }
    }

    public static void SendJson(Socket s, JsonNode obj) =>
        SendFrame(s, Encoding.UTF8.GetBytes(JsonEncode(obj)));               // Mã giả 3.6, SEND_JSON

    public static JsonNode? RecvJson(Socket s) => JsonDecode(RecvFrame(s));   // Mã giả 3.6, RECV_JSON
}
