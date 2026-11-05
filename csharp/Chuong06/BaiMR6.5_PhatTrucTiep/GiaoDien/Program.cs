// Bài mở rộng MR6.5 – ứng dụng giao diện (WinForms, chỉ Windows) cho phát trực tiếp 1→n: cửa sổ máy chủ (SFU),
// máy phát, người xem. Cùng giao thức với bản dòng lệnh ở thư mục cha (chạy chéo được với máy chủ/máy phát/người xem
// dòng lệnh mọi ngôn ngữ). Camera thật và tệp video qua tiến trình ffmpeg; micro/loa thật qua winmm.
//
// Chạy (Windows):  dotnet run -- server_gui [host] [cổng_tcp] [cổng_udp]   (mặc định 127.0.0.1 42652 42653)
//                  dotnet run -- phat_gui [tên] [host] [cổng_tcp]
//                  dotnet run -- xem_gui [tên] [host] [cổng_tcp]
//                  dotnet run -- demo host cổng_tcp cổng_udp [tệp_ảnh]     kịch bản tự động, chụp cửa sổ rồi thoát
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;
using BaiMR6_5;

static class Program
{
    [STAThread]                                                   // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = System.Text.Encoding.UTF8;
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        WinMM.TangDoChinhXacHenGio();                             // Thread.Sleep chính xác 1 ms cho nhịp 20 ms
        string vaiTro = args.FirstOrDefault() ?? "";
        if (vaiTro is not ("server_gui" or "phat_gui" or "xem_gui" or "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server_gui [host] [tcp] [udp] | phat_gui [tên] [host] [tcp]"
                              + " | xem_gui [tên] [host] [tcp] | demo host tcp udp [ảnh]");
            return 2;
        }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        string A(int i, string md) => args.Length > i ? args[i] : md;
        if (vaiTro is "phat_gui" or "xem_gui")
        {
            NhatKy.InRa = true;                                   // chạy riêng: nhật ký cũng ra dòng lệnh
            int cong = int.Parse(A(3, "42652"), CultureInfo.InvariantCulture);
            Form f = vaiTro == "phat_gui" ? new FormPhat(A(1, "P"), A(2, "127.0.0.1"), cong) : new FormXem(A(1, "A"), A(2, "127.0.0.1"), cong);
            f.Location = new Point(80, 80);
            Application.Run(f);
            return 0;
        }
        string host = A(1, "127.0.0.1");
        int tcp = int.Parse(A(2, "42652"), CultureInfo.InvariantCulture);
        int udp = int.Parse(A(3, (tcp + 1).ToString(CultureInfo.InvariantCulture)), CultureInfo.InvariantCulture);
        var sv = new FormServer(host, tcp, udp) { Location = new Point(0, 0) };
        int ma = 0;
        if (vaiTro == "demo")
        {
            string anh = A(4, "ket_qua_gui.png");
            sv.Shown += async (_, _) =>
            {
                try { await DemoGui.RunAsync(sv, host, tcp, anh); }
                catch (Exception e)
                {
                    Console.WriteLine($"[demo] {(e is TimeoutException ? e.Message : "Lỗi kịch bản: " + e)}");
                    ma = 1;
                }
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
