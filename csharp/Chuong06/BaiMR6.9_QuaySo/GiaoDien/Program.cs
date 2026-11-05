// Bài mở rộng MR6.9 – ứng dụng giao diện (WinForms, chỉ Windows) cho quay số trúng thưởng: bảng điều khiển máy chủ và
// vòng quay may mắn. Cùng giao thức với bản dòng lệnh ở thư mục cha (chạy chéo được với máy chủ/người dùng/bộ mô
// phỏng dòng lệnh mọi ngôn ngữ).
//
// Chạy (Windows):  dotnet run -- server_gui [host] [cổng]             (mặc định 127.0.0.1 42732; 10 quà, p = 1/2000)
//                  dotnet run -- vong_quay_gui [tên] [host] [cổng]
//                  dotnet run -- demo host cổng [tệp_ảnh]               kịch bản tự động, chụp cửa sổ rồi thoát
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;
using BaiMR6_9;

static class Program
{
    [STAThread]                                                   // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = System.Text.Encoding.UTF8;
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        WinMM.TangDoChinhXacHenGio();                             // Thread.Sleep chính xác 1 ms cho độ trễ giả lập 0–2 ms
        string vaiTro = args.FirstOrDefault() ?? "";
        if (vaiTro is not ("server_gui" or "vong_quay_gui" or "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server_gui [host] [cổng] | vong_quay_gui [tên] [host] [cổng] | demo host cổng [ảnh]");
            return 2;
        }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        string A(int i, string md) => args.Length > i ? args[i] : md;
        if (vaiTro == "vong_quay_gui")
        {
            NhatKy.InRa = true;                                   // chạy riêng: nhật ký cũng ra dòng lệnh
            var f = new FormVongQuay(A(1, "An"), A(2, "127.0.0.1"), int.Parse(A(3, "42732"), CultureInfo.InvariantCulture))
            { Location = new Point(80, 80) };
            Application.Run(f);
            return 0;
        }
        string host = A(1, "127.0.0.1");
        int cong = int.Parse(A(2, "42732"), CultureInfo.InvariantCulture);
        var sv = new FormServerQuaySo(host, cong) { Location = new Point(0, 0) };
        int ma = 0;
        if (vaiTro == "demo")
        {
            string anh = A(3, "ket_qua_gui.png");
            var ds = new List<FormVongQuay>();
            sv.Shown += async (_, _) =>
            {
                int x = sv.Right + 20, y = 0;
                foreach (var (i, ten) in new[] { "An", "Bình", "Bình" }.Index())
                {
                    if (i == 1) (x, y) = (0, Math.Max(sv.Bottom, ds[0].Bottom) + 20);   // Bình, Bình (máy 2): hàng dưới
                    var f = new FormVongQuay(ten, host, cong) { Location = new Point(x, y) };
                    if (i == 2)                                   // cùng người, thiết bị thứ hai
                    {
                        f.NhanHien = "Bình (máy 2)";
                        f.Text = "Vòng quay may mắn – Bình (máy 2)";
                    }
                    f.Show();
                    ds.Add(f);
                    x = f.Right + 20;
                }
                try { await DemoGui.RunAsync(sv, ds, anh); }
                catch (Exception e)
                {
                    Console.WriteLine($"[demo] {(e is TimeoutException ? e.Message : "Lỗi kịch bản: " + e)}");
                    ma = 1;
                }
                foreach (var f in ds) f.Close();
                sv.Close();
                Console.WriteLine("[demo] Kết thúc");
            };
        }
        else
        {
            NhatKy.InRa = true;
            sv.Location = new Point(80, 80);
        }
        Application.Run(sv);                                      // vòng lặp thông điệp trên luồng giao diện
        return ma;
    }
}
