using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Bai4_1;

// Mã giả 4.2 – UDP Echo Client có thời gian chờ 2 giây (Mục 4.2.1)
static class UdpEchoClient
{
    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "127.0.0.1", 9000);                     // dòng 2
        var server = new IPEndPoint(Dns.GetHostAddresses(host)[0], port);                // dòng 3: (FIRST(RESOLVE(host)), port)
        using var s = new Socket(server.AddressFamily, SocketType.Dgram, ProtocolType.Udp); // dòng 4
        s.ReceiveTimeout = 2000;                                                         // dòng 5: RECV_TIMEOUT 2000 ms
        Console.WriteLine($"[client] Gửi tới {host}:{port} qua UDP. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
        var buf = new byte[65535];
        string? line;
        while ((line = Console.ReadLine()) != null)                                      // dòng 6: READ_CONSOLE()
        {
            s.SendTo(Encoding.UTF8.GetBytes(line), server);                              // dòng 7: SENDTO
            try
            {
                EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
                int n = s.ReceiveFrom(buf, ref addr);                                    // dòng 9: RECVFROM
                Console.WriteLine($"[client] Gửi: '{line}' -> Nhận: '{Encoding.UTF8.GetString(buf, 0, n)}'");
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut
                                         || e.SocketErrorCode == SocketError.ConnectionReset)
            {   // dòng 11: CATCH Timeout, ConnectionReset (Windows báo ICMP port unreachable, Mục 4.2.2)
                Console.WriteLine($"[client] Gửi: '{line}' -> Không có phản hồi: datagram có thể đã mất");
            }
        }
        s.Close();                                                                       // dòng 15: CLOSE
        Console.WriteLine("[client] Đã đóng socket");
        return 0;
    }
}
