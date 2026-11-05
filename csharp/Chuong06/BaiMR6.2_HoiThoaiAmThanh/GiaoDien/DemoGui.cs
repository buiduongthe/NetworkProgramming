using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BaiMR6_2;

// Kịch bản minh họa tự động (vai trò demo): một server GUI + ba client GUI trong cùng tiến trình, thao tác như
// người dùng bằng PerformClick/đặt Value. Chỉ dùng nguồn GIẢ LẬP (không mở micro, không phát loa thật):
//   1. Start Server; server Start Mic (giả lập sin 330 Hz);
//   2. A, B, C Connect rồi Start Mic (sin 440 / 620 / 880 Hz) – chờ 2 s, in mức đo;
//   3. server khóa mic B, đặt độ lợi C = 150 %; A đặt âm lượng nghe 50 %;
//   4. B bấm Stop Mic rồi Start Mic -> bị từ chối (server đang khóa);
//   5. chờ 2,5 s cho thanh đo mức chạy, in số liệu, chụp 4 cửa sổ thành một ảnh PNG.
// Toàn bộ kịch bản là hàm async chạy trên luồng giao diện: mỗi await Task.Delay trả luồng giao diện cho vòng lặp
// thông điệp nên cửa sổ vẫn vẽ lại và bộ hẹn giờ vẫn chạy trong lúc chờ.
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

    static async Task Cho(Func<bool> dk, string viec)
    {
        for (int i = 0; i < 100; i++)
        {
            if (dk()) return;
            await Task.Delay(50);
        }
        throw new TimeoutException($"chờ quá lâu: {viec}");
    }

    static string Db(double v) => double.IsNegativeInfinity(v) ? "---" : $"{v:F1}";

    static void InSoLieu(FormServer s, FormKhach[] ks)
    {
        foreach (var k in ks)
            lock (k.May.Khoa)
                In($"  client {k.TxtTen.Text}: mức nhận {Db(k.MucNhan.LayMuc())} dBFS, mức mic {Db(k.MucMic.LayMuc())} dBFS, " +
                   $"gửi {k.May.GoiGui} gói, nhận {k.May.GoiNhan}, mất {k.May.MatNhan}, mic {k.May.LyDoMic}, " +
                   $"Server Vol {(k.May.ServerVol is double v ? GD.PhanTram(v) : "--")}");
        foreach (var t in new[] { "A", "B", "C" })
            if (s.Hang(t) is { } h) In($"  server thấy {t}: mức vào {Db(h.Muc)} dBFS");
    }

    public static async Task RunAsync(FormServer s, string anh)
    {
        var ks = new[] { new FormKhach("127.0.0.1", int.Parse(s.TxtTcp.Text), "A", 440),
                         new FormKhach("127.0.0.1", int.Parse(s.TxtTcp.Text), "B", 620),
                         new FormKhach("127.0.0.1", int.Parse(s.TxtTcp.Text), "C", 880) };
        for (int i = 0; i < ks.Length; i++)
        {
            ks[i].Location = new Point(s.Right + 10 + i * 30, 40 + i * 30);
            ks[i].Show();
        }
        await Task.Delay(500);

        In("1. Bấm Start Server, server Start Mic (giả lập sin 330 Hz)");
        s.BtnStart.PerformClick();
        if (s.Phong == null) throw new InvalidOperationException("server không khởi động được");
        s.CboMic.SelectedIndex = 0;
        s.BtnMicStart.PerformClick();

        In("2. A, B, C bấm Connect rồi Start Mic (giả lập 440 / 620 / 880 Hz)");
        foreach (var k in ks)
        {
            await k.KetNoiAsync();
            k.CboMic.SelectedIndex = 0;
            k.BtnMicStart.PerformClick();
        }
        await Cho(() => s.Hang("C") != null, "3 client vào phòng");
        await Task.Delay(2000);
        In("   Số liệu sau 2 s:");
        InSoLieu(s, ks);

        In("3. Server khóa mic B, kéo độ lợi C lên 150 %; A kéo âm lượng nghe xuống 50 %");
        s.Hang("B")!.BtnKhoa.PerformClick();
        s.Hang("C")!.TrkLoi.Value = 150;
        ks[0].TrkNghe.Value = 50;
        await Task.Delay(700);

        In("4. B bấm Stop Mic rồi Start Mic (xin bật lại khi đang bị khóa)");
        ks[1].BtnMicStop.PerformClick();
        await Task.Delay(300);
        ks[1].BtnMicStart.PerformClick();
        await Task.Delay(2500);

        In("5. Số liệu cuối:");
        InSoLieu(s, ks);
        s.Phong?.InTrangThai();

        using var a = Chup(s);
        var cs = ks.Select(Chup).ToList();
        const int vien = 8;
        int rongClient = cs.Sum(b => b.Width) + vien * (cs.Count - 1);
        using (var tong = new Bitmap(Math.Max(a.Width, rongClient) + 2 * vien, a.Height + cs.Max(b => b.Height) + 3 * vien))
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
            tong.Save(anh, ImageFormat.Png);
            if (new FileInfo(anh).Length > 950_000)            // giữ ảnh < 1 MB: thu nhỏ 70 %
            {
                using var nho = new Bitmap(tong, new Size(tong.Width * 7 / 10, tong.Height * 7 / 10));
                nho.Save(anh, ImageFormat.Png);
            }
        }
        foreach (var b in cs) b.Dispose();
        In($"Đã chụp 4 cửa sổ vào {Path.GetFullPath(anh)} ({new FileInfo(anh).Length / 1024} KB)");

        foreach (var k in ks) k.Close();
        await Task.Delay(500);
    }
}
