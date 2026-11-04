using System.Net;
using System.Net.Sockets;

namespace Bai4_1;

// Mã giả 4.1 – UDP Echo Server: bỏ LISTEN/ACCEPT, một socket cho mọi client (Mục 4.2.1)
static class UdpEchoServer
{
    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "0.0.0.0", 9000);                       // dòng 2
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp); // dòng 3: SOCKET(INET, DGRAM)
        s.Bind(new IPEndPoint(IPAddress.Parse(host), port));                             // dòng 4: BIND
        Console.WriteLine($"[server] Đang lắng nghe UDP tại {host}:{port}");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");  // Ctrl+C
        var buf = new byte[65535];
        while (true)                                                                     // dòng 5: LOOP
        {
            EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try
            {
                n = s.ReceiveFrom(buf, ref addr);                                        // dòng 6: datagram nguyên vẹn
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset)
            {
                continue;                                    // Windows: ICMP port unreachable (Mục 4.2.2)
            }
            Console.WriteLine($"[server] Nhận {n} byte từ {addr}");                     // dòng 7
            s.SendTo(buf, 0, n, SocketFlags.None, addr);                                 // dòng 8: trả lời đúng người gửi
        }
    }
}
