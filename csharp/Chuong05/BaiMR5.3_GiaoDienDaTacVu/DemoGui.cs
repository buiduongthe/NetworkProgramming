using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace BaiMR5_3;

// Kịch bản minh họa tự động (vai trò demo) – cùng các bước với kịch bản --demo của bản Python:
//   1. đặt giới hạn 3, bấm Tải tệp (5 tệp), Kiểm tra cổng, Tra cứu tên; bấm Hủy ở tác vụ #2; chụp ảnh 1;
//   2. khi xong: tải 4 tệp nền rồi bấm "Làm SAI" (chặn luồng giao diện ≈ 1,2 s); chụp ảnh 2;
//   3. tải 4 tệp rồi bấm Hủy tất cả; ghép hai ảnh thành ket_qua_gui.png; in thống kê nhịp đồng hồ.
// Toàn bộ kịch bản chạy trên luồng giao diện bằng async/await nên chính kịch bản cũng không chặn giao diện.
static class DemoGui
{
    static async Task WaitUntil(Func<bool> cond)            // chờ điều kiện, không chặn luồng giao diện
    {
        for (int i = 0; i < 600; i++)
        {
            if (cond()) return;
            await Task.Delay(50);
        }
        throw new TimeoutException("Kịch bản chờ quá lâu");
    }

    static Bitmap Snapshot(UngDung f)                        // chụp cửa sổ (kể cả thanh tiêu đề)
    {
        f.Refresh();
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        f.SkipGap = true;                                    // thời gian chụp ảnh không tính vào thống kê
        return bmp;
    }

    public static async Task RunAsync(UngDung f, string outPng)
    {
        bool Running() => f.Tasks.Any(t => !t.Done);
        await Task.Delay(600);
        f.TrackLimit.Value = 3;
        f.SpinN.Value = 5;
        f.BtnDl.PerformClick();
        f.BtnPorts.PerformClick();
        f.BtnDns.PerformClick();
        await Task.Delay(900);
        f.Log($"Kịch bản bấm Hủy ở tác vụ #{f.Tasks[1].Id}");
        f.Tasks[1].BtnCancel.PerformClick();
        await Task.Delay(600);
        int waiting = f.Tasks.Count(t => t.Status.Text == "chờ lượt");
        f.Log($"Chụp ảnh 1: {f.Tasks.Count(t => !t.Done)} tác vụ chưa xong, {waiting} đang chờ lượt");
        using var a = Snapshot(f);
        await WaitUntil(() => !Running());

        f.SpinN.Value = 4;
        f.BtnDl.PerformClick();                               // bốn tệp chạy nền trong lúc làm sai
        await Task.Delay(300);
        f.BtnBad.PerformClick();                              // chặn luồng giao diện ≈ 1,2 s
        await Task.Delay(100);
        using var b = Snapshot(f);
        await WaitUntil(() => !Running());

        f.SpinN.Value = 4;
        f.BtnDl.PerformClick();
        await Task.Delay(500);
        f.BtnCancelAll.PerformClick();
        await WaitUntil(() => !Running());
        await Task.Delay(200);

        const int border = 6;                                 // ghép hai ảnh theo chiều dọc, viền 6 px
        using (var both = new Bitmap(Math.Max(a.Width, b.Width) + 2 * border, a.Height + b.Height + 3 * border))
        using (var g = Graphics.FromImage(both))
        {
            g.Clear(Color.FromArgb(0xD0, 0xD4, 0xDC));
            g.DrawImage(a, border, border);
            g.DrawImage(b, border, a.Height + 2 * border);
            both.Save(outPng, ImageFormat.Png);
        }
        f.Log($"Đã chụp cửa sổ: {Path.GetFileName(outPng)}");
        foreach (var (phase, s) in f.Gaps)
            f.Log($"Nhịp {phase,-17}: {s[0],4} nhịp, TB {s[1] / s[0],6:F1} ms, tối đa {s[2],6:F0} ms");
    }
}
