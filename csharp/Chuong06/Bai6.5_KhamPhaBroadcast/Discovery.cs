using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai6_5;

// Mã giả 6.7, hàm DISCOVER – phía client (Mục 6.4.2).
// Kế thừa Mã giả 4.4 (UDP_REQUEST): gửi một yêu cầu nhưng thu thập NHIỀU phản hồi đến hết thời gian chờ.
static class Discovery
{
    public const int DiscPort = 9999;                                    // CONST DISC_PORT ← 9999

    public static List<(string Ip, int Port, string Name)> Discover(int waitMs, int discPort, string bcast)
    {
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp); // SOCKET(INET, DGRAM)
        s.EnableBroadcast = true;                                        // SETOPT(BROADCAST): bắt buộc
        int id = Random.Shared.Next(1, 1000000);                         // NEXT_ID()
        var q = new JsonObject { ["type"] = "discover", ["service"] = "chat", ["id"] = id };
        s.SendTo(Encoding.UTF8.GetBytes(q.ToJsonString()), new IPEndPoint(IPAddress.Parse(bcast), discPort));
        Console.WriteLine($"[client] Phát discover id={id} tới {bcast}:{discPort}");
        Console.WriteLine($"[client] Thu thập phản hồi trong {waitMs} ms");
        var found = new List<(string, int, string)>();
        var clock = Stopwatch.StartNew();
        var buf = new byte[2048];
        while (clock.ElapsedMilliseconds < waitMs)
        {
            s.ReceiveTimeout = (int)Math.Max(1, waitMs - clock.ElapsedMilliseconds);   // ≠ 0 (0 = chờ vô hạn)
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            JsonObject? r;
            try
            {
                int n = s.ReceiveFrom(buf, ref from);                    // RECVFROM(s, 2048)
                r = JsonNode.Parse(Encoding.UTF8.GetString(buf, 0, n)) as JsonObject;
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)
            {
                break;                                                   // Timeout
            }
            catch (JsonException)
            {
                continue;                                                // sai định dạng: bỏ
            }
            if (r?["id"] is JsonValue v && v.TryGetValue(out int rid) && rid == id)
                found.Add((((IPEndPoint)from).Address.ToString(), (int)r["port"]!, (string)r["name"]!));
        }
        return found;                                                    // 'using' đóng socket (CLOSE)
    }
}
