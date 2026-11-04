using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;

namespace Bai7_2;

// Mã giả 7.2 – Server REST tối giản: HTTP_SESSION, ROUTE, RESPOND (kế thừa Mã giả 6.3: mỗi kết nối một tác vụ)
// Bổ sung theo bài tập cuối chương: POST /api/v1/terms (201 + Location), DELETE /api/v1/terms/{term} (204)
static class RestServer
{
    const int MaxBody = 1048576;                                        // 1 MiB, như MAX_FRAME (Mã giả 3.6)
    static readonly Dictionary<string, string> Dict = JsonSerializer.Deserialize<Dictionary<string, string>>(
        File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "glossary.json")))!;   // DICT (Mã giả 3.7)
    static readonly JsonSerializerOptions Json = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly Dictionary<int, string> Reason = new()
    {
        [200] = "OK", [201] = "Created", [204] = "No Content", [400] = "Bad Request", [404] = "Not Found",
        [405] = "Method Not Allowed", [409] = "Conflict", [413] = "Content Too Large", [501] = "Not Implemented",
    };

    public static async Task<int> RunAsync(string[] args)
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
            Socket conn = await srv.AcceptAsync();                     // ACCEPT_ASYNC
            _ = HttpSession(conn);                                      // RUN_BACKGROUND: không chờ
        }
    }

    static object Problem(int status, string title) => new { type = "about:blank", title, status };

    // Mã giả 7.2, dòng 1–22: phiên keep-alive, nhiều yêu cầu trên một kết nối
    static async Task HttpSession(Socket conn)
    {
        string addr = conn.RemoteEndPoint!.ToString()!;
        await using var ns = new NetworkStream(conn, ownsSocket: true);
        var r = new AsyncReader(ns);
        try
        {
            while (true)
            {
                string? line = await r.ReadLineAsync(8192);
                if (line == null) return;                               // client đóng kết nối
                string[] parts = line.Split(' ');
                if (parts.Length != 3) { await Respond(ns, 400, Problem(400, "Dòng yêu cầu sai")); return; }
                var (method, target) = (parts[0], parts[1]);
                var hdr = await r.ReadHeadersAsync();
                long n = long.Parse(hdr.GetValueOrDefault("content-length", "0"));   // dòng 8: TO_INT64, có thể > 4 GB
                if (hdr.ContainsKey("transfer-encoding"))               // chưa hỗ trợ chunked
                {
                    Console.WriteLine($"[server] {addr} {method} {target} -> 501");
                    await Respond(ns, 501, Problem(501, "Chưa hỗ trợ"));
                    return;                                             // độ dài thân không xác định: đóng
                }
                if (n < 0 || n > MaxBody)                               // dòng 13
                {
                    Console.WriteLine($"[server] {addr} {method} {target} -> 413");
                    await Respond(ns, 413, Problem(413, "Thân quá lớn"));
                    return;
                }
                byte[] body = await r.ReadExactAsync((int)n);
                var (status, obj, location) = Route(method, target.Split('?')[0], body);   // PATH(target)
                Console.WriteLine($"[server] {addr} {method} {target} -> {status}");
                await Respond(ns, status, obj, location);
                if (hdr.GetValueOrDefault("connection", "").ToLowerInvariant() == "close") return;
            }
        }
        catch (Exception e) when (e is IOException or InvalidDataException or FormatException or OverflowException)
        {
            Console.WriteLine($"[server] Kết thúc phiên {addr}: {e.Message}");
        }
        finally
        {
            Console.WriteLine($"[server] Đóng kết nối {addr}");     // 'await using' đóng luồng và socket
        }
    }

    // Mã giả 7.2, dòng 23–37 (thêm POST, DELETE); lock vì các tác vụ chạy song song trên nhóm luồng
    static (int, object?, string?) Route(string method, string path, byte[] body)
    {
        string[] p = path.Trim('/').Split('/');                         // ["api","v1","terms",…]
        if (p.Length < 3 || p[0] != "api" || p[1] != "v1" || p[2] != "terms" || p.Length > 4)
            return (404, Problem(404, "Không có tài nguyên"), null);
        lock (Dict)
        {
            if (method == "GET" && p.Length == 3) return (200, new { terms = Dict.Keys.ToArray() }, null);
            if (method == "GET" && p.Length == 4)
            {
                string t = Uri.UnescapeDataString(p[3]);                // URL_DECODE: %20 -> dấu cách
                return Dict.TryGetValue(t, out var text)
                    ? (200, new { term = t, text }, null)
                    : (404, Problem(404, "Không có thuật ngữ"), null);
            }
            if (method == "POST" && p.Length == 3)                      // mở rộng: thêm thuật ngữ
            {
                string? t, text;
                try
                {
                    using var doc = JsonDocument.Parse(body);
                    t = doc.RootElement.GetProperty("term").GetString();
                    text = doc.RootElement.GetProperty("text").GetString();
                }
                catch (Exception e) when (e is JsonException or KeyNotFoundException or InvalidOperationException)
                { t = text = null; }
                if (t == null || text == null) return (400, Problem(400, "JSON sai hoặc thiếu trường term, text"), null);
                if (Dict.ContainsKey(t)) return (409, Problem(409, "Thuật ngữ đã tồn tại"), null);
                Dict[t] = text;
                return (201, new { term = t, text }, "/api/v1/terms/" + Uri.EscapeDataString(t));
            }
            if (method == "DELETE" && p.Length == 4)                    // mở rộng: xóa thuật ngữ
                return Dict.Remove(Uri.UnescapeDataString(p[3]))
                    ? (204, null, null) : (404, Problem(404, "Không có thuật ngữ"), null);
        }
        return (405, Problem(405, "Phương thức không hỗ trợ"), null);
    }

    // Mã giả 7.2, dòng 38–48: Content-Length là số byte sau khi mã hóa UTF-8
    static async Task Respond(Stream ns, int status, object? obj, string? location = null)
    {
        var h = new StringBuilder($"HTTP/1.1 {status} {Reason[status]}\r\n");
        if (location != null) h.Append($"Location: {location}\r\n");
        byte[] b = [];
        if (status != 204)                                              // dòng 40–41: 204 không thân, không Content-Length
        {
            b = Encoding.UTF8.GetBytes(JsonSerializer.Serialize(obj, Json));
            string ct = status >= 400 ? "application/problem+json" : "application/json";
            h.Append($"Content-Type: {ct}; charset=utf-8\r\n");
            h.Append($"Content-Length: {b.Length}\r\n");
        }
        h.Append("\r\n");
        await ns.WriteAsync((byte[])[.. Encoding.ASCII.GetBytes(h.ToString()), .. b]);   // SEND_ALL_ASYNC
    }
}
