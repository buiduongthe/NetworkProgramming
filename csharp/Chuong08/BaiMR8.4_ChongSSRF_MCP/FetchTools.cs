using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace BaiMR8_4;

// Bài mở rộng MR8.4 – Thân công cụ fetch_url: bản NGÂY THƠ và bản AN TOÀN (chống SSRF).
//
// Dùng bởi McpServer. SSRF (Server-Side Request Forgery) là lỗ hổng khi server nhận một URL từ bên ngoài
// rồi tự đi tải URL đó: kẻ tấn công ép server truy cập dịch vụ nội bộ (máy chủ siêu dữ liệu đám mây
// 169.254.169.254, dịch vụ chỉ nghe loopback, mạng riêng 10/8…) mà client bên ngoài không với tới.
//
// Hai bộ bảng được "tiêm" (inject) qua biến môi trường để demo chạy hoàn toàn offline, tất định:
//   SSRF_RESOLVER = {"tên miền": ["ip1", "ip2", ...]}  – bảng phân giải tên giả lập; nếu danh sách có
//                   nhiều IP thì mỗi lần gọi trả IP kế tiếp (mô phỏng DNS rebinding / TOCTOU).
//   SSRF_DIAL     = {"ip": cổng}                        – nối tới 127.0.0.1:cổng thay cho ip thật.
// Hai bảng này CHỈ tồn tại cho bản demo offline; thực tế dùng DNS thật và nối thẳng tới IP đã kiểm tra.

/// <summary>URL bị chính sách an toàn từ chối (trả về mô hình với isError = true).</summary>
sealed class SsrfException(string msg) : Exception(msg);

/// <summary>Lỗi khi thực thi (không nối được, phản hồi hỏng…).</summary>
sealed class FetchException(string msg) : Exception(msg);

static class FetchTools
{
    // Dải địa chỉ BỊ CHẶN (liệt kê tường minh, KHÔNG dựa vào is_private vì dải tài liệu cũng là private).
    static readonly (byte[] Net, int Prefix, string Text)[] BlockedNets =
    [
        Cidr("0.0.0.0/8"),      // địa chỉ "không xác định" / mạng này
        Cidr("127.0.0.0/8"),    // loopback
        Cidr("10.0.0.0/8"),     // RFC 1918
        Cidr("172.16.0.0/12"),  // RFC 1918
        Cidr("192.168.0.0/16"), // RFC 1918
        Cidr("169.254.0.0/16"), // link-local, gồm 169.254.169.254 (siêu dữ liệu đám mây)
        Cidr("::1/128"),        // loopback IPv6
        Cidr("::/128"),         // không xác định IPv6
        Cidr("fc00::/7"),       // IPv6 unique-local
        Cidr("fe80::/10"),      // IPv6 link-local
    ];
    const int MaxRedirect = 5;
    const int MaxBytes = 64 * 1024;          // giới hạn kích thước phản hồi
    const double HanGiay = 5.0;              // giới hạn tổng thời gian một lần fetch

    // Bảng phân giải và bảng "dial" được tiêm (chỉ cho demo).
    static readonly Dictionary<string, List<string>> Resolver = DocResolver();
    static readonly Dictionary<string, int> DialMap = DocDial();

    static (byte[], int, string) Cidr(string s)
    {
        var p = s.Split('/');
        return (IPAddress.Parse(p[0]).GetAddressBytes(), int.Parse(p[1]), s);
    }

    static Dictionary<string, List<string>> DocResolver()
    {
        var d = new Dictionary<string, List<string>>();
        var node = JsonNode.Parse(Environment.GetEnvironmentVariable("SSRF_RESOLVER") ?? "{}")!.AsObject();
        foreach (var (k, v) in node) d[k] = [.. v!.AsArray().Select(x => x!.GetValue<string>())];
        return d;
    }

    static Dictionary<string, int> DocDial()
    {
        var d = new Dictionary<string, int>();
        var node = JsonNode.Parse(Environment.GetEnvironmentVariable("SSRF_DIAL") ?? "{}")!.AsObject();
        foreach (var (k, v) in node) d[k] = v!.GetValue<int>();
        return d;
    }

    /// <summary>Phân giải tên -> một địa chỉ IP (chuỗi). Nếu host đã là IP thì trả nguyên.
    /// Với bảng giả lập: mỗi lần gọi trả IP kế tiếp trong danh sách (IP cuối "dính"), nhờ đó mô phỏng
    /// DNS rebinding: lần kiểm tra trả IP công khai, lần nối thật trả IP nội bộ.</summary>
    static string ResolveOnce(string host)
    {
        if (IPAddress.TryParse(host, out _)) return host;        // host vốn là địa chỉ IP dạng số
        if (Resolver.TryGetValue(host, out var lst))
        {
            string ip = lst[0];
            if (lst.Count > 1) lst.RemoveAt(0);                  // advance: lần sau trả IP kế tiếp
            return ip;
        }
        try { return Dns.GetHostAddresses(host)[0].ToString(); } // ngoài demo: DNS thật
        catch (SocketException) { throw new FetchException($"Không phân giải được tên {host}"); }
    }

    /// <summary>Chặn nếu IP thuộc dải nội bộ/loopback/link-local… (dùng cho bản AN TOÀN).</summary>
    static void ValidateIp(string ip)
    {
        var addr = IPAddress.Parse(ip).GetAddressBytes();
        foreach (var (net, prefix, text) in BlockedNets)
            if (addr.Length == net.Length && InNet(addr, net, prefix))
                throw new SsrfException($"Chặn SSRF: địa chỉ {ip} thuộc dải cấm {text}");
    }

    static bool InNet(byte[] addr, byte[] net, int prefix)
    {
        for (int i = 0; i < addr.Length; i++)
        {
            int bits = Math.Clamp(prefix - i * 8, 0, 8);
            if (bits == 0) break;
            int mask = (0xFF << (8 - bits)) & 0xFF;
            if ((addr[i] & mask) != (net[i] & mask)) return false;
        }
        return true;
    }

    /// <summary>Mở socket tới (ip, port). Bảng DialMap (chỉ demo) ánh xạ ip -> 127.0.0.1:cổng_cục_bộ.</summary>
    static Socket Dial(string ip, int port, double timeout)
    {
        string h = ip;
        int p = port;
        if (DialMap.TryGetValue(ip, out int local)) { h = "127.0.0.1"; p = local; }
        var s = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        var ar = s.BeginConnect(h, p, null, null);
        if (!ar.AsyncWaitHandle.WaitOne((int)(timeout * 1000))) { s.Close(); throw new FetchException($"Không nối được tới {ip}:{port}"); }
        s.EndConnect(ar);
        return s;
    }

    static (Uri U, string Host, int Port, string Path) SplitUrl(string url)
    {
        var u = new Uri(url);
        string host = u.HostNameType == UriHostNameType.Unknown ? "" : u.Host;
        int port = u.Port >= 0 ? u.Port : (u.Scheme == "https" ? 443 : 80);
        string path = u.AbsolutePath.Length == 0 ? "/" : u.AbsolutePath;
        if (u.Query.Length > 0) path += u.Query;                 // Query đã có sẵn dấu '?'
        return (u, host, port, path);
    }

    /// <summary>Gửi GET HTTP/1.0 (Connection: close) tới IP đã chọn, đọc phản hồi (giới hạn kích thước).</summary>
    static (int Status, Dictionary<string, string> Headers, byte[] Body) HttpGet(
        string ip, string host, int port, string path, double timeout, int limitBytes)
    {
        var s = Dial(ip, port, timeout);
        var data = new List<byte>();
        try
        {
            s.ReceiveTimeout = s.SendTimeout = (int)(timeout * 1000);
            var req = Encoding.UTF8.GetBytes($"GET {path} HTTP/1.0\r\nHost: {host}\r\nAccept: */*\r\nConnection: close\r\n\r\n");
            s.Send(req);
            var buf = new byte[4096];
            while (data.Count < limitBytes)
            {
                int n = s.Receive(buf);
                if (n == 0) break;
                data.AddRange(buf.AsSpan(0, n).ToArray());
            }
        }
        finally { s.Close(); }

        var all = data.ToArray();
        int sep = FindSep(all);
        var head = Encoding.Latin1.GetString(all, 0, sep < 0 ? all.Length : sep);
        var body = sep < 0 ? [] : all[(sep + 4)..];
        var lines = head.Split("\r\n");
        var first = lines[0].Split(' ');
        int status = first.Length > 1 && int.TryParse(first[1], out int st) ? st : 0;
        var headers = new Dictionary<string, string>();
        foreach (var ln in lines.Skip(1))
        {
            int c = ln.IndexOf(':');
            if (c >= 0) headers[ln[..c].Trim().ToLowerInvariant()] = ln[(c + 1)..].Trim();
        }
        return (status, headers, body);
    }

    static int FindSep(byte[] d)
    {
        for (int i = 0; i + 3 < d.Length; i++)
            if (d[i] == '\r' && d[i + 1] == '\n' && d[i + 2] == '\r' && d[i + 3] == '\n') return i;
        return -1;
    }

    static readonly int[] RedirectCodes = [301, 302, 303, 307, 308];

    /// <summary>Bản NGÂY THƠ: KHÔNG kiểm tra gì. Phân giải tên khi "kiểm tra" rồi phân giải LẠI khi nối
    /// (lỗi TOCTOU), theo mọi chuyển hướng, không giới hạn. Ví dụ dễ bị SSRF – không dùng thật.</summary>
    public static (int Status, byte[] Body, List<string> Log) FetchNaive(string url)
    {
        var log = new List<string>();
        for (int i = 0; i <= MaxRedirect; i++)
        {
            var (_, host, port, path) = SplitUrl(url);
            _ = ResolveOnce(host);                               // "kiểm tra" hình thức rồi vứt đi (vô dụng)
            string ip = ResolveOnce(host);                       // nối bằng lần phân giải MỚI -> rebinding lọt
            log.Add($"nối tới {host} ({ip}):{port}{path}");
            var (status, headers, body) = HttpGet(ip, host, port, path, HanGiay, 1 << 30);
            if (RedirectCodes.Contains(status) && headers.TryGetValue("location", out var loc))
            {
                url = UrlJoin(url, loc);
                log.Add($"theo chuyển hướng -> {url}");
                continue;
            }
            return (status, body, log);
        }
        throw new FetchException("Quá nhiều chuyển hướng");
    }

    /// <summary>Bản AN TOÀN: lần lượt áp các lớp phòng thủ SSRF.</summary>
    public static (int Status, byte[] Body, List<string> Log) FetchSafe(string url)
    {
        var log = new List<string>();
        var sw = Stopwatch.StartNew();
        for (int hop = 0; hop <= MaxRedirect; hop++)
        {
            var (u, host, port, path) = SplitUrl(url);
            if (u.Scheme != "http" && u.Scheme != "https")               // (1) chỉ cho http/https
                throw new SsrfException($"Chặn SSRF: giao thức '{u.Scheme}' không được phép");
            if (u.UserInfo.Length > 0)                                   // (2) loại bỏ thông tin đăng nhập
                log.Add("gỡ bỏ thông tin đăng nhập nhúng trong URL");
            if (host.Length == 0) throw new SsrfException("Chặn SSRF: URL thiếu tên máy");
            string ip = ResolveOnce(host);                               // (3) phân giải MỘT lần
            ValidateIp(ip);                                              // (4) chặn dải nội bộ/loopback/link-local
            double conLai = HanGiay - sw.Elapsed.TotalSeconds;
            if (conLai <= 0) throw new FetchException("Hết thời gian cho phép");   // (5) giới hạn tổng thời gian
            log.Add($"cho phép {host} ({ip}):{port}{path}; nối tới ĐÚNG IP đã kiểm tra");
            // (6) nối tới chính IP đã kiểm tra (chống DNS rebinding/TOCTOU), (7) giới hạn kích thước
            var (status, headers, body) = HttpGet(ip, host, port, path, Math.Min(2.0, conLai), MaxBytes);
            if (RedirectCodes.Contains(status) && headers.TryGetValue("location", out var loc))
            {
                if (hop >= MaxRedirect)                                   // (8) giới hạn số lần chuyển hướng
                    throw new SsrfException("Chặn SSRF: vượt số lần chuyển hướng cho phép");
                url = UrlJoin(url, loc);
                log.Add($"kiểm tra lại chặng chuyển hướng -> {url}");
                continue;                                                // (9) kiểm tra lại MỌI chặng
            }
            return (status, body, log);
        }
        throw new FetchException("Quá nhiều chuyển hướng");
    }

    static string UrlJoin(string baseUrl, string loc) =>
        Uri.TryCreate(loc, UriKind.Absolute, out _) ? loc : new Uri(new Uri(baseUrl), loc).ToString();

    public static void LogErr(params string[] parts) =>
        Console.Error.WriteLine(parts.Length > 0 ? "[ssrf] " + string.Join(" ", parts) : "[ssrf]");
}
