using System.Diagnostics;
using System.Text.RegularExpressions;
using static BaiMR6_5.Chung;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Nguồn camera của máy phát: camera thật qua ffmpeg, mẫu thử testsrc hoặc camera giả lập
// "bảng clapper". Chép từ MR6.3 (GiaoDien/CameraFfmpeg.cs), sửa: LayKhung(số khung, nhãn ms), thêm mẫu thử
// testsrc, và camera giả lập là NguonClapper (đồng hồ + chớp trắng mỗi giây tròn). Không phụ thuộc WinForms:
// dùng cho cả bản dòng lệnh (--camera) và bản giao diện.
//
// .NET không có API camera trong thư viện chuẩn; thay vì thêm thư viện ngoài, ta chạy tiến trình `ffmpeg` và đọc
// khung hình thô từ ống (pipe) stdout của nó:
//     ffmpeg -f <dshow|v4l2|avfoundation> -i <camera> -vf scale=160:120,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
// Mỗi khung đúng 160 × 120 × 3 = 57 600 byte RGB, đưa thẳng vào đường gửi (mã hóa I/P bằng zlib, chia mảnh).
// Một luồng nền đọc ống liên tục và chỉ giữ khung MỚI NHẤT – video trực tiếp không cần khung cũ.
// Liệt kê camera: Windows `ffmpeg -list_devices true -f dshow -i dummy` (đọc stderr), Linux /dev/video*,
// macOS `ffmpeg -f avfoundation -list_devices true -i ""`.
// Không có ffmpeg -> chỉ còn "Camera giả lập"; ffmpeg dừng/lỗi (không có camera, camera bận…) -> lõi máy phát
// tự chuyển sang camera giả lập.
interface INguonHinh : IDisposable
{
    /// <summary>Khung 160×120 RGB cho khung số `so` chụp lúc `tsMs` (ms phương tiện), hoặc null nếu chưa có.</summary>
    byte[]? LayKhung(long so, long tsMs);
    string MoTa { get; }
    bool That { get; }                                   // true = camera thật (testsrc không tính là thật)
    string? Loi { get; }                                 // khác null: nguồn đã hỏng (ffmpeg thoát…)
    void DatFps(int fps);
}

/// <summary>Bọc NguonClapper theo cùng giao diện với CameraFfmpeg.</summary>
sealed class CameraGiaLap(string ten, int ma, int fps = FpsMacDinh) : INguonHinh
{
    readonly NguonClapper nguon = new(ten, Math.Max(1, ma), fps);
    public byte[]? LayKhung(long so, long tsMs) => nguon.LayKhung(so, tsMs);
    public string MoTa => "GIẢ LẬP – bảng clapper";
    public bool That => false;
    public string? Loi => null;
    public void DatFps(int fps) => nguon.Fps = fps;
    public void Dispose() { }
}

/// <summary>Camera qua tiến trình ffmpeg; luồng nền đọc từng khung 57 600 byte, giữ khung mới nhất.</summary>
sealed class CameraFfmpeg : INguonHinh
{
    public const string GiaLap = "gia_lap", MauThu = "testsrc";   // hai "camera" không phải thiết bị thật
    readonly Process p;
    readonly object khoa = new();
    byte[]? moiNhat;
    volatile string? loi;
    string dongLoiCuoi = "";
    public int SoKhung;

    public string MoTa { get; }
    public bool That { get; }
    public string? Loi => loi;

    public CameraFfmpeg(string ma, string ten, int fps = FpsMacDinh)
    {
        That = ma != MauThu;
        MoTa = That ? $"THẬT – {ten} (ffmpeg)" : "MẪU THỬ ffmpeg testsrc (không phải camera)";
        var psi = new ProcessStartInfo("ffmpeg")
        {
            RedirectStandardOutput = true, RedirectStandardError = true, RedirectStandardInput = true,
            UseShellExecute = false, CreateNoWindow = true,
        };
        foreach (var a in LenhFfmpeg(ma, fps)) psi.ArgumentList.Add(a);   // ArgumentList tự đặt dấu nháy cho tên có khoảng trắng
        p = Process.Start(psi) ?? throw new InvalidOperationException("không chạy được ffmpeg");
        p.StandardInput.Close();                                       // như stdin=DEVNULL: ffmpeg không chờ phím
        p.ErrorDataReceived += (_, e) => { if (!string.IsNullOrWhiteSpace(e.Data)) dongLoiCuoi = e.Data.Trim(); };
        p.BeginErrorReadLine();                          // phải đọc stderr, nếu không ffmpeg có thể bị chặn khi ống đầy
        new Thread(DocKhung) { IsBackground = true, Name = "ffmpeg-doc" }.Start();
    }

    /// <summary>Dòng lệnh ffmpeg thu camera `ma` (hoặc ảnh mẫu thử testsrc) ra khung RGB thô 160×120.</summary>
    static List<string> LenhFfmpeg(string ma, int fps)
    {
        List<string> vao = ma == MauThu ? ["-re", "-f", "lavfi", "-i", $"testsrc=size={Rong}x{Cao}:rate={fps}"]
            : OperatingSystem.IsWindows() ? ["-f", "dshow", "-i", $"video={ma}"]
            : OperatingSystem.IsMacOS() ? ["-f", "avfoundation", "-framerate", "30", "-i", ma]
            : ["-f", "v4l2", "-i", ma];
        return [.. new[] { "-hide_banner", "-loglevel", "error" }, .. vao,
                "-vf", $"scale={Rong}:{Cao},fps={fps}", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"];
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
        catch (Exception e) when (e is EndOfStreamException or IOException or ObjectDisposedException) { }
        Thread.Sleep(200);                               // chờ dòng lỗi cuối của ffmpeg
        loi = dongLoiCuoi != "" ? dongLoiCuoi : "ffmpeg đã dừng";
    }

    public byte[]? LayKhung(long so, long tsMs)
    {
        lock (khoa) return moiNhat == null ? null : (byte[])moiNhat.Clone();   // null: chưa có khung nào
    }

    public void DatFps(int fps) { }                      // ffmpeg chạy theo fps lúc mở

    public void Dispose()
    {
        try { if (!p.HasExited) p.Kill(); p.WaitForExit(2000); }
        catch (Exception e) when (e is InvalidOperationException or System.ComponentModel.Win32Exception) { }
        p.Dispose();
    }

    // ---------------------------------------------------------------- Tìm ffmpeg và liệt kê camera
    /// <summary>Có ffmpeg trong PATH không (như shutil.which("ffmpeg")).</summary>
    public static bool CoFfmpeg()
    {
        string ten = OperatingSystem.IsWindows() ? "ffmpeg.exe" : "ffmpeg";
        foreach (var d in (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator))
        {
            try { if (d != "" && File.Exists(Path.Combine(d.Trim('"'), ten))) return true; }
            catch (ArgumentException) { }
        }
        return false;
    }

    static string ChayLay(string doiSo)
    {
        var psi = new ProcessStartInfo("ffmpeg") { RedirectStandardError = true, RedirectStandardOutput = true, UseShellExecute = false, CreateNoWindow = true };
        foreach (var a in doiSo.Split(' ')) psi.ArgumentList.Add(a == "\"\"" ? "" : a);
        using var q = Process.Start(psi);
        if (q == null) return "";
        q.StandardOutput.ReadToEndAsync();
        string err = q.StandardError.ReadToEnd();
        q.WaitForExit(10000);
        return err;
    }

    /// <summary>Danh sách (mã thiết bị, tên hiển thị) camera thật tìm được qua ffmpeg (KHÔNG bật camera).</summary>
    public static List<(string Ma, string Ten)> LietKeCamera()
    {
        var ds = new List<(string, string)>();
        if (!CoFfmpeg()) return ds;
        try
        {
            if (OperatingSystem.IsWindows())
            {
                bool trongMucVideo = false;
                foreach (var dong in ChayLay("-hide_banner -list_devices true -f dshow -i dummy").Split('\n'))
                {
                    if (dong.Contains("DirectShow video devices")) trongMucVideo = true;      // định dạng ffmpeg cũ (theo mục)
                    else if (dong.Contains("DirectShow audio devices")) trongMucVideo = false;
                    var m = Regex.Match(dong, "\"([^\"]+)\"\\s*\\((video|none)\\)");          // định dạng mới: "Tên" (video)
                    if (m.Success && m.Groups[2].Value == "video") ds.Add((m.Groups[1].Value, m.Groups[1].Value));
                    else if (trongMucVideo && !dong.Contains("Alternative name"))
                    {
                        m = Regex.Match(dong, "\"([^\"]+)\"");
                        if (m.Success) ds.Add((m.Groups[1].Value, m.Groups[1].Value));
                    }
                }
                return ds.Distinct().ToList();
            }
            if (OperatingSystem.IsMacOS())
            {
                bool trongMucVideo = false;
                foreach (var dong in ChayLay("-hide_banner -f avfoundation -list_devices true -i \"\"").Split('\n'))
                {
                    if (dong.Contains("video devices")) trongMucVideo = true;
                    else if (dong.Contains("audio devices")) trongMucVideo = false;
                    else if (trongMucVideo)
                    {
                        var m = Regex.Match(dong, @"\[(\d+)\]\s+(.+)$");
                        if (m.Success) ds.Add((m.Groups[1].Value, m.Groups[2].Value.Trim()));
                    }
                }
                return ds;
            }
            if (Directory.Exists("/dev"))                                                  // Linux
                ds.AddRange(Directory.GetFiles("/dev", "video*").Order().Select(p => (p, p)));
        }
        catch (Exception e) when (e is System.ComponentModel.Win32Exception or IOException or UnauthorizedAccessException) { }
        return ds;
    }

    /// <summary>Mở camera đã chọn; chọn giả lập hoặc không có ffmpeg -> camera giả lập.</summary>
    public static INguonHinh MoCamera(string maThietBi, string tenHienThi, string tenMayPhat, int maMayPhat, int fps = FpsMacDinh)
    {
        if (maThietBi == GiaLap || !CoFfmpeg()) return new CameraGiaLap(tenMayPhat, maMayPhat, fps);
        return new CameraFfmpeg(maThietBi, tenHienThi, fps);
    }
}
