using System.Net;
using System.Net.Sockets;

namespace Bai3_1;

// Mã giả 3.1 – TCP Echo Server tuần tự: phục vụ lần lượt từng client, gửi trả nguyên văn dữ liệu nhận được
static class EchoServer
{
    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "0.0.0.0", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET(INET, STREAM)
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // SETOPT(REUSEADDR)
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(16);                                                                               // LISTEN, backlog 16
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        while (true)                                                    // vòng chấp nhận kết nối
        {
            using Socket conn = srv.Accept();                           // ACCEPT: chờ client mới
            var addr = conn.RemoteEndPoint;
            Console.WriteLine($"[server] Client kết nối: {addr}");
            try
            {
                HandleEcho(conn);
            }
            catch (SocketException e)                                   // lỗi của một client không dừng server
            {
                Console.WriteLine($"[server] Lỗi với {addr}: {e.SocketErrorCode}");
            }
            Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
        }                                                               // 'using' đóng conn (CLOSE) ở cuối mỗi vòng
    }

    // Vòng phục vụ một client: nhận bao nhiêu gửi trả bấy nhiêu
    static void HandleEcho(Socket conn)
    {
        var buf = new byte[4096];
        while (true)
        {
            int n = conn.Receive(buf);                                  // RECV: trả về 0 khi client đóng chiều gửi
            if (n == 0) break;
            NetUtil.SendAll(conn, buf, n);
        }
    }
}
