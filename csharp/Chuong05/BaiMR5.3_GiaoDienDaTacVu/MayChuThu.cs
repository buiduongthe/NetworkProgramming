using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR5_3;

// Máy chủ thử cục bộ cho ứng dụng giao diện đa tác vụ (chỉ chạy trong máy).
// Một tiến trình cung cấp ba dịch vụ giả lập (giống may_chu_thu.py của bản Python):
//   - HTTP  <port>/tcp : GET /tep?kb=<số KB>&tre=<ms> – gửi tệp <kb> KB theo từng khối 16 KB,
//                        nghỉ <tre> ms sau mỗi khối (giả lập đường truyền chậm, độ trễ khác nhau);
//   - "DNS" <port>/udp : nhận "A <tên>", trả "<tên> <địa chỉ>" hoặc "<tên> NXDOMAIN" sau 100–400 ms
//                        (bảng tên giả lập, không phải giao thức DNS thật – xem Bài 4.3);
//   - cổng mở <port+1>, <port+2>/tcp: chỉ nhận rồi đóng kết nối, để ứng dụng kiểm tra cổng.
// HTTP viết thẳng trên TcpListener (không dùng HttpListener, vốn cần quyền đăng ký URL trên Windows).
static class MayChuThu
{
    const int Chunk = 16 * 1024;
    static readonly Dictionary<string, (string Ip, int Ms)> Names = new()
    {
        ["router.lab"] = ("10.10.0.1", 100), ["may-in.lab"] = ("10.10.0.21", 250),
        ["camera-1.lab"] = ("10.10.0.31", 400), ["nas.lab"] = ("10.10.0.40", 150),
    };
    static readonly object outLock = new();

    static void Log(string text)
    {
        lock (outLock) Console.WriteLine($"[server] {text}");   // nhiều luồng cùng in: in nguyên dòng một lần
    }

    /// <summary>Phục vụ một kết nối HTTP: đọc dòng yêu cầu và tiêu đề, gửi tệp giả lập từng khối.</summary>
    static async Task HandleHttp(TcpClient client)
    {
        using (client)
        {
            var stream = client.GetStream();
            var reader = new StreamReader(stream, Encoding.ASCII);
            string? requestLine = await reader.ReadLineAsync();
            string? h;
            while (!string.IsNullOrEmpty(h = await reader.ReadLineAsync())) { }   // bỏ qua các tiêu đề
            var parts = (requestLine ?? "").Split(' ');
            string target = parts.Length > 1 ? parts[1] : "/";
            string path = target.Split('?')[0];
            var q = (target.Contains('?') ? target[(target.IndexOf('?') + 1)..] : "")
                .Split('&', StringSplitOptions.RemoveEmptyEntries)
                .Select(kv => kv.Split('=', 2)).Where(kv => kv.Length == 2)
                .ToDictionary(kv => kv[0], kv => kv[1]);
            if (parts[0] != "GET" || path != "/tep")
            {
                await stream.WriteAsync(Encoding.ASCII.GetBytes("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"));
                return;
            }
            int kb = int.Parse(q.GetValueOrDefault("kb", "256")), delay = int.Parse(q.GetValueOrDefault("tre", "50"));
            int size = kb * 1024;
            Log($"HTTP GET /tep {kb} KB, nghỉ {delay} ms mỗi khối");
            string head = $"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n";
            int sent = 0;
            try
            {
                await stream.WriteAsync(Encoding.ASCII.GetBytes(head));
                var block = Enumerable.Repeat((byte)'x', Chunk).ToArray();
                while (sent < size)
                {
                    int n = Math.Min(Chunk, size - sent);
                    await stream.WriteAsync(block.AsMemory(0, n));
                    sent += n;
                    if (sent < size) await Task.Delay(delay);       // khối cuối: không cần nghỉ
                }
            }
            catch (IOException)
            {
                Log($"HTTP: client đóng kết nối giữa chừng ({sent / 1024}/{kb} KB)");
            }
        }
    }

    static async Task DnsServer(string host, int port)
    {
        using var udp = new UdpClient(new IPEndPoint(IPAddress.Parse(host), port));
        while (true)
        {
            UdpReceiveResult r;
            try { r = await udp.ReceiveAsync(); }
            catch (SocketException) { continue; }        // Windows: ICMP "port unreachable" từ client đã đóng
            string name = Encoding.UTF8.GetString(r.Buffer).Trim();
            if (name.StartsWith("A ")) name = name[2..].Trim();
            var (ip, ms) = Names.GetValueOrDefault(name, ("NXDOMAIN", 300));
            _ = Task.Run(async () =>                     // mỗi câu hỏi trả lời riêng, sau độ trễ của tên đó
            {
                await Task.Delay(ms);
                await udp.SendAsync(Encoding.UTF8.GetBytes($"{name} {ip}"), r.RemoteEndPoint);
            });
        }
    }

    static async Task OpenPort(string host, int port)
    {
        var ls = new TcpListener(IPAddress.Parse(host), port);
        ls.Start();
        while (true)
        {
            using var c = await ls.AcceptTcpClientAsync();   // nhận rồi đóng ngay
        }
    }

    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int port = a.Length > 1 ? int.Parse(a[1]) : 9530;
        var http = new TcpListener(IPAddress.Parse(host), port);
        http.Start(64);
        _ = DnsServer(host, port);
        _ = OpenPort(host, port + 1);
        _ = OpenPort(host, port + 2);
        Log($"HTTP {host}:{port}/tcp, \"DNS\" {host}:{port}/udp, cổng mở {port + 1}, {port + 2}");
        var stop = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.Set(); };
        _ = Task.Run(async () =>
        {
            while (true)
            {
                var c = await http.AcceptTcpClientAsync();
                _ = HandleHttp(c);                       // mỗi kết nối một tác vụ bất đồng bộ
            }
        });
        stop.Wait();
        Log("dừng");
        return 0;
    }
}
