using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai4_5;

// Mã giả 4.8 – COOKIE, HANDLE_DATAGRAM (Mục 4.6.1); kế thừa Mã giả 4.3 – thay thân vòng lặp bằng HANDLE_DATAGRAM
static class CookieServer
{
    const int MAX_DGRAM = 1200, MAX_RATIO = 3;
    static readonly byte[] KEY = RandomNumberGenerator.GetBytes(32);                    // dòng 1: RANDOM_BYTES(32)
    static readonly Dictionary<string, string> DICT =
        JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText("glossary.json"))!;

    // PROCESS_REQUEST: nghiệp vụ, độc lập với socket (kế thừa nguyên vẹn)
    static JsonObject ProcessRequest(JsonObject req)
    {
        switch (Json.Str(req["op"]))
        {
            case "lookup":
                string? term = Json.Str(req["term"]);
                if (term != null && DICT.TryGetValue(term, out var text))
                    return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text };
                return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
            case "list":
                return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray(DICT.Keys.Select(k => (JsonNode?)k).ToArray()) };
            default:
                return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };
        }
    }

    // dòng 2–5: mã thách thức gắn với địa chỉ (ip, cổng) và khung thời gian w
    static string Cookie(IPEndPoint addr, long w) =>
        Convert.ToHexStringLower(HMACSHA256.HashData(KEY, Encoding.UTF8.GetBytes($"{addr.Address}:{addr.Port}:{w}")));

    static readonly UTF8Encoding StrictUtf8 = new(false, true);                         // báo lỗi khi gặp byte sai

    // SAFE_EQUAL: so sánh thời gian hằng, không phụ thuộc vị trí ký tự khác nhau đầu tiên
    static bool SafeEqual(string a, string b) =>
        CryptographicOperations.FixedTimeEquals(Encoding.UTF8.GetBytes(a), Encoding.UTF8.GetBytes(b));

    // Mã giả 4.8, dòng 6–25; datagram bị bỏ chỉ được ghi nhật ký, không có phản hồi nào
    static void HandleDatagram(Socket s, byte[] buf, int n, IPEndPoint addr)
    {
        if (n > MAX_DGRAM)                                                               // dòng 7: quá lớn: bỏ
        {
            Console.WriteLine($"[server] Bỏ datagram {n} byte > MAX_DGRAM từ {addr}");
            return;
        }
        JsonNode? node;
        try { node = JsonNode.Parse(StrictUtf8.GetString(buf, 0, n)); }                  // dòng 8–9
        catch (Exception e) when (e is JsonException || e is DecoderFallbackException)   // dòng 10–11: bỏ im lặng
        {
            Console.WriteLine($"[server] Bỏ datagram không phải JSON từ {addr}");
            return;
        }
        if (node is not JsonObject req)                                                  // dòng 13
        {
            Console.WriteLine($"[server] Bỏ datagram không phải đối tượng JSON từ {addr}");
            return;
        }
        long w = DateTimeOffset.UtcNow.ToUnixTimeSeconds() / 60;                         // dòng 14
        string? c = Json.Str(req["cookie"]);
        bool ok = c != null && (SafeEqual(c, Cookie(addr, w)) || SafeEqual(c, Cookie(addr, w - 1)));  // dòng 15–16
        string state = req["cookie"] == null ? "không có" : ok ? "hợp lệ" : "không hợp lệ";
        Console.WriteLine($"[server] Nhận {n} byte từ {addr} (id={Json.Text(req["id"])}, cookie: {state})");
        JsonObject resp = ProcessRequest(req);                                           // dòng 17
        resp["id"] = req["id"]?.DeepClone();
        byte[] outb = Json.ToBytes(resp);                                                // dòng 18
        if (outb.Length > MAX_DGRAM)                                                     // giữ kiểm tra của Mã giả 4.3
        {
            resp = new JsonObject { ["id"] = req["id"]?.DeepClone(), ["status"] = 413, ["error"] = "Quá lớn" };
            outb = Json.ToBytes(resp);
        }
        if (!ok && outb.Length > MAX_RATIO * n)                                          // dòng 19
        {
            var ch = new JsonObject { ["id"] = req["id"]?.DeepClone(), ["status"] = 401, ["cookie"] = Cookie(addr, w) }; // dòng 20
            byte[] small = Json.ToBytes(ch);                                             // dòng 21: phản hồi nhỏ
            if (small.Length > MAX_RATIO * n)                                            // dòng 22: thách thức cũng quá lớn
            {
                Console.WriteLine($"[server]   -> phản hồi {outb.Length} byte, thách thức {small.Length} byte đều > {MAX_RATIO} × {n} byte: không gửi gì");
                return;
            }
            Console.WriteLine($"[server]   -> phản hồi {outb.Length} byte > {MAX_RATIO} × {n} byte: gửi thách thức 401 ({small.Length} byte)");
            outb = small;
        }
        else
            Console.WriteLine($"[server]   -> gửi phản hồi status={resp["status"]} ({outb.Length} byte)");
        s.SendTo(outb, addr);                                                            // dòng 24
    }

    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "0.0.0.0", 9100);
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        s.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        Console.WriteLine($"[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {DICT.Count} thuật ngữ, MAX_RATIO = {MAX_RATIO}");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        var buf = new byte[65535];
        while (true)
        {
            EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = s.ReceiveFrom(buf, ref addr); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset) { continue; }
            HandleDatagram(s, buf, n, (IPEndPoint)addr);
        }
    }
}
