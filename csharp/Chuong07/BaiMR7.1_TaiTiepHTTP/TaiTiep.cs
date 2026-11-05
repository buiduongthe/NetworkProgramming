using System.Globalization;
using System.Net.Http.Headers;
using System.Security.Cryptography;

namespace BaiMR7_1;

// Client tải tệp HTTP có tải tiếp (resume) khi kết nối bị đứt. Quy trình:
//   1. HEAD lấy kích thước, ETag và mã băm SHA-256 của tệp.
//   2. GET với "Range: bytes=<đã_có>-" và "If-Range: <ETag>"; ghi nối vào tệp cục bộ.
//      206 -> nối tiếp; 200 -> server gửi lại từ đầu (tệp đã đổi hoặc không hỗ trợ Range).
//   3. Nếu nhận ít hơn Content-Length (kết nối đứt) -> chờ lùi thời gian rồi thử lại từ chỗ dừng.
//   4. Kiểm tra kích thước và SHA-256.
// Khác http.client của Python (im lặng trả chuỗi rỗng), HttpClient của .NET ném HttpIOException
// ("The response ended prematurely") khi kết nối đóng trước khi đủ Content-Length; bài bắt ngoại lệ này.
static class TaiTiep
{
    static readonly HttpClient Http = new(new SocketsHttpHandler { UseProxy = false })   // máy chủ thử cục bộ: không đi qua proxy hệ thống
        { Timeout = TimeSpan.FromSeconds(5) };

    static (long Size, EntityTagHeaderValue? ETag, string? Sha) Head(Uri u)
    {
        using var req = new HttpRequestMessage(HttpMethod.Head, u);
        using var r = Http.Send(req);
        if ((int)r.StatusCode != 200) throw new HttpRequestException($"HEAD trả {(int)r.StatusCode}");
        string? sha = r.Headers.TryGetValues("X-Checksum-SHA256", out var v) ? v.First() : null;
        return (r.Content.Headers.ContentLength ?? 0, r.Headers.ETag, sha);
    }

    /// <summary>Một lần GET; trả về số byte có trong tệp cục bộ sau lần này.</summary>
    static long GetRange(Uri u, string path, long have, EntityTagHeaderValue? etag)
    {
        using var req = new HttpRequestMessage(HttpMethod.Get, u);
        if (have > 0)
        {
            req.Headers.Range = new RangeHeaderValue(have, null);              // Range: bytes=<have>-
            if (etag != null) req.Headers.IfRange = new RangeConditionHeaderValue(etag);   // If-Range: <ETag>
        }
        using var r = Http.Send(req, HttpCompletionOption.ResponseHeadersRead);
        int st = (int)r.StatusCode;
        Console.WriteLine($"[tai] -> {st} {r.ReasonPhrase}" + (st == 206 ? $", {r.Content.Headers.ContentRange}" : ""));
        if (st == 416) return have;                    // dải không thỏa mãn được: giữ nguyên phần đã có
        FileMode mode;
        if (st == 200) (mode, have) = (FileMode.Create, 0);   // server gửi toàn bộ: ghi đè từ đầu
        else if (st == 206) mode = FileMode.Append;
        else throw new HttpRequestException($"Mã trạng thái không mong đợi {st}");
        long expected = r.Content.Headers.ContentLength ?? 0, got = 0;
        using (var f = new FileStream(path, mode))
        using (var body = r.Content.ReadAsStream())
        {
            var buf = new byte[16384];
            try
            {
                while (got < expected)
                {
                    int n = body.Read(buf, 0, (int)Math.Min(buf.Length, expected - got));
                    if (n == 0) break;
                    f.Write(buf, 0, n);
                    got += n;
                }
            }
            catch (IOException) { }                    // HttpIOException: kết nối đóng sớm
        }
        if (got < expected) Console.WriteLine($"[tai] Kết nối đứt: nhận {got}/{expected} byte của lần này");
        return have + got;
    }

    public static int Run(string[] args)
    {
        var pos = new List<string>();
        int soLanThu = 5;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--so-lan-thu") soLanThu = int.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        var u = new Uri(pos[0]);
        string output = pos[1];
        var (size, etag, sha) = Head(u);
        Console.WriteLine($"[tai] HEAD: {size} byte, ETag {etag}");
        long have = File.Exists(output) ? new FileInfo(output).Length : 0;
        for (int attempt = 1; attempt <= soLanThu; attempt++)
        {
            if (have >= size) break;
            Console.WriteLine($"[tai] Lần {attempt}: đã có {have} byte, xin " + (have > 0 ? $"Range: bytes={have}-" : "toàn bộ"));
            try
            {
                have = GetRange(u, output, have, etag);
            }
            catch (Exception e) when (e is HttpRequestException || e is IOException || e is TaskCanceledException)
            {
                Console.WriteLine($"[tai] Lỗi: {e.Message}");
                have = File.Exists(output) ? new FileInfo(output).Length : 0;
            }
            if (have < size)
            {
                double delay = 0.2 * Math.Pow(2, attempt - 1);        // lùi thời gian theo cấp số nhân (Bài 5.6)
                Console.WriteLine($"[tai] Chờ {delay.ToString("F1", CultureInfo.InvariantCulture)} s rồi tải tiếp");
                Thread.Sleep(TimeSpan.FromSeconds(delay));
            }
        }
        if (have < size)
        {
            Console.WriteLine($"[tai] THẤT BẠI sau {soLanThu} lần thử: có {have}/{size} byte");
            return 1;
        }
        string digest = Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(output)));
        Console.WriteLine($"[tai] Đủ {have}/{size} byte; SHA-256 {(digest == sha ? "KHỚP" : "SAI")} ({digest[..16]}...)");
        return 0;
    }
}
