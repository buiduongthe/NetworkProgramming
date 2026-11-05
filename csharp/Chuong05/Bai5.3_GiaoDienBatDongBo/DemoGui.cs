using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace Bai5_3;

// Kịch bản minh họa tự động cho LookupForm (giống DemoGui.java, demo_gui.py).
// Thao tác trên cửa sổ thật như người dùng: bấm Kết nối tới cổng không có server (lỗi REFUSED),
// bấm Kết nối tới đúng cổng, nhập "TCP" và bấm Tra cứu. Trong lúc chờ server trả lời chậm, đếm số lần
// đồng hồ giao diện được vẽ lại và chụp cửa sổ; có kết quả thì chụp lần nữa; hai ảnh ghép thành một tệp PNG.
// Toàn bộ kịch bản chạy trên luồng giao diện bằng async/await, nên chính kịch bản cũng không chặn giao diện.
// Chạy (Windows): dotnet run -- demo <host> <port> <cổng_không_có_server> [tệp_ảnh]
static class DemoGui
{
    static void Log(string text) => Console.WriteLine("[gui] " + text);

    static async Task WaitUntil(Func<bool> cond)                         // chờ điều kiện, không chặn luồng giao diện
    {
        for (int i = 0; i < 200; i++)
        {
            if (cond()) return;
            await Task.Delay(50);
        }
        throw new TimeoutException("Kịch bản chờ quá lâu");
    }

    static Bitmap Snapshot(Form f)                                       // chụp cửa sổ (kể cả thanh tiêu đề)
    {
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        return bmp;
    }

    public static async Task RunAsync(LookupForm w, int port, int bad, string outPng)
    {
        await Task.Delay(800);
        w.TxtPort.Text = bad.ToString();
        w.BtnConnect.PerformClick();
        await WaitUntil(() => w.BtnConnect.Enabled);
        Log($"Kết nối cổng {bad}: {w.LblStatus.Text}");

        w.TxtPort.Text = port.ToString();
        w.BtnConnect.PerformClick();
        await WaitUntil(() => w.BtnLookup.Enabled);
        Log($"Kết nối cổng {port}: {w.LblStatus.Text}");

        w.TxtTerm.Text = "TCP";
        int t0 = w.Ticks;
        w.BtnLookup.PerformClick();
        await Task.Delay(600);
        Log($"Đang chờ phản hồi: trạng thái '{w.LblStatus.Text}', nút Tra cứu {(w.BtnLookup.Enabled ? "normal" : "disabled")}");
        using var a = Snapshot(w);
        await WaitUntil(() => w.LblStatus.Text != "Đang tra cứu...");
        Log($"Kết quả: {w.TxtResult.Text} – trạng thái '{w.LblStatus.Text}'");
        Log($"Trong lúc chờ, đồng hồ giao diện được vẽ lại {w.Ticks - t0} lần (không đứng hình)");
        await Task.Delay(150);
        using var b = Snapshot(w);

        using (var both = new Bitmap(a.Width + b.Width + 24, Math.Max(a.Height, b.Height) + 16))
        using (var g = Graphics.FromImage(both))
        {
            g.Clear(Color.FromArgb(0xF5, 0xF5, 0xF5));
            g.DrawImage(a, 8, 8);
            g.DrawImage(b, a.Width + 16, 8);
            both.Save(outPng, ImageFormat.Png);
        }
        Log("Đã chụp cửa sổ: " + Path.GetFileName(outPng));

        w.TxtTerm.Text = "XYZ";                                          // phản hồi 404
        w.BtnLookup.PerformClick();
        await Task.Delay(300);
        await WaitUntil(() => w.BtnLookup.Enabled);
        Log($"Tra cứu 'XYZ': {w.TxtResult.Text} – trạng thái '{w.LblStatus.Text}'");
    }
}
