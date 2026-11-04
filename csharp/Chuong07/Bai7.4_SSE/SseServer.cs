using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Bai7_4;

// Server SSE nhỏ: phát luồng text/event-stream bằng chunked; một sự kiện cố ý bị cắt giữa dòng qua hai khối.
// Mỗi kết nối một luồng (Mã giả 6.1)
static class SseServer
{
    static readonly string[] Events =
    [
        ": nhip tim\n\n",
        "retry: 3000\n\n",
        "id: 1\nevent: delta\ndata: {\"text\":\"Giao thức\"}\n\n",
        "id: 2\nevent: delta\ndata:{\"text\":\" TCP\"}\n\n",             // không có dấu cách sau ':' cũng hợp lệ
        "id: 3\ndata: dòng thứ nhất\ndata: dòng th",                    // sự kiện bị cắt giữa dòng ...
        "ứ hai\n\n",                                                    // ... và kết thúc ở khối sau
        "event: done\ndata: [DONE]\n\n",
    ];

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
            Socket conn = srv.Accept();
            new Thread(() => Serve(conn)) { IsBackground = true }.Start();
        }
    }

    static void Serve(Socket conn)
    {
        string addr = conn.RemoteEndPoint!.ToString()!;
        using (conn)
        {
            try
            {
                var r = new Reader((b, o, c) => conn.Receive(b, o, c, SocketFlags.None));
                string? line = r.ReadLine(8192);
                if (line == null) return;
                r.ReadHeaders();
                if (line.Split(' ')[1] != "/events")
                {
                    conn.Send("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"u8.ToArray());
                    return;
                }
                Console.WriteLine($"[server] Client {addr} đăng ký luồng: {line}");
                conn.Send("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n"u8.ToArray());
                conn.Send("Cache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n"u8.ToArray());
                for (int i = 0; i < Events.Length; i++)
                {
                    byte[] b = Encoding.UTF8.GetBytes(Events[i]);       // một khối: độ dài hex, CRLF, dữ liệu, CRLF
                    conn.Send([.. Encoding.ASCII.GetBytes($"{b.Length:X}\r\n"), .. b, (byte)'\r', (byte)'\n']);
                    Console.WriteLine($"[server] Gửi khối {i + 1} ({b.Length} byte): '{Events[i].Replace("\n", "\\n")}'");
                    Thread.Sleep(300);                                  // sự kiện đến dần theo thời gian
                }
                conn.Send("0\r\n\r\n"u8.ToArray());                     // khối 0: kết thúc thân
                Console.WriteLine($"[server] Kết thúc luồng cho {addr}");
            }
            catch (SocketException e) { Console.WriteLine($"[server] Client {addr} ngắt kết nối: {e.SocketErrorCode}"); }
        }
    }
}
