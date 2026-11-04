using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Bai7_1;

// Server HTTP thử nghiệm cho client tối giản (khung server tuần tự của Mã giả 3.1), ba kiểu định khung thân:
//   /api/v1/terms/TCP -> Content-Length;  /chunked -> chunked (có mở rộng khối, trailer);
//   /dong-ket-noi -> không có độ dài, đóng kết nối;  đường dẫn khác -> 404
static class ServerThu
{
    public static int Run(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "0.0.0.0";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        while (true)
        {
            using Socket conn = srv.Accept();
            var r = new Reader(conn);
            string? line = r.ReadLine(8192);                                // dòng yêu cầu
            if (line == null) continue;
            while (!string.IsNullOrEmpty(r.ReadLine(8192))) { }              // bỏ qua tiêu đề đến dòng trống
            string how = Respond(conn, line.Split(' ')[1]);
            Console.WriteLine($"[server] {line} -> định khung: {how}");
        }
    }

    static string Respond(Socket conn, string path)
    {
        var u8 = Encoding.UTF8;
        if (path == "/api/v1/terms/TCP")
        {
            byte[] body = u8.GetBytes("{\"term\":\"TCP\",\"text\":\"Giao thức vận chuyển hướng kết nối\"}");
            Send(conn, "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n" +
                       $"Content-Length: {body.Length}\r\n\r\n", body);
            return "Content-Length";
        }
        if (path == "/chunked")
        {
            var ms = new MemoryStream();
            string[] parts = ["Thân phản hồi ", "được chia thành ", "nhiều khối."];
            for (int i = 0; i < parts.Length; i++)
            {
                byte[] b = u8.GetBytes(parts[i]);
                string ext = i == 0 ? ";vi-du=mo-rong" : "";                // phần mở rộng khối
                ms.Write(Encoding.ASCII.GetBytes($"{b.Length:X}{ext}\r\n"));
                ms.Write(b); ms.Write("\r\n"u8);
            }
            ms.Write("0\r\nX-So-Khoi: 3\r\n\r\n"u8);                         // khối 0, trailer, dòng trống
            Send(conn, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n" +
                       "Transfer-Encoding: chunked\r\nTrailer: X-So-Khoi\r\n\r\n", ms.ToArray());
            return "chunked";
        }
        if (path == "/dong-ket-noi")
        {
            Send(conn, "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n",
                 u8.GetBytes("Không có Content-Length: client đọc đến khi kết nối đóng."));
            return "đóng kết nối";
        }
        byte[] nf = u8.GetBytes("{\"type\":\"about:blank\",\"title\":\"Not Found\",\"status\":404}");
        Send(conn, "HTTP/1.1 404 Not Found\r\nContent-Type: application/problem+json\r\n" +
                   $"Content-Length: {nf.Length}\r\n\r\n", nf);
        return "Content-Length";
    }

    static void Send(Socket conn, string head, byte[] body)
    {
        byte[] all = [.. Encoding.ASCII.GetBytes(head), .. body];
        int sent = 0;
        while (sent < all.Length) sent += conn.Send(all, sent, all.Length - sent, SocketFlags.None);
    }
}
