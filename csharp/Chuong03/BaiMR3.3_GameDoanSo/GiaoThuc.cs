using System.Buffers.Binary;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR3_3;

// Giao thức trò chơi đoán số: khung JSON có tiền tố độ dài, bộ sinh số giả ngẫu nhiên.
// Kế thừa Bài 3.2 (Mã giả 3.3–3.6): khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8.
// Các thông điệp (mỗi thông điệp là một đối tượng JSON có trường "type"):
//   client -> server: hello {name}, guess {value}, bye
//   server -> client: welcome, start, turn, result, timeout, left, error {code, msg}, game_over, bye
class ConnectionClosed() : Exception("Đối tác đóng kết nối");
class ProtocolError(string msg) : Exception(msg);

static class GiaoThuc
{
    public const int MaxFrame = 64 * 1024;   // thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    /// <summary>Mã giả 3.4 (RECV_EXACT): nhận đúng n byte.</summary>
    static byte[] RecvExact(Socket s, int n)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = s.Receive(buf, got, n - got, SocketFlags.None);
            if (k == 0) throw new ConnectionClosed();
            got += k;
        }
        return buf;
    }

    /// <summary>Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết (SEND_ALL).</summary>
    public static void SendFrame(Socket s, byte[] payload)
    {
        var frame = new byte[4 + payload.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)payload.Length);
        payload.CopyTo(frame, 4);
        int sent = 0;
        while (sent < frame.Length) sent += s.Send(frame, sent, frame.Length - sent, SocketFlags.None);
    }

    /// <summary>Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte.</summary>
    public static byte[] RecvFrame(Socket s)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(RecvExact(s, 4));
        if (n > MaxFrame) throw new ProtocolError($"Khung quá lớn: {n} byte");
        return RecvExact(s, (int)n);
    }

    public static void SendJson(Socket s, JsonObject obj) =>
        SendFrame(s, Encoding.UTF8.GetBytes(obj.ToJsonString(Opt)));

    /// <summary>Giải mã nội dung khung; trả null nếu không phải đối tượng JSON hợp lệ.</summary>
    public static JsonObject? DecodeJson(byte[] payload)
    {
        try
        {
            string text = new UTF8Encoding(false, true).GetString(payload);   // ném lỗi nếu UTF-8 hỏng
            return JsonNode.Parse(text) as JsonObject;
        }
        catch (Exception e) when (e is JsonException || e is DecoderFallbackException)
        {
            return null;
        }
    }

    public static JsonObject? RecvJson(Socket s) => DecodeJson(RecvFrame(s));

    // ---- tiện ích đọc trường ----
    public static string? Str(this JsonObject o, string k) =>
        o[k] is JsonValue v && v.TryGetValue(out string? s) ? s : null;

    /// <summary>Số nguyên thật sự (không phải chuỗi, bool hay số thực) – giống isinstance(v, int) của Python.</summary>
    public static bool TryInt(JsonNode? n, out long v)
    {
        v = 0;
        return n is JsonValue jv && jv.GetValueKind() == JsonValueKind.Number && jv.TryGetValue(out v);
    }

    public static long Int(this JsonObject o, string k) => TryInt(o[k], out long v) ? v : 0;

    /// <summary>Biểu diễn giá trị như repr() của Python để nhật ký giống bản Python.</summary>
    public static string Repr(JsonNode? n) => n switch
    {
        null => "None",
        JsonValue v when v.GetValueKind() == JsonValueKind.String => $"'{v.GetValue<string>()}'",
        JsonValue v when v.GetValueKind() == JsonValueKind.True => "True",
        JsonValue v when v.GetValueKind() == JsonValueKind.False => "False",
        _ => n.ToJsonString(Opt),
    };
}

/// <summary>Bộ sinh số giả ngẫu nhiên đồng dư tuyến tính (LCG) rất đơn giản.
/// Mọi bản ngôn ngữ dùng cùng công thức nên với cùng hạt giống (seed) cho cùng dãy số,
/// nhờ đó kết quả demo trùng nhau. Không dùng cho mục đích bảo mật.</summary>
class Lcg(long seed)
{
    long x = seed & 0x7FFFFFFF;

    /// <summary>Số nguyên ngẫu nhiên trong [lo, hi].</summary>
    public long NextInt(long lo, long hi)
    {
        x = (x * 1103515245 + 12345) & 0x7FFFFFFF;
        return lo + (x >> 8) % (hi - lo + 1);
    }
}
