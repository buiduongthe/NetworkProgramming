using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;

namespace BaiMR7_1;

// Server HTTP phục vụ tệp có hỗ trợ tải từng phần (Range, RFC 9110), viết trực tiếp trên TcpListener để thấy
// rõ từng dòng tiêu đề. Phục vụ một tệp nhị phân sinh sẵn tại /tep.bin:
//   - HEAD: trả Content-Length, Accept-Ranges: bytes, ETag, X-Checksum-SHA256.
//   - GET có "Range: bytes=a-" hoặc "bytes=a-b": trả 206 Partial Content + Content-Range;
//     dải không thỏa mãn được: 416 + "Content-Range: bytes */<kích thước>".
//   - If-Range: <ETag>: nếu ETag không khớp (tệp đã đổi) thì bỏ qua Range, trả 200 toàn bộ.
//   - Giả lập sự cố mạng: --so-lan-cat N yêu cầu GET đầu tiên bị ngắt kết nối sau --cat-sau byte.
static class ServerTep
{
    static byte[] data = [];
    static string etag = "", sha = "";
    static int counter, cutsLeft, catSau;
    static readonly object khoa = new();
    static readonly Dictionary<int, string> Reason = new()
        { [200] = "OK", [206] = "Partial Content", [404] = "Not Found", [416] = "Requested Range Not Satisfiable" };

    static void Log(string text) => Console.WriteLine($"[server] {text}");

    /// <summary>Nội dung tệp cố định (giống bản Python, Go) để mọi bản cho cùng SHA-256.</summary>
    static byte[] MakeData(int size) => Enumerable.Range(0, size).Select(i => (byte)((i * 31 + i / 7) % 256)).ToArray();

    static void Write(Stream s, string text) => s.Write(Encoding.ASCII.GetBytes(text));

    static void CommonHeaders(Stream s, int status, long length, Dictionary<string, string>? extra = null)
    {
        var sb = new StringBuilder($"HTTP/1.1 {status} {Reason[status]}\r\n");
        sb.Append("Content-Type: application/octet-stream\r\n");
        sb.Append($"Content-Length: {length}\r\n");
        sb.Append("Accept-Ranges: bytes\r\n");
        sb.Append($"ETag: {etag}\r\n");
        sb.Append("Connection: close\r\n");
        foreach (var (k, v) in extra ?? []) sb.Append($"{k}: {v}\r\n");
        sb.Append("\r\n");
        Write(s, sb.ToString());
    }

    static void Handle(Socket conn)
    {
        using (conn)
        using (var stream = new NetworkStream(conn))
        {
            var reader = new StreamReader(stream, Encoding.ASCII);
            var parts = (reader.ReadLine() ?? "").Split(' ');
            var headers = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);   // tên tiêu đề không phân biệt hoa thường
            string? h;
            while (!string.IsNullOrEmpty(h = reader.ReadLine()))
            {
                int i = h.IndexOf(':');
                if (i > 0) headers[h[..i].Trim()] = h[(i + 1)..].Trim();
            }
            if (parts.Length < 2) return;
            string method = parts[0], path = parts[1];
            int k = Interlocked.Increment(ref counter);
            if (path != "/tep.bin" || (method != "GET" && method != "HEAD"))
            {
                CommonHeaders(stream, 404, 0);
                return;
            }
            int size = data.Length;
            if (method == "HEAD")
            {
                CommonHeaders(stream, 200, size, new() { ["X-Checksum-SHA256"] = sha });
                Log($"#{k} HEAD -> 200, {size} byte");
                return;
            }
            headers.TryGetValue("Range", out var rng);
            headers.TryGetValue("If-Range", out var ifRange);
            int start = 0, end = size - 1, status = 200;
            if (rng != null && (ifRange == null || ifRange == etag))   // If-Range khớp mới dùng Range
            {
                var m = Regex.Match(rng.Trim(), @"^bytes=(\d+)-(\d*)$");
                if (m.Success)
                {
                    start = (int)Math.Min(long.Parse(m.Groups[1].Value), int.MaxValue);
                    end = m.Groups[2].Value != "" ? (int)Math.Min(long.Parse(m.Groups[2].Value), size - 1) : size - 1;
                    if (start >= size || start > end)
                    {
                        CommonHeaders(stream, 416, 0, new() { ["Content-Range"] = $"bytes */{size}" });
                        Log($"#{k} GET Range={rng} -> 416");
                        return;
                    }
                    status = 206;
                }
            }
            var extra = status == 206 ? new Dictionary<string, string> { ["Content-Range"] = $"bytes {start}-{end}/{size}" } : null;
            int len = end - start + 1;
            CommonHeaders(stream, status, len, extra);
            bool cut;
            lock (khoa)
            {
                cut = cutsLeft > 0;
                if (cut) cutsLeft--;
            }
            if (cut)                                               // giả lập đứt kết nối giữa chừng
            {
                stream.Write(data, start, Math.Min(catSau, len));
                stream.Flush();
                conn.Shutdown(SocketShutdown.Both);
                Log($"#{k} GET Range={rng ?? "-"} -> {status}, NGẮT sau {Math.Min(catSau, len)} byte");
                return;
            }
            stream.Write(data, start, len);
            Log($"#{k} GET Range={rng ?? "-"} -> {status}, gửi {len} byte");
        }
    }

    public static int Run(string[] args)
    {
        var pos = new List<string>();
        var o = new Dictionary<string, int> { ["--kich-thuoc"] = 300000, ["--cat-sau"] = 100000, ["--so-lan-cat"] = 0 };
        for (int i = 0; i < args.Length; i++)
        {
            if (o.ContainsKey(args[i]) && i + 1 < args.Length) o[args[i]] = int.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        string host = pos.Count > 0 ? pos[0] : "127.0.0.1";
        int port = pos.Count > 1 ? int.Parse(pos[1]) : 8000;
        data = MakeData(o["--kich-thuoc"]);
        sha = Convert.ToHexStringLower(SHA256.HashData(data));
        etag = $"\"{sha[..16]}\"";
        cutsLeft = o["--so-lan-cat"];
        catSau = o["--cat-sau"];
        var srv = new TcpListener(IPAddress.Parse(host), port);
        srv.Start();
        Log($"http://{host}:{port}/tep.bin, {data.Length} byte, ETag {etag}");
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; srv.Stop(); };
        try
        {
            while (true)
            {
                Socket c = srv.AcceptSocket();
                new Thread(() =>
                {
                    try { Handle(c); }
                    catch (IOException) { }                        // client đóng giữa chừng
                }) { IsBackground = true }.Start();
            }
        }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
        Log("Dừng server");
        return 0;
    }
}
