using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BaiMR6_3;

// Kịch bản minh họa tự động (vai trò demo): Camera Server + ba Camera Client trong cùng tiến trình, thao tác như
// người dùng. Chỉ dùng CAMERA GIẢ LẬP (không mở camera thật):
//   1. START SERVER; A, B, C Kết nối rồi START (camera giả lập) – chờ 2 s, in fps/kbit/s;
//   2. server chọn B -> Tắt camera; chọn C -> fps gửi tối đa 5, độ phân giải gửi 1/2;
//   3. A chọn nhận độ phân giải 1/2; B bấm CANCEL rồi START -> bị từ chối (server đang tắt);
//   4. chờ 2,5 s, in số liệu, chụp 4 cửa sổ thành một ảnh PNG.
// Kịch bản là hàm async trên luồng giao diện: await Task.Delay trả luồng cho vòng lặp thông điệp.
static class DemoGui
{
    static void In(string s) => Console.WriteLine($"[demo] {s}");

    static Bitmap Chup(Form f)
    {
        f.Refresh();
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        return bmp;
    }

    static void InSoLieu(FormServer s, FormKhach[] ks)
    {
        foreach (var k in ks)
            lock (k.May.Khoa)
                In($"  client {k.TxtTen.Text}: camera {k.May.LyDoCam}, gửi {k.May.FpsGui:F0} fps {k.May.KbpsGui:F0} kbit/s, " +
                   $"nhận {k.May.FpsNhan:F0} fps {k.May.KbpsNhan:F0} kbit/s, lưới {k.May.Luoi?.W}x{k.May.Luoi?.H}, mảnh mất {k.May.ManhMat}");
        foreach (var d in s.SoLieu()) In("  " + d);
    }

    public static async Task RunAsync(FormServer s, string anh)
    {
        int cong = int.Parse(s.TxtTcp.Text);
        var ks = new[] { new FormKhach("127.0.0.1", cong, "A"), new FormKhach("127.0.0.1", cong, "B"), new FormKhach("127.0.0.1", cong, "C") };
        for (int i = 0; i < ks.Length; i++)
        {
            ks[i].Location = new Point(300 + i * 60, 120 + i * 60);
            ks[i].Show();
        }
        await Task.Delay(800);

        In("1. START SERVER; A, B, C Kết nối rồi START (camera giả lập)");
        s.BtnStart.PerformClick();
        if (s.Phong == null) throw new InvalidOperationException("server không khởi động được");
        foreach (var k in ks)
        {
            await k.KetNoiAsync();
            k.CboCam.SelectedIndex = 0;                          // "Camera giả lập"
            k.BtnStart.PerformClick();
        }
        await Task.Delay(2000);
        In("   Số liệu sau 2 s:");
        InSoLieu(s, ks);

        In("2. Server chọn B -> Tắt camera; chọn C -> fps gửi tối đa 5, độ phân giải gửi 1/2");
        s.ChonKhach("B");
        s.BtnTatCam.PerformClick();
        s.ChonKhach("C");
        s.TrkFps.Value = 5;                                      // gửi lệnh sau 300 ms (khi "dừng kéo")
        await Task.Delay(400);
        s.CboChia.SelectedIndex = 1;

        In("3. A chọn nhận 1/2; B bấm CANCEL rồi START (xin bật lại khi server đang tắt)");
        ks[0].CboNhanChia.SelectedIndex = 1;
        ks[1].BtnCancel.PerformClick();
        await Task.Delay(300);
        ks[1].BtnStart.PerformClick();
        await Task.Delay(2500);

        In("4. Số liệu cuối:");
        InSoLieu(s, ks);
        s.Phong?.InTrangThai();

        using var a = Chup(s);
        var cs = ks.Select(Chup).ToList();
        const int vien = 8;
        int rongClient = cs.Sum(b => b.Width) + vien * (cs.Count - 1);
        using (var tong = new Bitmap(Math.Max(a.Width, rongClient) + 2 * vien, a.Height + cs.Max(b => b.Height) + 3 * vien))
        {
            using (var g = Graphics.FromImage(tong))
            {
                g.Clear(Color.FromArgb(0xD0, 0xD4, 0xDC));
                g.DrawImage(a, vien, vien);
                int x = vien;
                foreach (var b in cs)
                {
                    g.DrawImage(b, x, a.Height + 2 * vien);
                    x += b.Width + vien;
                }
            }
            tong.Save(anh, ImageFormat.Png);
            for (int phan = 8; new FileInfo(anh).Length > 950_000 && phan >= 4; phan--)   // giữ ảnh < 1 MB
            {
                using var nho = new Bitmap(tong, new Size(tong.Width * phan / 10, tong.Height * phan / 10));
                nho.Save(anh, ImageFormat.Png);
            }
        }
        foreach (var b in cs) b.Dispose();
        In($"Đã chụp 4 cửa sổ vào {Path.GetFullPath(anh)} ({new FileInfo(anh).Length / 1024} KB)");

        foreach (var k in ks) k.Close();
        await Task.Delay(500);
    }
}
