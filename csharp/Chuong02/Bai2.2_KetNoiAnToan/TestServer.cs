using System.Net;
using System.Net.Sockets;

namespace Bai2_2;

// Server thử nghiệm tối giản: chấp nhận kết nối, in địa chỉ client rồi đóng ngay.
// Chỉ dùng để tạo trường hợp kết nối thành công; chuỗi SOCKET – BIND – LISTEN – ACCEPT trình bày ở Chương 3.
static class TestServer
{
    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "0.0.0.0", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET(INET, STREAM)
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // SETOPT(REUSEADDR)
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(16);                                                                               // LISTEN
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        while (true)
        {
            using Socket conn = srv.Accept();                                                         // ACCEPT
            Console.WriteLine($"[server] Client kết nối: {conn.RemoteEndPoint}");
        }                                                                                             // 'using' = CLOSE ngay
    }
}
