using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BaiMR5_6;

// Kịch bản minh họa tự động (vai trò demo) – cùng các bước với --demo của bản Python (ung_dung.py):
//   1. bấm "Đa luồng – đồng bộ", dừng ở 3,8 s; chụp ảnh 1;
//   2. bấm "Đa luồng – bất đồng bộ", tốc độ × 4, chờ hoạt hình chạy hết (đo số lần vẽ lại);
//   3. bấm "So sánh 4 chế độ", tua tới cuối; chụp ảnh 2; in chỉ số từng chế độ;
//   4. bỏ việc khẩn, bấm "Đa luồng – đồng bộ", dừng ở 3,3 s rồi bấm "Chen ngang U ngay"; in chỉ số;
//   5. ghép hai ảnh (viền 4 px, xếp dọc) thành ket_qua_gui.png.
// Kịch bản chạy trên luồng giao diện bằng async/await (await Task.Delay trả luồng về vòng lặp thông điệp),
// nên chính kịch bản cũng không làm giao diện đứng.
static class DemoGui
{
    static void In(string s) => Console.Out.Write("[gui] " + s + "\n");

    static Bitmap Chup(UngDung f)                            // chụp cửa sổ (kể cả thanh tiêu đề)
    {
        f.Refresh();
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        return bmp;
    }

    static string DongChiSo(string m, KetQua kq)
    {
        var s = LapLich.TinhChiSo(kq);
        string u = s.UCho >= 0 ? LapLich.Giay(s.UCho) + " s" : "–";
        return $"  {LapLich.Trai(LapLich.TenNgan[m], 14)} hoàn tất {LapLich.Giay(s.Tong)} s, luồng {s.Luong}, " +
               $"U chờ {u}, chen ngang {s.Chen}";
    }

    public static async Task RunAsync(UngDung f, string tepAnh)
    {
        await Task.Delay(500);
        In("Bấm 'Đa luồng – đồng bộ', dừng ở 3.8 s: U đã chen ngang Luồng T1, " +
           "hai nửa T1 chạy trên Luồng T1 và T1b");
        f.Nut["da_dong_bo"].PerformClick();
        f.DangChay = false;
        f.T = 38.0;
        f.Ve();
        await Task.Delay(300);
        using var a = Chup(f);

        In("Bấm 'Đa luồng – bất đồng bộ', chạy hết: T2 chạy tiếp trên thợ khác sau I/O");
        f.Nut["da_bat_dong_bo"].PerformClick();
        f.CboTocDo.SelectedItem = "× 4";
        var sw = Stopwatch.StartNew();
        int ve0 = f.SoLanVe;
        while (f.DangChay && sw.Elapsed.TotalSeconds < 10)
            await Task.Delay(50);
        In($"Hoạt hình chạy xong sau {sw.Elapsed.TotalSeconds:F1} s thật, vẽ lại {f.SoLanVe - ve0} lần " +
           "(giao diện không đứng)");

        In("Bấm 'So sánh 4 chế độ', tua tới cuối");
        f.BtnSoSanh.PerformClick();
        f.DangChay = false;
        f.T = f.DoDai();
        f.Ve();
        await Task.Delay(300);
        using var b = Chup(f);
        foreach (var (m, kq) in f.KetQua)
            In(DongChiSo(m, kq));

        In("Bỏ việc khẩn, bấm 'Đa luồng – đồng bộ', dừng ở 3.3 s rồi bấm 'Chen ngang U ngay'");
        f.ChkKhan.Checked = false;
        f.Nut["da_dong_bo"].PerformClick();
        f.DangChay = false;
        f.T = 33.0;
        f.BtnChen.PerformClick();
        var s = LapLich.TinhChiSo(f.KetQua[0].Kq);
        In($"  U đến 3.3 s, chờ {LapLich.Giay(s.UCho)} s, chen ngang {s.Chen} lần, hoàn tất {LapLich.Giay(s.Tong)} s");

        const int vien = 4;                                   // như "convert -border 4 -append" của bản Python
        int rong = Math.Max(a.Width, b.Width) + 2 * vien;
        using (var ca = new Bitmap(rong, a.Height + b.Height + 4 * vien))
        using (var g = Graphics.FromImage(ca))
        {
            g.Clear(ColorTranslator.FromHtml("#d0d4dc"));
            g.DrawImage(a, vien, vien, a.Width, a.Height);
            g.DrawImage(b, vien, a.Height + 3 * vien, b.Width, b.Height);
            ca.Save(tepAnh, ImageFormat.Png);
        }
        In($"Đã chụp cửa sổ: {Path.GetFileName(tepAnh)}");
    }
}
