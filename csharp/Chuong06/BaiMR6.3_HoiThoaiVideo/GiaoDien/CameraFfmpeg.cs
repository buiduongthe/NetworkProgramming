using System.Diagnostics;
using System.Text.RegularExpressions;
using static BaiMR6_3.Chung;

namespace BaiMR6_3;

// Nguồn hình cho bản giao diện: camera thật qua tiến trình ffmpeg, hoặc camera giả lập (khung tổng hợp có sẵn).
// .NET không có API camera trong thư viện chuẩn; thay vì thêm thư viện ngoài, ta chạy ffmpeg và đọc ẢNH THÔ từ
// đầu ra chuẩn (stdout) của nó:
//     ffmpeg -f dshow -i video="Tên camera" -vf scale=160:120,fps=15 -pix_fmt rgb24 -f rawvideo pipe:1
// Mỗi khung đúng 160 × 120 × 3 = 57 600 byte RGB – cùng dạng khung của camera giả lập, nên phần gửi (thu nhỏ,
// nén zlib, chia mảnh) giữ nguyên. Phần này không phụ thuộc WinForms.
interface INguonHinh : IDisposable
{
    /// <summary>Khung mới nhất 160×120 RGB, hoặc null nếu chưa có.</summary>
    byte[]? LayKhung();
    string MoTa { get; }
    bool That { get; }
    string? Loi { get; }                                 // khác null: nguồn đã hỏng (ffmpeg thoát…)
}

sealed class CameraGiaLap(string ten, int ma) : INguonHinh
{
    readonly CameraTongHop cam = new(ten, Math.Max(1, ma));   // lớp của bản dòng lệnh (Chung.cs)
    long so;
    public byte[]? LayKhung() => cam.LayKhung(so++);
    public string MoTa => "camera giả lập";
    public bool That => false;
    public string? Loi => null;
    public void Dispose() { }
}

sealed class CameraFfmpeg : INguonHinh
{
    readonly Process p;
    readonly object khoa = new();
    byte[]? moiNhat;
    volatile string? loi;
    string dongLoiCuoi = "";
    public int SoKhung;
    readonly string ten;

    public string MoTa => $"camera thật «{ten}» (ffmpeg)";
    public bool That => true;
    public string? Loi => loi;

    /// <param name="dauVao">tham số chọn nguồn của ffmpeg, ví dụ ["-f","dshow","-i","video=HP HD Camera"]</param>
    public CameraFfmpeg(string ten, IEnumerable<string> dauVao, int fps)
    {
        this.ten = ten;
        var psi = new ProcessStartInfo("ffmpeg")
        {
            RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
        };
        foreach (var a in new[] { "-hide_banner", "-loglevel", "error" }.Concat(dauVao)
                     .Concat(["-vf", $"scale={Rong}:{Cao},fps={fps}", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"]))
            psi.ArgumentList.Add(a);                     // ArgumentList tự đặt dấu nháy cho tên có khoảng trắng
        p = Process.Start(psi) ?? throw new InvalidOperationException("không chạy được ffmpeg");
        p.ErrorDataReceived += (_, e) => { if (!string.IsNullOrWhiteSpace(e.Data)) dongLoiCuoi = e.Data; };
        p.BeginErrorReadLine();                          // phải đọc stderr, nếu không ffmpeg có thể bị chặn khi ống đầy
        new Thread(DocKhung) { IsBackground = true, Name = "ffmpeg-doc" }.Start();
    }

    void DocKhung()
    {
        var s = p.StandardOutput.BaseStream;
        try
        {
            while (true)
            {
                var k = new byte[Rong * Cao * 3];
                s.ReadExactly(k);                        // chặn tới khi đủ một khung (luồng nền nên không sao)
                lock (khoa) moiNhat = k;
                SoKhung++;
            }
        }
        catch (Exception e) when (e is EndOfStreamException || e is IOException || e is ObjectDisposedException) { }
        Thread.Sleep(200);                               // chờ dòng lỗi cuối của ffmpeg
        loi = $"ffmpeg dừng: {(dongLoiCuoi != "" ? dongLoiCuoi : "không có khung hình")}";
    }

    public byte[]? LayKhung()
    {
        lock (khoa) return moiNhat;
    }

    public void Dispose()
    {
        try { if (!p.HasExited) p.Kill(); } catch (Exception e) when (e is InvalidOperationException || e is System.ComponentModel.Win32Exception) { }
        p.Dispose();
    }

    // ---------------------------------------------------------------- Tìm ffmpeg và liệt kê camera
    public static bool CoFfmpeg()
    {
        try
        {
            using var q = Process.Start(new ProcessStartInfo("ffmpeg", "-version")
                { RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true });
            if (q == null) return false;
            q.StandardOutput.ReadToEnd();
            q.WaitForExit(3000);
            return true;
        }
        catch (System.ComponentModel.Win32Exception) { return false; }   // không có ffmpeg trong PATH
    }

    /// <summary>Danh sách camera: Windows – DirectShow (đọc stderr của ffmpeg -list_devices);
    /// Linux – các tệp /dev/video*. Không có ffmpeg -> danh sách rỗng.</summary>
    public static List<string> DanhSach()
    {
        var ds = new List<string>();
        if (OperatingSystem.IsLinux())
        {
            if (Directory.Exists("/dev")) ds.AddRange(Directory.GetFiles("/dev", "video*").Order());
            return CoFfmpeg() ? ds : [];
        }
        if (!OperatingSystem.IsWindows()) return ds;
        try
        {
            using var q = Process.Start(new ProcessStartInfo("ffmpeg", "-hide_banner -list_devices true -f dshow -i dummy")
                { RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true });
            if (q == null) return ds;
            string loai = "";
            foreach (var dong in q.StandardError.ReadToEnd().Split('\n'))
            {
                // ffmpeg mới: [dshow @ ...] "HP HD Camera" (video)
                // ffmpeg cũ : dòng "DirectShow video devices" rồi các dòng [dshow @ ...]  "HP HD Camera"
                if (dong.Contains("DirectShow video devices")) loai = "video";
                else if (dong.Contains("DirectShow audio devices")) loai = "audio";
                if (dong.Contains("Alternative name")) continue;
                var m = Regex.Match(dong, "\"([^\"]+)\"(\\s*\\((video|audio|none)\\))?");
                if (!m.Success) continue;
                string kieu = m.Groups[3].Success ? m.Groups[3].Value : loai;
                if (kieu == "video" && !ds.Contains(m.Groups[1].Value)) ds.Add(m.Groups[1].Value);
            }
            q.WaitForExit(3000);
        }
        catch (System.ComponentModel.Win32Exception) { }
        return ds;
    }

    /// <summary>Mở camera theo tên trong DanhSach().</summary>
    public static CameraFfmpeg Mo(string ten, int fps) =>
        OperatingSystem.IsWindows() ? new CameraFfmpeg(ten, ["-f", "dshow", "-i", $"video={ten}"], fps)
                                    : new CameraFfmpeg(ten, ["-f", "v4l2", "-i", ten], fps);
}
