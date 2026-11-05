// Bài mở rộng MR6.8 – ứng dụng giao diện (WinForms, chỉ Windows) cho đặt vé xem phim: bảng điều khiển máy
// chủ và ứng dụng đặt vé (sơ đồ ghế). Cùng giao thức với bản dòng lệnh ở thư mục cha (chạy chéo được với máy chủ/người dùng/bộ mô
// phỏng dòng lệnh mọi ngôn ngữ).
//
// Chạy (Windows):  dotnet run -- server_gui [host] [cổng]             (mặc định 127.0.0.1 42722; giữ chỗ 30 s)
//                  dotnet run -- dat_ve_gui [tên] [host] [cổng]
//                  dotnet run -- demo host cổng [tệp_ảnh]               kịch bản tự động, chụp cửa sổ rồi thoát
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;
using BaiMR6_8;

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
        if (vaiTro is not ("server_gui" or "dat_ve_gui" or "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server_gui [host] [cổng] | dat_ve_gui [tên] [host] [cổng] | demo host cổng [ảnh]");
            return 2;
        }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        string A(int i, string md) => args.Length > i ? args[i] : md;
        if (vaiTro == "dat_ve_gui")
        {
            NhatKy.InRa = true;                                   // chạy riêng: nhật ký cũng ra dòng lệnh
            var f = new FormDatVe(A(1, "An"), A(2, "127.0.0.1"), int.Parse(A(3, "42722"), CultureInfo.InvariantCulture))
            { Location = new Point(80, 80) };
            Application.Run(f);
            return 0;
        }
        string host = A(1, "127.0.0.1");
        int cong = int.Parse(A(2, "42722"), CultureInfo.InvariantCulture);
        var sv = new FormServerDatVe(host, cong) { Location = new Point(0, 0) };
        int ma = 0;
        if (vaiTro == "demo")
        {
            string anh = A(3, "ket_qua_gui.png");
            var ds = new List<FormDatVe>();
            sv.Shown += async (_, _) =>
            {
                int x = sv.Right + 20, y = 0;
                foreach (var (i, ten) in new[] { "An", "Bình", "Chi" }.Index())
                {
                    if (i == 1) (x, y) = (0, Math.Max(sv.Bottom, ds[0].Bottom) + 20);   // Bình, Chi: hàng dưới
                    var f = new FormDatVe(ten, host, cong) { Location = new Point(x, y) };
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
