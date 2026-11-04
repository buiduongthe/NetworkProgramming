using System.Net.Sockets;
using System.Text;

namespace Bai7_2;

// Client thử server REST: gửi nhiều yêu cầu trên MỘT kết nối giữ lại (keep-alive)
// Mỗi dòng nhập: PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON], ví dụ  POST /api/v1/terms {"term":"QUIC","text":"..."}
static class RestClient
{
    public static async Task<int> RunAsync(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        using var tcp = new TcpClient();
        using (var cts = new CancellationTokenSource(3000)) await tcp.ConnectAsync(host, port, cts.Token);  // SAFE_CONNECT (rút gọn)
        var ns = tcp.GetStream();
        ns.ReadTimeout = 5000;
        var r = new AsyncReader(ns);
        Console.WriteLine($"[client] Đã kết nối {host}:{port} (một kết nối cho mọi yêu cầu)");
        string? input;
        while ((input = Console.ReadLine()) != null)
        {
            string[] parts = input.Trim().Split(' ', 3);
            if (parts.Length < 2) continue;
            var (method, path) = (parts[0], parts[1]);
            byte[] body = parts.Length == 3 ? Encoding.UTF8.GetBytes(parts[2]) : [];
            var h = new StringBuilder($"{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json\r\n");
            if (body.Length > 0) h.Append($"Content-Type: application/json\r\nContent-Length: {body.Length}\r\n");
            h.Append("\r\n");
            await ns.WriteAsync((byte[])[.. Encoding.ASCII.GetBytes(h.ToString()), .. body]);
            string? statusLine = await r.ReadLineAsync(8192);
            if (statusLine == null) { Console.WriteLine("[client] Server đã đóng kết nối"); break; }
            var hdr = await r.ReadHeadersAsync();
            byte[] data = await r.ReadExactAsync(int.Parse(hdr.GetValueOrDefault("content-length", "0")));
            Console.WriteLine($"[client] {method} {path} -> {statusLine}");
            if (hdr.TryGetValue("location", out var loc)) Console.WriteLine($"[client]   Location: {loc}");
            if (data.Length > 0) Console.WriteLine($"[client]   {Encoding.UTF8.GetString(data)}");
        }
        Console.WriteLine("[client] Đã đóng kết nối");
        return 0;
    }
}
