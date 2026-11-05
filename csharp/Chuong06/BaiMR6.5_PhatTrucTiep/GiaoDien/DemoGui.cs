using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
// Một tiến trình, một luồng giao diện: cửa sổ máy chủ, máy phát P, người xem A và B. P dùng camera giả lập "bảng
// clapper" và micro giả lập; A vào ngay, đồng bộ tiếng-hình BẬT; B bỏ chọn đồng bộ rồi vào muộn ~3,5 s. Kịch bản
// bấm nút như người dùng, in số liệu `[demo] …`, chụp 4 cửa sổ ghép thành ket_qua_gui.png. Có ffmpeg: thêm một lượt
// "Mẫu thử ffmpeg (testsrc)" và một lượt "Tệp video" (tạo tệp mẫu mp4 có chớp + bíp mỗi giây, phát cả hình lẫn tiếng
// của tệp; chụp máy phát và người xem A thành ket_qua_gui_tep.png).
// Kịch bản là hàm async trên luồng giao diện: await Task.Delay trả luồng cho vòng lặp thông điệp.
static class DemoGui
{
    static void Log(string s) => Console.WriteLine($"[demo] {s}");

    static string Ms(double? x, bool dau = false) => x is { } v ? Chung.ChuMs(v, dau) : "–";

    /// <summary>Chờ tới khi điều kiện đúng (tối đa 15 s), vòng lặp thông điệp vẫn chạy trong lúc chờ.</summary>
    static async Task Cho(Func<bool> dk, double hetGio = 15.0)
    {
        for (double con = hetGio; !dk(); con -= 0.05)
        {
            if (con <= 0) throw new TimeoutException("Kịch bản chờ quá lâu – dừng");
            await Task.Delay(50);
        }
    }

    static void InSoLieu(FormServer sv, FormPhat p, Dictionary<string, FormXem> ds)
    {
        var t = sv.Lop.TrangThai();
        Log($"   server: vào {t.KbpsVao:F0}, ra {t.KbpsRa:F0} kbit/s, hệ số {t.HeSo:F2}");
        foreach (var x in t.NguoiXem)
            Log($"     -> {x.Ten}: {x.Nhan}, {x.Kbps:F0} kbit/s, I đầu {Ms(x.KhungDauMs)} ms ({(x.NguonDau != "" ? x.NguonDau : "–")})");
        var tp = p.Lop!.TrangThai();
        Log($"   P: I {tp.SoI} P {tp.SoP}, TB I {tp.TbI:F0} B, P {tp.TbP:F0} B, PLI {tp.PliNhan}");
        foreach (var (ten, cs) in ds)
        {
            var tt = cs.Lop!.TrangThai();
            var g = tt.GiaiDoan[^1];
            double? lech = g.N > 0 ? g.Tong / g.N : null;
            Log($"   {ten}: đồng bộ {(tt.DongBo ? "BẬT" : "TẮT")}, khung đầu {Ms(tt.KhungDauMs)} ms ({tt.NguonDau})");
            Log($"      lệch TB {Ms(lech, true)} ms, trễ {Ms(tt.TreTb)} ms, mất tiếng {tt.MatAm}, đứng {tt.DungHinh}");
        }
    }

    /// <summary>Ghép các ảnh theo hàng (mỗi hàng một danh sách) trên nền xám, viền 4; giữ tệp &lt; 1 MB.</summary>
    static void Ghep(string duong, List<List<Bitmap>> hang)
    {
        const int vien = 4;
        int W = hang.Max(h => h.Sum(b => b.Width)) + 2 * vien, H = hang.Sum(h => h.Max(b => b.Height)) + 2 * vien;
        using (var tong = new Bitmap(W, H))
        {
            using (var g = Graphics.FromImage(tong))
            {
                g.Clear(Color.FromArgb(0xD0, 0xD0, 0xD0));
                int y = vien;
                foreach (var h in hang)
                {
                    int x = vien;
                    foreach (var b in h)
                    {
                        g.DrawImage(b, x, y);
                        x += b.Width;
                    }
                    y += h.Max(b => b.Height);
                }
            }
            tong.Save(duong, ImageFormat.Png);
            for (int phan = 9; new FileInfo(duong).Length > 950_000 && phan >= 4; phan--)
            {
                using var nho = new Bitmap(tong, new Size(tong.Width * phan / 10, tong.Height * phan / 10));
                nho.Save(duong, ImageFormat.Png);
            }
        }
        foreach (var b in hang.SelectMany(h => h)) b.Dispose();
    }

    public static async Task RunAsync(FormServer sv, string host, int congTcp, string anh)
    {
        var p = new FormPhat("P", host, congTcp) { Location = new Point(sv.Right + 20, 0) };
        p.Show();
        int y = Math.Max(sv.Bottom, p.Bottom) + 20, x = 0;
        var ds = new Dictionary<string, FormXem>();
        foreach (var ten in new[] { "A", "B" })
        {
            var f = new FormXem(ten, host, congTcp) { Location = new Point(x, y) };
            f.Show();
            ds[ten] = f;
            x = f.Right + 20;
        }
        var (a, b) = (ds["A"], ds["B"]);
        await Task.Delay(500);
        Log("1. Server: bấm START SERVER");
        sv.BtnStart.PerformClick();
        Log("2. P: Kết nối; camera giả lập (clapper), micro giả lập");
        p.CboMic.SelectedIndex = p.DsMic.Count - 1;          // kịch bản luôn dùng micro giả lập (mặc định là micro thật)
        p.BtnKetNoi.PerformClick();
        await Cho(() => p.BtnPhat.Enabled);
        Log("   P bấm PHÁT (10 fps, GOP 20)");
        p.BtnPhat.PerformClick();
        await Task.Delay(1000);
        Log("3. A: Kết nối ngay (đồng bộ tiếng-hình BẬT)");
        a.BtnKetNoi.PerformClick();
        await Task.Delay(3500);
        Log("4. B: bỏ chọn 'Đồng bộ', Kết nối (vào muộn 3,5 s)");
        b.ChkDongBo.Checked = false;
        b.BtnKetNoi.PerformClick();
        await Cho(() => b.Lop?.TrangThai().KhungDauMs != null);
        await Task.Delay(3000);
        Log("5. Số liệu (bộ đệm GOP bật):");
        InSoLieu(sv, p, ds);
        Log("6. P bấm 'Khung khóa ngay'; A bấm 'Chụp'");
        p.BtnI.PerformClick();
        await Task.Delay(600);
        Log($"   A đã chụp {a.Chup()}");
        await Task.Delay(500);
        Log("7. Chụp 4 cửa sổ");
        Ghep(anh, [[GD.Chup(sv), GD.Chup(p)], [GD.Chup(a), GD.Chup(b)]]);
        Log($"   Đã ghi {anh} ({new FileInfo(anh).Length / 1024} KB)");
        Log("8. Server: chọn B, bấm 'Ngắt người chọn'");
        sv.ChonNguoiXem("B");
        sv.BtnNgat.PerformClick();
        await Task.Delay(800);
        Log($"   B: {b.LblTrangThai.Text}");
        if (CameraFfmpeg.CoFfmpeg())
        {
            Log("9. P: DỪNG PHÁT, chọn 'Mẫu thử ffmpeg (testsrc)', PHÁT");
            p.BtnDung.PerformClick();
            await Task.Delay(500);
            p.CboCam.SelectedIndex = p.DsCam.FindIndex(c => c.Ma == CameraFfmpeg.MauThu);
            p.BtnPhat.PerformClick();
            await Task.Delay(4000);
            var tp = p.Lop!.TrangThai();
            Log($"   P testsrc: I {tp.SoI} P {tp.SoP}, TB I {tp.TbI:F0} B, P {tp.TbP:F0} B");
            Log($"   P: {tp.Camera}");
            var ta = a.Lop!.TrangThai();
            Log($"   A: khung I {ta.KhungI}, P {ta.KhungP}, lệch {Ms(ta.LechHien, true)} ms");
            Log("10. P: DỪNG PHÁT, tạo tệp mẫu mp4, nguồn Tệp video, PHÁT");
            p.BtnDung.PerformClick();
            Directory.CreateDirectory("out");
            string tep = await Task.Run(() => TepVideo.TaoTepMau(Path.Combine("out", "tep_mau.mp4"), 10));
            p.DatTep(tep);
            Log($"   {p.CboCam.Text} | {p.CboMic.Text}");
            await Task.Delay(300);
            p.BtnPhat.PerformClick();
            await Task.Delay(4400);
            tp = p.Lop!.TrangThai();
            Log($"   P tệp: I {tp.SoI} P {tp.SoP}, TB I {tp.TbI:F0} B, P {tp.TbP:F0} B");
            Log($"   P: {(tp.Camera.Length > 52 ? tp.Camera[..52] : tp.Camera)}");
            ta = a.Lop!.TrangThai();
            Log($"   A: khung I {ta.KhungI}, P {ta.KhungP}, lệch {Ms(ta.LechHien, true)} ms, mức tiếng {ta.MucAm:F1} dBFS");
            string ra = Path.Combine(Path.GetDirectoryName(anh) is { Length: > 0 } d ? d : ".", "ket_qua_gui_tep.png");
            Ghep(ra, [[GD.Chup(p), GD.Chup(a)]]);
            Log($"   Đã ghi {ra} ({new FileInfo(ra).Length / 1024} KB)");
        }
        Log("11. Server bấm STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Task.Delay(600);
        string Cat(string s) => s.Length > 46 ? s[..46] : s;
        Log($"   P: {Cat(p.LblTrangThai.Text)}");
        Log($"   A: {Cat(a.LblTrangThai.Text)}");
        foreach (var f in new Form[] { p, a, b }) f.Close();
    }
}
