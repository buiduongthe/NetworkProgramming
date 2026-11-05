using System.Buffers.Binary;
using System.Diagnostics;
using System.Globalization;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR5_1;

// Khung giao thức dùng chung: định khung JSON, thùng token, băm SHA-256.
// Giao thức (một kết nối TCP cho mỗi lượt truyền):
//   - Một khung điều khiển JSON có tiền tố độ dài 4 byte (big-endian) mở đầu.
//   - Sau đó là luồng byte nhị phân của tệp (từ offset tới hết) để hỗ trợ tải tiếp (resume).
static class Khung
{
    public const int Chunk = 32 * 1024;               // cỡ mỗi lần đọc/ghi
    public const double KhongGioiHan = 1L << 40;      // "không giới hạn" tốc độ (1 TiB/s) – dùng cho mặc định
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    public static double Now => Clock.Elapsed.TotalSeconds;     // đồng hồ đơn điệu (giây)

    public static void SendAll(Socket s, byte[] data, int count)
    {
        int sent = 0;
        while (sent < count) sent += s.Send(data, sent, count - sent, SocketFlags.None);
    }

    /// <summary>Gửi một khung điều khiển JSON: 4 byte độ dài + nội dung UTF-8.</summary>
    public static void GuiKhung(Socket s, JsonObject obj)
    {
        byte[] body = Encoding.UTF8.GetBytes(obj.ToJsonString(Opt));
        var frame = new byte[4 + body.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)body.Length);
        body.CopyTo(frame, 4);
        SendAll(s, frame, frame.Length);
    }

    /// <summary>Đọc đúng n byte (hoặc báo lỗi nếu kết nối đóng giữa chừng).</summary>
    public static byte[] DocDu(Socket s, int n)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = s.Receive(buf, got, n - got, SocketFlags.None);
            if (k == 0) throw new IOException("Kết nối đóng khi đang đọc");
            got += k;
        }
        return buf;
    }

    /// <summary>Nhận một khung điều khiển JSON.</summary>
    public static JsonObject NhanKhung(Socket s)
    {
        int n = (int)BinaryPrimitives.ReadUInt32BigEndian(DocDu(s, 4));
        return JsonNode.Parse(Encoding.UTF8.GetString(DocDu(s, n)))!.AsObject();
    }

    /// <summary>Băm SHA-256 của tệp, trả chuỗi hex chữ thường (giống hexdigest()).</summary>
    public static string Sha256Tep(string path)
    {
        using var f = File.OpenRead(path);
        return Convert.ToHexStringLower(SHA256.HashData(f));
    }

    /// <summary>Phân tích chuỗi tốc độ: '512K', '1M', '2MiB', '1048576' -> số byte/giây.</summary>
    public static long DocSoByte(string s)
    {
        s = s.Trim().ToUpperInvariant().Replace("IB", "").Replace("B", "");
        long nhan = 1;
        if (s.EndsWith('K')) { nhan = 1024; s = s[..^1]; }
        else if (s.EndsWith('M')) { nhan = 1024 * 1024; s = s[..^1]; }
        else if (s.EndsWith('G')) { nhan = 1024 * 1024 * 1024; s = s[..^1]; }
        return (long)(double.Parse(s, CultureInfo.InvariantCulture) * nhan);
    }

    /// <summary>Số byte -> chuỗi dễ đọc (KiB/MiB).</summary>
    public static string DinhDangByte(long n)
    {
        foreach (var (donVi, chia) in new[] { ("MiB", 1024.0 * 1024), ("KiB", 1024.0) })
            if (n >= chia) return (n / chia).ToString("F2", CultureInfo.InvariantCulture) + " " + donVi;
        return $"{n} B";
    }
}

/// <summary>Thùng token giới hạn tốc độ (byte/giây), đổ đầy liên tục; có thể đổi tốc độ lúc chạy.
/// Dùng chung cho mức phiên (session) và mức từng tệp. TieuThu(n) chặn tới khi đủ n token.</summary>
class ThungToken
{
    public double Rate;
    double sucChua, token, moc;
    readonly object khoa = new();

    public ThungToken(double rate)
    {
        Rate = rate;
        sucChua = rate;                               // cho phép bùng một giây
        token = Math.Min(rate, Khung.Chunk * 4);      // bùng khởi động nhỏ -> tốc độ đo sát giới hạn
        moc = Khung.Now;
    }

    void DoDay()
    {
        double now = Khung.Now;
        token = Math.Min(sucChua, token + (now - moc) * Rate);
        moc = now;
    }

    public void DatRate(double rate)
    {
        lock (khoa)
        {
            DoDay();
            Rate = rate;
            sucChua = rate;                           // sức chứa = 1 giây ở tốc độ mới
            // Giới hạn "bùng" còn 0,25 giây để nhiều tệp chia sẻ đều hơn, tốc độ đo sát giới hạn phiên.
            token = Math.Min(token, rate * 0.25);
        }
    }

    /// <summary>Chặn tới khi lấy được n token. dungKhi() trả true thì bỏ dở (khi pause/stop).</summary>
    public bool TieuThu(int n, Func<bool> dungKhi)
    {
        while (!dungKhi())
        {
            double thieu;
            lock (khoa)
            {
                DoDay();
                if (token >= n)
                {
                    token -= n;
                    return true;
                }
                thieu = (n - token) / Rate;
            }
            Thread.Sleep(TimeSpan.FromSeconds(Math.Min(thieu, 0.05)));
        }
        return false;
    }
}
