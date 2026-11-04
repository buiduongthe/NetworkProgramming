using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai4_2;

// Client tra cứu trên UDP dùng Mã giả 4.4 UDP_REQUEST: gửi lại có lùi thời gian, ghép mã định danh (Mục 4.3.1)
static class UdpLookupClient
{
    static long nextId = 0;
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static long Now() => Clock.ElapsedMilliseconds;                                      // NOW() tính bằng ms

    // Mã giả 4.4 – UDP_REQUEST; các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả
    static JsonObject UdpRequest(Socket s, IPEndPoint server, JsonObject req, int timeoutMs, int maxTries)
    {
        long id = ++nextId;
        req["id"] = id;                                                                  // dòng 2: NEXT_ID()
        byte[] payload = Json.ToBytes(req);                                              // dòng 3
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

    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "127.0.0.1", 9100);
        var server = new IPEndPoint(Dns.GetHostAddresses(host)[0], port);
        using var s = new Socket(server.AddressFamily, SocketType.Dgram, ProtocolType.Udp);
        Console.WriteLine($"[client] Gửi yêu cầu tới {host}:{port} qua UDP (chờ 500 ms, tối đa 4 lần). "
                          + "Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
        string? line;
        while ((line = Console.ReadLine()) != null)                                      // READ_CONSOLE()
        {
            string term = line.Trim();
            if (term.Length == 0) continue;
            Console.WriteLine($"[client] Tra cứu '{term}'");
            long t0 = Now();
            JsonObject resp;
            try
            {
                resp = UdpRequest(s, server, new JsonObject { ["op"] = "lookup", ["term"] = term }, 500, 4);
            }
            catch (IOException e)
            {
                Console.WriteLine($"[client] Lỗi: {e.Message} ({Now() - t0} ms)");
                continue;
            }
            if (resp["status"]?.GetValue<int>() == 200)
                Console.WriteLine($"[client] {Json.Str(resp["term"])}: {Json.Str(resp["text"])} ({Now() - t0} ms)");
            else
                Console.WriteLine($"[client] Lỗi {resp["status"]}: {Json.Str(resp["error"])} ({Now() - t0} ms)");
        }
        s.Close();
        Console.WriteLine("[client] Đã đóng socket");
        return 0;
    }
}
