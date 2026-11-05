using System.Buffers.Binary;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR6_4;

// Giao thức trò chơi Caro: khung JSON có tiền tố độ dài (giống MR3.3, Bài 3.2).
// Khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8. Mỗi thông điệp có trường "type":
//   client -> server: hello {name}, list, create {room}, join {room}, watch {room}, move {r, c},
//                     rematch, leave, bye
//   server -> client: welcome, rooms, joined, start, board, moved, over, rematch_wait, left,
//                     room_closed, error {code, msg}
// Khám phá server trong LAN (UDP broadcast, như Bài 6.5):
//   client phát {"type": "discover", "service": "caro", "id": n}
//   server trả unicast {"id": n, "name", "port", "rooms", "players"}
class ConnectionClosed() : Exception("Đối tác đóng kết nối");
class ProtocolError(string msg) : Exception(msg);

static class GiaoThuc
{
    public const int MaxFrame = 64 * 1024;
    public const string DiscService = "caro";
    public static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

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

    public static void SendFrame(Socket s, byte[] payload)
    {
        var frame = new byte[4 + payload.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)payload.Length);
        payload.CopyTo(frame, 4);
        int sent = 0;
        while (sent < frame.Length) sent += s.Send(frame, sent, frame.Length - sent, SocketFlags.None);
    }

    public static byte[] RecvFrame(Socket s)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(RecvExact(s, 4));
        if (n > MaxFrame) throw new ProtocolError($"Khung quá lớn: {n} byte");
        return RecvExact(s, (int)n);
    }

    public static void SendJson(Socket s, JsonObject obj) => SendFrame(s, Encoding.UTF8.GetBytes(obj.ToJsonString(Opt)));

    public static JsonObject? DecodeJson(ReadOnlySpan<byte> payload)
    {
        try { return JsonNode.Parse(new UTF8Encoding(false, true).GetString(payload)) as JsonObject; }
        catch (Exception e) when (e is JsonException || e is DecoderFallbackException) { return null; }
    }

    public static JsonObject? RecvJson(Socket s) => DecodeJson(RecvFrame(s));

    // ---- tiện ích đọc trường ----
    public static string? Str(this JsonObject o, string k) =>
        o[k] is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;

    public static bool TryInt(JsonNode? n, out int v)
    {
        v = 0;
        return n is JsonValue jv && jv.GetValueKind() == JsonValueKind.Number && jv.TryGetValue(out v);
    }

    public static int Int(this JsonObject o, string k) => TryInt(o[k], out int v) ? v : 0;

    /// <summary>Biểu diễn giá trị như repr() của Python để nhật ký giống bản Python.</summary>
    public static string Repr(JsonNode? n) => n switch
    {
        null => "None",
        JsonValue v when v.GetValueKind() == JsonValueKind.String => $"'{v.GetValue<string>()}'",
        _ => n.ToJsonString(Opt),
    };

    /// <summary>Mã giả 6.7 (DISCOVER, Bài 6.5): phát một yêu cầu broadcast, thu thập MỌI phản hồi đến hết thời gian chờ.
    /// Trả danh sách (ip, port, name, rooms, players), sắp theo tên để kết quả ổn định.</summary>
    public static List<(string Ip, int Port, string Name, int Rooms, int Players)> Discover(int discPort, int waitMs = 1000,
                                                                                         string bcast = "255.255.255.255")
    {
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp) { EnableBroadcast = true };   // SO_BROADCAST
        int qid = Random.Shared.Next(1, 1_000_000);
        var q = new JsonObject { ["type"] = "discover", ["service"] = DiscService, ["id"] = qid };
        s.SendTo(Encoding.UTF8.GetBytes(q.ToJsonString()), new IPEndPoint(IPAddress.Parse(bcast), discPort));
        var found = new List<(string, int, string, int, int)>();
        var sw = Stopwatch.StartNew();
        var buf = new byte[2048];
        while (true)
        {
            int left = waitMs - (int)sw.ElapsedMilliseconds;
            if (left <= 0) break;
            s.ReceiveTimeout = left;
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = s.ReceiveFrom(buf, ref from); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut) { break; }
            var r = DecodeJson(buf.AsSpan(0, n));
            if (r != null && r.Int("id") == qid)              // chỉ nhận phản hồi đúng yêu cầu của mình
                found.Add((((IPEndPoint)from).Address.ToString(), r.Int("port"), r.Str("name") ?? "", r.Int("rooms"), r.Int("players")));
        }
        return found.OrderBy(f => f.Item3, StringComparer.Ordinal).ThenBy(f => f.Item1, StringComparer.Ordinal)
                    .ThenBy(f => f.Item2.ToString(), StringComparer.Ordinal).ToList();
    }
}
