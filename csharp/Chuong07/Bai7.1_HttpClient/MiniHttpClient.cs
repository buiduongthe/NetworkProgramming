using System.Net.Sockets;
using System.Text;

namespace Bai7_1;

// Mã giả 7.1 – Client HTTP/1.1 tối giản trên socket TCP: HTTP_REQUEST, READ_HEADERS, READ_CHUNKED
static class MiniHttpClient
{
    public static int Run(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        string[] paths = args.Length >= 3 ? args[2..] : ["/"];
        foreach (var path in paths)
        {
            var (status, hdr, data) = HttpRequest(host, port, "GET", path,
                new() { ["Accept"] = "application/json" }, null);
            Console.WriteLine($"[client] GET {path} -> {status}");
            string how = hdr.GetValueOrDefault("transfer-encoding", "").ToLowerInvariant().EndsWith("chunked")
                ? "Transfer-Encoding = chunked"
                : hdr.TryGetValue("content-length", out var cl) ? $"Content-Length = {cl}"
                : "đọc đến khi server đóng kết nối";
            Console.WriteLine($"[client] Định khung thân: {how}");
            Console.WriteLine($"[client] Thân ({data.Length} byte): {Encoding.UTF8.GetString(data)}");
        }
        return 0;
    }

    // Mã giả 7.1, dòng 1–23: gửi một yêu cầu HTTP/1.1 và đọc phản hồi
    public static (int Status, Dictionary<string, string> Hdr, byte[] Data) HttpRequest(
        string host, int port, string method, string path, Dictionary<string, string> headers, byte[]? body)
    {
        using Socket s = NetUtil.SafeConnect(host, port, 3000);
        s.ReceiveTimeout = 5000;
        var r = new Reader(s);
        var h = new StringBuilder($"{method} {path} HTTP/1.1\r\n");
        h.Append($"Host: {host}:{port}\r\n");                               // kèm cổng
        foreach (var (k, v) in headers) h.Append($"{k}: {v}\r\n");
        if (body != null) h.Append($"Content-Length: {body.Length}\r\n");
        h.Append("Connection: close\r\n\r\n");                              // dòng trống kết thúc tiêu đề
        byte[] req = [.. Encoding.ASCII.GetBytes(h.ToString()), .. body ?? []];
        int sent = 0;
        while (sent < req.Length) sent += s.Send(req, sent, req.Length - sent, SocketFlags.None); // SEND_ALL
        string line = r.ReadLine(8192) ?? throw new IOException("Server đóng kết nối trước khi gửi phản hồi");
        int status = int.Parse(line.Split(' ')[1]);
        var hdr = ReadHeaders(r);
        string te = hdr.GetValueOrDefault("transfer-encoding", "").ToLowerInvariant();
        byte[] data;
        if (te.EndsWith("chunked")) data = ReadChunked(r);                   // vd. "gzip, chunked"
        else if (hdr.TryGetValue("content-length", out var cl)) data = r.ReadExact(int.Parse(cl));
        else data = r.ReadUntilClose();
        return (status, hdr, data);                                         // 'using' đóng socket (CLOSE)
    }

    // Mã giả 7.1, dòng 25–34: đọc các trường tiêu đề đến dòng trống
    static Dictionary<string, string> ReadHeaders(Reader r)
    {
        var hdr = new Dictionary<string, string>();
        while (true)
        {
            string? line = r.ReadLine(8192);
            if (line == null)                                               // dòng 29: đóng giữa phần tiêu đề
                throw new ConnectionClosedException("Kết nối đóng khi chưa đọc hết phần tiêu đề");
            if (line == "") return hdr;                                     // dòng 30: dòng trống – hết
            int i = line.IndexOf(':');                                      // SPLIT_FIRST tại ':' đầu tiên
            string k = i >= 0 ? line[..i] : line, v = i >= 0 ? line[(i + 1)..] : "";
            hdr[k.Trim().ToLowerInvariant()] = v.Trim();                     // không phân biệt hoa/thường
        }
    }

    // Mã giả 7.1, dòng 35–45: giải mã thân Transfer-Encoding: chunked
    static byte[] ReadChunked(Reader r)
    {
        var data = new MemoryStream();
        while (true)
        {
            string sizeLine = r.ReadLine(1024) ?? throw new IOException("Thiếu dòng độ dài khối");
            int n = Convert.ToInt32(sizeLine.Split(';')[0].Trim(), 16);    // bỏ phần mở rộng sau ';'
            if (n == 0) break;
            data.Write(r.ReadExact(n));
            r.ReadLine(2);                                                  // CRLF sau mỗi khối
        }
        ReadHeaders(r);                                                     // bỏ phần trailer
        return data.ToArray();
    }
}
