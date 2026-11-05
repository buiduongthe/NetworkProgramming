// Bài mở rộng MR5.5 – Phần dùng chung: hằng số, việc CPU (băm), hàm "thư viện cũ" kiểu chặn, nhật ký.
// Giao thức (mỗi kết nối một yêu cầu) giống bản Python/Go:
//   BAM <mã> <chế_độ> <số_byte> <số_vòng> <hạn_ms>\n + dữ liệu    chế_độ: chan | tach | gioihan
//       → "OK <mã> <16 hex đầu SHA-256>" hoặc "HETGIO <mã>"
//   CAMBIEN <mã>   → "CAMBIEN <mã> <giá trị> <tên luồng đã hoàn tất TaskCompletionSource>"
//   DATLAI         → "OK";   THONGKE <nhãn> → "TK <trễ nhịp ms> <số mẫu> <trễ giao mẫu ms> <nhóm tính tối đa>"
using System.Diagnostics;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;

namespace BaiMR5_5;

static class Chung
{
    public const int NYC = 16, SoByte = 64 * 1024, SoVong = 400, MsCu = 20, NhomTinh = 2, GioiHan = 4,
                     NhipMs = 10, CamBienMs = 50;

    static readonly Stopwatch Dong = Stopwatch.StartNew();
    static readonly object KhoaIn = new();
    public static readonly UTF8Encoding Utf8 = new(false);

    public static double BayGioMs() => Dong.Elapsed.TotalMilliseconds;

    // Dữ liệu thử: byte thứ i = i mod 251 (giống mọi bản ngôn ngữ)
    public static byte[] DuLieu(int n) => Enumerable.Range(0, n).Select(i => (byte)(i % 251)).ToArray();

    // Tên dễ đọc của luồng hiện tại: luồng của ThreadPool không có tên riêng → "nhóm#<mã luồng>"
    public static string TenLuong() =>
        Thread.CurrentThread.IsThreadPoolThread ? $"nhóm#{Environment.CurrentManagedThreadId}" : Thread.CurrentThread.Name ?? "?";

    // Việc nặng CPU: SHA-256 của dữ liệu lặp soVong lần; kiểm tra token hủy trước mỗi vòng.
    // Trả về (hex16, tên luồng, -1) hoặc (null, tên luồng, vòng đã dừng) khi bị hủy.
    public static (string? Hex, string Ten, int DungO) Bam(byte[] data, int soVong, CancellationToken ct)
    {
        using var h = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        for (int r = 0; r < soVong; r++)
        {
            if (ct.IsCancellationRequested) return (null, TenLuong(), r);
            h.AppendData(data);
        }
        return (Convert.ToHexStringLower(h.GetHashAndReset())[..16], TenLuong(), -1);
    }

    // Giả lập hàm của thư viện CŨ chỉ có API chặn (driver CSDL đồng bộ, socket chặn...): giữ luồng MsCu ms
    public static string TraCuuCu()
    {
        Thread.Sleep(MsCu);
        return TenLuong();
    }

    public static void Log(string msg)
    {
        lock (KhoaIn) Console.WriteLine($"[{BayGioMs() / 1000,7:F3} s] {TenLuong(),-16}| {msg}");
    }

    public static void Say(string msg = "")
    {
        lock (KhoaIn) Console.WriteLine(msg);
    }

    // Gửi một dòng (kèm dữ liệu nếu có) rồi đọc một dòng trả lời – dùng cho máy tạo tải
    public static async Task<string> GuiAsync(string host, int port, string line, byte[]? data = null)
    {
        using var c = new TcpClient();
        await c.ConnectAsync(host, port);
        var ns = c.GetStream();
        await ns.WriteAsync(Utf8.GetBytes(line + "\n"));
        if (data != null) await ns.WriteAsync(data);
        using var r = new StreamReader(ns, Utf8);
        return await r.ReadLineAsync() ?? "";
    }
}
