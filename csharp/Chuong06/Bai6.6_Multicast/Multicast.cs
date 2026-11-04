using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Unicode;

namespace Bai6_6;

// Mã giả 6.8 – Phát và nhận thông báo dịch vụ qua multicast (Mục 6.4.3).
// Kế thừa Mã giả 6.7: thay địa chỉ broadcast bằng địa chỉ nhóm 239.255.42.99 (dải quản trị cục bộ).
// Tham số count của ANNOUNCER và run_ms của LISTENER được bổ sung để minh họa (0 = chạy mãi).
static class Multicast
{
    public const string Group = "239.255.42.99";                         // CONST GROUP
    public const int MPort = 9998;                                       // CONST MPORT
    static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.Create(UnicodeRanges.All) };
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static long NowMs => Clock.ElapsedMilliseconds;

    public static void Announcer(int servicePort, string name, int periodMs, int count, int mport)
    {
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);       // SOCKET(INET, DGRAM)
        s.SetSocketOption(SocketOptionLevel.IP, SocketOptionName.MulticastTimeToLive, 1);             // chỉ trong mạng con
        s.SetSocketOption(SocketOptionLevel.IP, SocketOptionName.MulticastLoopback, true);            // nhận cả trên máy gửi
        var dest = new IPEndPoint(IPAddress.Parse(Group), mport);
        Console.WriteLine($"[announcer] Phát tới nhóm {Group}:{mport}, chu kỳ {periodMs} ms");
        int n = 0;
        while (count == 0 || n < count)
        {
            var a = new JsonObject { ["type"] = "announce", ["name"] = name, ["port"] = servicePort, ["ttl_ms"] = 3 * periodMs };
            s.SendTo(Encoding.UTF8.GetBytes(a.ToJsonString(Opts)), dest);
            n++;
            Console.WriteLine($"[announcer] Lần {n}: announce {name}, cổng {servicePort}, ttl_ms={3 * periodMs}");
            Thread.Sleep(periodMs + Random.Shared.Next(0, periodMs / 10 + 1));   // có nhiễu
        }
        Console.WriteLine($"[announcer] Dừng phát sau {n} lần");
    }

    public static void Listener(Dictionary<string, (string Ip, int Port, long Expires)> cache, int runMs, int mport)
    {
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);       // SOCKET(INET, DGRAM)
        s.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);             // nhiều tiến trình cùng nghe
        s.Bind(new IPEndPoint(IPAddress.Any, mport));                                                 // BIND 0.0.0.0
        s.SetSocketOption(SocketOptionLevel.IP, SocketOptionName.AddMembership,
                          new MulticastOption(IPAddress.Parse(Group), IPAddress.Any));                // IGMP join, ANY_INTERFACE
        Console.WriteLine($"[listener] Tham gia nhóm {Group}, nghe cổng {mport}");
        long end = NowMs + runMs;
        var buf = new byte[2048];
        while (runMs == 0 || NowMs < end)
        {
            if (runMs > 0) s.ReceiveTimeout = (int)Math.Max(1, end - NowMs);
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            JsonObject? a;
            try
            {
                int n = s.ReceiveFrom(buf, ref from);                    // RECVFROM(s, 2048)
                a = JsonNode.Parse(Encoding.UTF8.GetString(buf, 0, n)) as JsonObject;
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut) { break; }
            catch (JsonException) { continue; }                          // sai định dạng: bỏ
            if (a?["type"]?.ToString() == "announce")
            {
                string name = (string)a["name"]!;
                string ip = ((IPEndPoint)from).Address.ToString();
                if (!cache.ContainsKey(name)) Console.WriteLine($"[listener] Thêm {name} tại {ip}:{a["port"]}");
                cache[name] = (ip, (int)a["port"]!, NowMs + (long)a["ttl_ms"]!);
            }
            RemoveExpired(cache, NowMs);                                 // dịch vụ hết hạn: xóa
        }
    }

    // REMOVE_EXPIRED: xóa dịch vụ đã quá hạn ttl_ms
    static void RemoveExpired(Dictionary<string, (string Ip, int Port, long Expires)> cache, long now)
    {
        foreach (var name in cache.Where(kv => kv.Value.Expires < now).Select(kv => kv.Key).ToList())
        {
            cache.Remove(name);
            Console.WriteLine($"[listener] Hết hạn, xóa: {name}");
        }
    }
}
