using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR3_1;

// Server đích để thử bộ chuyển tiếp cổng – dịch vụ "chữ hoa": đọc toàn bộ dữ liệu tới khi client
// đóng chiều gửi (Receive trả 0), rồi gửi lại bản viết hoa và đóng kết nối. Vì server chỉ trả lời
// SAU KHI thấy EOF, bộ chuyển tiếp ở giữa bắt buộc phải chuyển tiếp đúng việc đóng một chiều
// (half-close); nếu không, client và server sẽ chờ nhau mãi.
static class ServerDich
{
    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int port = a.Length > 1 ? int.Parse(a[1]) : 9000;
        var srv = new TcpListener(IPAddress.Parse(host), port);
        srv.Start();
        Console.WriteLine($"[dich] Server chữ hoa lắng nghe tại {host}:{port}");
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; srv.Stop(); };
        try
        {
            while (true)
            {
                Socket conn = srv.AcceptSocket();
                new Thread(() => Handle(conn)) { IsBackground = true }.Start();
            }
        }
        catch (SocketException) { }
        catch (ObjectDisposedException) { }
        Console.WriteLine("[dich] Dừng server");
        return 0;
    }

    static void Handle(Socket conn)
    {
        using (conn)
        {
            var addr = (IPEndPoint)conn.RemoteEndPoint!;
            var body = new MemoryStream();
            var buf = new byte[65536];
            int n;
            while ((n = conn.Receive(buf)) > 0)            // 0 = client đã Shutdown(Send)
                body.Write(buf, 0, n);
            byte[] reply = Encoding.UTF8.GetBytes(Encoding.UTF8.GetString(body.ToArray()).ToUpperInvariant());
            int sent = 0;
            while (sent < reply.Length) sent += conn.Send(reply, sent, reply.Length - sent, SocketFlags.None);
            Console.WriteLine($"[dich] {addr.Address}:{addr.Port} gửi {body.Length} byte, trả {reply.Length} byte chữ hoa");
        }
    }
}
