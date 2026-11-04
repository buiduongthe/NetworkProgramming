using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai4_5;

// Client tra cứu UDP xử lý mã thách thức (đoạn văn sau Mã giả 4.8, Mục 4.6.1): nhận 401 kèm cookie thì lưu mã
// và gửi lại yêu cầu có trường cookie; các yêu cầu sau dùng lại mã đó. Kế thừa Mã giả 4.4 UDP_REQUEST.
static class CookieClient
{
    static long nextId = 0;
    // Yêu cầu chưa có cookie được đệm khoảng trắng tới MIN_FIRST byte để thách thức 401 (khoảng 100 byte)
    // không vượt MAX_RATIO = 3 lần yêu cầu (đoạn văn sau Mã giả 4.8)
    const int MIN_FIRST = 48;
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static long Now() => Clock.ElapsedMilliseconds;                                      // NOW() tính bằng ms

    // Mã giả 4.4 – UDP_REQUEST (chép từ Bài 4.2); các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả.
    // padTo (bổ sung): đệm khoảng trắng sau JSON tới padTo byte – JSON hợp lệ cho phép khoảng trắng cuối
    static JsonObject UdpRequest(Socket s, IPEndPoint server, JsonObject req, int timeoutMs, int maxTries, int padTo = 0)
    {
        long id = ++nextId;
        req["id"] = id;                                                                  // dòng 2: NEXT_ID()
        byte[] payload = Json.ToBytes(req);                                              // dòng 3
        if (payload.Length < padTo)
            payload = [.. payload, .. Enumerable.Repeat((byte)' ', padTo - payload.Length)];
        var buf = new byte[65535];
        for (int attempt = 1; attempt <= maxTries; attempt++)                            // dòng 4
        {
            s.SendTo(payload, server);                                                   // dòng 5
            Console.WriteLine($"[client]   Lần {attempt}: gửi id={id} ({payload.Length} byte), chờ tối đa {timeoutMs} ms");
            long deadline = Now() + timeoutMs;                                           // dòng 6
            while (Now() < deadline)                                                     // dòng 7
            {
                s.ReceiveTimeout = (int)Math.Max(1, deadline - Now());                   // dòng 8: phần còn lại
                EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
                JsonObject resp;
                try
                {
                    int n = s.ReceiveFrom(buf, ref addr);                                // dòng 10
                    resp = Json.FromBytes(buf, n);                                       // dòng 11
                }
                catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut           // dòng 12–13:
                                             || e.SocketErrorCode == SocketError.ConnectionReset) // Windows: ICMP
                { break; }
                catch (JsonException) { continue; }                                      // dòng 14–15: không phải JSON
                if (addr.Equals(server) && resp["id"] is JsonValue v && v.TryGetValue(out long rid) && rid == id)
                    return resp;                                                         // dòng 17
                Console.WriteLine($"[client]   Bỏ datagram lạ hoặc muộn từ {addr}");
            }
            Console.WriteLine($"[client]   Hết {timeoutMs} ms không có phản hồi");
            timeoutMs *= 2;                                                              // dòng 19: lùi theo cấp số nhân
        }
        throw new IOException($"Không có phản hồi sau {maxTries} lần");                  // dòng 21: NetError
    }

    static JsonObject Lookup(Socket s, IPEndPoint server, string term, string? cookie)
    {
        var req = new JsonObject { ["op"] = "lookup", ["term"] = term };
        if (cookie != null)
        {
            req["cookie"] = cookie;                                                      // gửi kèm cookie đã lưu
            return UdpRequest(s, server, req, 500, 4);
        }
        return UdpRequest(s, server, req, 500, 4, MIN_FIRST);                            // chưa có cookie: đệm yêu cầu
    }

    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args.Take(2).ToArray(), "127.0.0.1", 9100);
        string? cookie = args.Length > 2 ? args[2] : null;                               // cookie ban đầu (thêm)
        var server = new IPEndPoint(Dns.GetHostAddresses(host)[0], port);
        using var s = new Socket(server.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
        Console.WriteLine($"[client] Gửi yêu cầu tới {host}:{port} qua UDP. Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
        string? line;
        while ((line = Console.ReadLine()) != null)
        {
            string term = line.Trim();
            if (term.Length == 0) continue;
            Console.WriteLine($"[client] Tra cứu '{term}' ({(cookie != null ? "kèm cookie" : "không kèm cookie")})");
            JsonObject resp;
            try
            {
                resp = Lookup(s, server, term, cookie);
                if (resp["status"]?.GetValue<int>() == 401 && Json.Str(resp["cookie"]) is string c)
                {
                    cookie = c;                                                          // lưu mã thách thức
                    Console.WriteLine($"[client] Nhận thách thức 401, lưu cookie {cookie[..16]}…; gửi lại kèm cookie");
                    resp = Lookup(s, server, term, cookie);                              // gửi lại đúng một lần
                }
            }
            catch (IOException e)
            {
                Console.WriteLine($"[client] Lỗi: {e.Message}");
                continue;
            }
            if (resp["status"]?.GetValue<int>() == 200)
                Console.WriteLine($"[client] {Json.Str(resp["term"])}: {Json.Str(resp["text"])}");
            else
                Console.WriteLine($"[client] Lỗi {resp["status"]}: {Json.Str(resp["error"]) ?? "không được phục vụ"}");
        }
        s.Close();
        Console.WriteLine("[client] Đã đóng socket");
        return 0;
    }
}
