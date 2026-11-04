using System.Net.Sockets;
using System.Text;

namespace Bai7_4;

// Mã giả 7.5 – SSE_READ; phần gửi GET và đọc tiêu đề kế thừa Mã giả 7.1
static class SseClient
{
    static string? lastEventId;
    static int? reconnectMs;

    public static int Run(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        string path = args.Length >= 3 ? args[2] : "/events";
        using var tcp = new TcpClient();
        tcp.ConnectAsync(host, port).Wait(3000);                        // SAFE_CONNECT (rút gọn)
        Socket s = tcp.Client;
        s.ReceiveTimeout = 30000;
        s.Send(Encoding.ASCII.GetBytes($"GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: text/event-stream\r\n" +
                                        "Cache-Control: no-cache\r\nConnection: close\r\n\r\n"));
        var r = new Reader((b, o, c) => s.Receive(b, o, c, SocketFlags.None));
        string status = r.ReadLine(8192) ?? throw new IOException("Server đóng kết nối");
        var hdr = r.ReadHeaders();
        Console.WriteLine($"[client] {status}; Content-Type: {hdr.GetValueOrDefault("content-type")}");
        if (status.Split(' ')[1] != "200") return 1;
        if (hdr.GetValueOrDefault("transfer-encoding", "").ToLowerInvariant().EndsWith("chunked"))
        {
            Console.WriteLine("[client] Thân dùng chunked: đặt bộ đọc dòng sau lớp giải mã khối");
            var chunked = new ChunkedSource(r);
            r = new Reader(chunked.Recv);
        }
        SseRead(r, (type, data) =>                                      // in '\n' dạng hai ký tự
            Console.WriteLine($"[client] Sự kiện '{type}': '{data.Replace("\n", "\\n")}'"));
        Console.WriteLine($"[client] Server đóng luồng; id cuối = {lastEventId}, retry = {reconnectMs} ms");
        return 0;
    }

    // Mã giả 7.5: r là Reader đặt sau phần tiêu đề
    static void SseRead(Reader r, Action<string, string> onEvent)
    {
        string type = "message"; var data = new List<string>();
        string? line;
        while ((line = r.ReadLine(65536)) != null)
        {
            if (line == "")                                             // dòng trống: hết một sự kiện
            {
                if (data.Count > 0) onEvent(type, string.Join("\n", data));
                type = "message"; data = [];
            }
            else if (!line.StartsWith(':'))                             // ":" mở đầu dòng chú thích
            {
                int i = line.IndexOf(':');
                string field = i >= 0 ? line[..i] : line, value = i >= 0 ? line[(i + 1)..] : "";
                if (value.StartsWith(' ')) value = value[1..];          // STRIP_ONE_SPACE
                switch (field)
                {
                    case "event": type = value; break;
                    case "data": data.Add(value); break;
                    case "id": lastEventId = value; break;
                    case "retry": reconnectMs = int.Parse(value); break;
                }
            }
        }                                                               // null: server đóng luồng
    }
}
