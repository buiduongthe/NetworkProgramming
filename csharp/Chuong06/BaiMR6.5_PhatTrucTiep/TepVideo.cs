using System.Diagnostics;
using static BaiMR6_5.Chung;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Nguồn "tệp video" cho máy phát: phát lại hình + tiếng của một tệp (mp4, mkv, avi, webm…)
// như một buổi phát trực tiếp (giống OBS "Media Source" hay `ffmpeg -re … rtmp://…`). Không phụ thuộc WinForms:
// dùng cho cả bản dòng lệnh (--tep, --tao-tep-mau) và bản giao diện (nguồn "Tệp video").
//
// Hai tiến trình ffmpeg cùng đọc tệp với `-re` (đọc theo tốc độ thật, không nhanh hơn thời gian phát) và
// `-stream_loop -1` (hết tệp thì phát lại từ đầu):
//     hình:  ffmpeg -re -stream_loop -1 -i <tệp> -an -vf scale=160:120:force_original_aspect_ratio=decrease,
//                   pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
//     tiếng: ffmpeg -re -stream_loop -1 -i <tệp> -vn -ac 1 -ar 16000 -f s16le pipe:1
// Bộ lọc `fps=<fps>` cho đúng <fps> khung mỗi giây THỜI GIAN CỦA TỆP, nên khung thứ i có thời điểm trong tệp
// i × 1000 / fps ms; tiếng có 16 000 mẫu mỗi giây của tệp.
//
// Đồng bộ tiếng-hình ở phía phát. Hai tiến trình khởi động lệch nhau vài chục – vài trăm ms; nếu cứ gửi "khung mới
// nhất" thì hình và tiếng của TỆP lệch nhau ngay từ máy phát, người xem đồng bộ theo nhãn thời gian cũng không sửa
// được. Cách làm ở đây (giống bộ ghép kênh – muxer – của bộ mã hóa thật):
//   - Tiếng là đồng hồ chủ: vòng 20 ms của máy phát lấy lần lượt 320 mẫu của tệp. Chưa đủ mẫu (ffmpeg chưa chạy
//     kịp) -> gửi im lặng và LechMs += 20; dồn quá 300 ms -> bỏ bớt 20 ms và LechMs −= 20. Vậy ở thời điểm phương
//     tiện T máy phát đang gửi tiếng của giây T − LechMs trong tệp.
//   - Hình: luồng đọc giữ ~3 s khung gần nhất kèm thời điểm trong tệp; khi máy phát chụp khung ở thời điểm phương
//     tiện T, chọn khung có thời điểm trong tệp ≤ T − LechMs gần nhất. Nhờ vậy nhãn thời gian gửi đi khớp với nội
//     dung của tệp, và cơ chế đồng bộ ở người xem hoạt động như với camera.
// Tệp không có tiếng (hoặc chọn micro khác) -> hình lấy khung mới nhất.
sealed class TepVideo : IDisposable
{
    public const string Tep = "tep";                        // mã nguồn "tệp video" trong danh sách camera/micro
    const int ByteMoiKhungAm = MauMoiKhung * 2;             // 320 mẫu × 2 byte (s16le)
    const int DonToiDaMs = 300;                             // tiếng của tệp dồn quá mức này thì bỏ bớt
    const int GiuHinhMs = 3000;                             // giữ khung hình của 3 s gần nhất

    public readonly string DuongDan, Ten;
    readonly int fps;
    readonly object khoa = new();
    readonly LinkedList<(long Ms, byte[] Khung)> hinh = new();   // (ms trong tệp, khung rgb)
    readonly List<byte> tieng = [];
    long soKhungDoc;
    public long LechMs;                                      // thời điểm phương tiện − thời điểm trong tệp (tiếng)
    public volatile bool DungTieng;                          // micro của máy phát đang là tiếng của tệp này
    public volatile string LoiHinh = "", LoiTieng = "";
    bool hetTieng;
    readonly Process ffmpegHinh;
    readonly Process? ffmpegTieng;
    string dongLoiHinh = "", dongLoiTieng = "";

    static List<string> LenhHinh(string duongDan, int fps) =>
    [
        "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan, "-an", "-vf",
        $"scale={Rong}:{Cao}:force_original_aspect_ratio=decrease,pad={Rong}:{Cao}:(ow-iw)/2:(oh-ih)/2,fps={fps}",
        "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1",
    ];

    static List<string> LenhTieng(string duongDan) =>
    [
        "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan,
        "-vn", "-ac", "1", "-ar", $"{TanSoMau}", "-f", "s16le", "pipe:1",
    ];

    static Process Chay(List<string> doiSo, Action<string> loi)
    {
        var psi = new ProcessStartInfo("ffmpeg")
        {
            RedirectStandardOutput = true, RedirectStandardError = true, RedirectStandardInput = true,
            UseShellExecute = false, CreateNoWindow = true,
        };
        foreach (var a in doiSo) psi.ArgumentList.Add(a);
        var p = Process.Start(psi) ?? throw new IOException("không chạy được ffmpeg");
        p.StandardInput.Close();                                         // như stdin=DEVNULL
        p.ErrorDataReceived += (_, e) => { if (!string.IsNullOrWhiteSpace(e.Data)) loi(e.Data.Trim()); };
        p.BeginErrorReadLine();
        return p;
    }

    /// <summary>Mở tệp: chạy ffmpeg hình (và tiếng nếu coTieng). Lỗi -> IOException.</summary>
    public TepVideo(string duongDan, int fps = FpsMacDinh, bool coTieng = true)
    {
        if (!CameraFfmpeg.CoFfmpeg()) throw new IOException("không có ffmpeg trong PATH");
        if (!File.Exists(duongDan)) throw new IOException($"không thấy tệp {duongDan}");
        (DuongDan, this.fps, Ten) = (duongDan, fps, Path.GetFileName(duongDan));
        hetTieng = !coTieng;
        ffmpegHinh = Chay(LenhHinh(duongDan, fps), d => dongLoiHinh = d);
        new Thread(DocHinh) { IsBackground = true, Name = "tep-hinh" }.Start();
        if (coTieng)
        {
            ffmpegTieng = Chay(LenhTieng(duongDan), d => dongLoiTieng = d);
            new Thread(DocTieng) { IsBackground = true, Name = "tep-tieng" }.Start();
        }
    }

    void DocHinh()
    {
        var s = ffmpegHinh.StandardOutput.BaseStream;
        try
        {
            while (true)
            {
                var k = new byte[Rong * Cao * 3];
                s.ReadExactly(k);
                lock (khoa)
                {
                    long ms = soKhungDoc * 1000 / fps;
                    soKhungDoc++;
                    hinh.AddLast((ms, k));
                    while (hinh.Count > 0 && hinh.First!.Value.Ms < ms - GiuHinhMs) hinh.RemoveFirst();
                }
            }
        }
        catch (Exception e) when (e is EndOfStreamException or IOException or ObjectDisposedException) { }
        Thread.Sleep(200);                                               // chờ dòng lỗi cuối của ffmpeg
        LoiHinh = dongLoiHinh != "" ? dongLoiHinh : "ffmpeg (hình) đã dừng";
    }

    void DocTieng()
    {
        var s = ffmpegTieng!.StandardOutput.BaseStream;
        var buf = new byte[ByteMoiKhungAm];
        try
        {
            int n;
            while ((n = s.Read(buf, 0, buf.Length)) > 0)
                lock (khoa) tieng.AddRange(buf.AsSpan(0, n));
        }
        catch (Exception e) when (e is IOException or ObjectDisposedException) { }
        Thread.Sleep(200);
        lock (khoa)
        {
            hetTieng = true;
            LoiTieng = dongLoiTieng != "" ? dongLoiTieng : "tệp không có tiếng";
        }
    }

    // ------------------------------------------------------------ tiếng (vòng 20 ms của máy phát)
    /// <summary>320 mẫu tiếp theo của tệp; chưa đủ -> im lặng (LechMs += 20).</summary>
    public short[] LayTieng()
    {
        var ra = new short[MauMoiKhung];
        lock (khoa)
        {
            int don = ByteMoiKhungAm * DonToiDaMs / KhungMs;
            while (tieng.Count > don)                                    // dồn quá nhiều: bỏ 20 ms cũ nhất
            {
                tieng.RemoveRange(0, ByteMoiKhungAm);
                LechMs -= KhungMs;
            }
            if (tieng.Count < ByteMoiKhungAm)
            {
                LechMs += KhungMs;
                return ra;
            }
            for (int i = 0; i < MauMoiKhung; i++) ra[i] = (short)(tieng[2 * i] | (tieng[2 * i + 1] << 8));   // s16le: byte thấp trước
            tieng.RemoveRange(0, ByteMoiKhungAm);
        }
        return ra;
    }

    // ------------------------------------------------------------ hình (vòng fps của máy phát)
    public byte[]? LayHinh(long tsMs)
    {
        lock (khoa)
        {
            if (hinh.Count == 0) return null;
            if (!DungTieng || hetTieng) return (byte[])hinh.Last!.Value.Khung.Clone();   // không có tiếng của tệp: khung mới nhất
            long can = tsMs - LechMs;                                    // thời điểm trong tệp ứng với tiếng đang gửi
            byte[]? chon = null;
            foreach (var (ms, k) in hinh)
            {
                if (ms <= can) chon = k;
                else break;
            }
            chon ??= hinh.First!.Value.Khung;                            // hình chưa tới kịp: dùng khung sớm nhất đang có
            return (byte[])chon.Clone();
        }
    }

    public void Dispose()
    {
        foreach (var p in new[] { ffmpegHinh, ffmpegTieng })
        {
            if (p == null) continue;
            try
            {
                if (!p.HasExited) p.Kill();
                p.WaitForExit(2000);
            }
            catch (Exception e) when (e is InvalidOperationException or System.ComponentModel.Win32Exception) { }
        }
    }

    /// <summary>Mở tệp video; trả về (CameraTep, MicTep hoặc null). Lỗi -> IOException.</summary>
    public static (CameraTep Cam, MicTep? Mic) MoTep(string duongDan, int fps = FpsMacDinh, bool layTieng = true)
    {
        var tep = new TepVideo(duongDan, fps, layTieng);
        return (new CameraTep(tep), layTieng ? new MicTep(tep) : null);
    }

    /// <summary>Tạo tệp mp4 mẫu bằng ffmpeg (để thử khi chưa có tệp): ảnh testsrc2 chớp trắng 0,1 s đầu mỗi giây,
    /// tiếng 440 Hz nhỏ + bíp 1000 Hz đúng lúc chớp – nhìn/nghe là thấy hình và tiếng có khớp.</summary>
    public static string TaoTepMau(string duongDan, int giay = 6)
    {
        string hinhMau = $"testsrc2=size=320x240:rate=25:duration={giay}," +
                         "drawbox=x=0:y=0:w=iw:h=ih:color=white@0.85:t=fill:enable='lt(mod(t\\,1)\\,0.1)'";
        string tiengMau = $"aevalsrc='0.08*sin(2*PI*440*t)+0.4*sin(2*PI*1000*t)*lt(mod(t\\,1)\\,0.1)':s={TanSoMau}:d={giay}";
        List<string> lenh = ["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", hinhMau,
                             "-f", "lavfi", "-i", tiengMau, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "25",
                             "-c:a", "aac", "-shortest", duongDan];
        var (ma, loi) = ChayCho(lenh);
        if (ma != 0)                                                     // ffmpeg thiếu libx264: dùng mpeg4
        {
            lenh[lenh.IndexOf("libx264")] = "mpeg4";
            (ma, loi) = ChayCho(lenh);
        }
        if (ma != 0) throw new IOException(loi != "" ? loi.Split('\n', StringSplitOptions.RemoveEmptyEntries)[^1].Trim() : "ffmpeg lỗi");
        return duongDan;
    }

    static (int Ma, string Loi) ChayCho(List<string> doiSo)
    {
        var psi = new ProcessStartInfo("ffmpeg")
        {
            RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
        };
        foreach (var a in doiSo) psi.ArgumentList.Add(a);
        try
        {
            using var p = Process.Start(psi) ?? throw new IOException("không chạy được ffmpeg");
            var outTask = p.StandardOutput.ReadToEndAsync();
            string err = p.StandardError.ReadToEnd();
            p.WaitForExit();
            outTask.Wait();
            return (p.ExitCode, err.Trim());
        }
        catch (System.ComponentModel.Win32Exception e) { throw new IOException($"không chạy được ffmpeg ({e.Message})"); }
    }
}

/// <summary>Phần hình của TepVideo theo giao diện camera của máy phát.</summary>
sealed class CameraTep(TepVideo tep) : INguonHinh
{
    public readonly TepVideo Tep = tep;
    public string MoTa => $"TỆP VIDEO – {Tep.Ten}";
    public bool That => false;
    public string? Loi => Tep.LoiHinh != "" ? Tep.LoiHinh : null;
    public void DatFps(int fps) { }                                      // ffmpeg chạy theo fps lúc mở
    public byte[]? LayKhung(long so, long tsMs) => Tep.LayHinh(tsMs);
    public void Dispose() => Tep.Dispose();
}

/// <summary>Phần tiếng của TepVideo theo giao diện micro của máy phát.</summary>
sealed class MicTep : INguonMic
{
    public readonly TepVideo Tep;

    public MicTep(TepVideo tep)
    {
        Tep = tep;
        tep.DungTieng = true;
    }

    public string MoTa => $"TIẾNG CỦA TỆP – {Tep.Ten}";
    public bool That => false;
    public short[] LayKhung() => Tep.LayTieng();
    public void Dispose() => Tep.Dispose();
}
