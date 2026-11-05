// Bài mở rộng MR6.2 – ứng dụng giao diện (WinForms, chỉ Windows) cho phòng hội thoại âm thanh.
// Cùng giao thức với bản dòng lệnh ở thư mục cha (chạy chéo được với server/client dòng lệnh mọi ngôn ngữ).
// Micro/loa thật qua winmm.dll (P/Invoke); không có thiết bị thì dùng nguồn giả lập (sin).
//
// Chạy (Windows):  dotnet run -- server_gui [host] [cổng_tcp] [cổng_udp]    (mặc định 127.0.0.1 41620 41621)
//                  dotnet run -- client_gui [host] [cổng_tcp] [tên]
//                  dotnet run -- demo host cổng_tcp cổng_udp [tệp_ảnh]      kịch bản tự động, chụp cửa sổ rồi thoát
using System.Globalization;
using System.Windows.Forms;
using BaiMR6_2;

static class Program
{
    [STAThread]                                                   // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = System.Text.Encoding.UTF8;
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số thập phân dùng dấu chấm
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        string vaiTro = args.FirstOrDefault() ?? "";
        if (vaiTro is not ("server_gui" or "client_gui" or "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server_gui [host] [tcp] [udp] | client_gui [host] [tcp] [tên] | demo host tcp udp [ảnh]");
            return 2;
        }
        string host = args.Length > 1 ? args[1] : "127.0.0.1";
        int tcp = args.Length > 2 ? int.Parse(args[2]) : 41620;
        WinMM.TangDoChinhXacHenGio();                             // nhịp 20 ms cần Sleep chính xác ~1 ms
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        if (vaiTro == "client_gui")
        {
            Application.Run(new FormKhach(host, tcp, args.Length > 3 ? args[3] : "A") { Location = new System.Drawing.Point(80, 80) });
            return 0;
        }
        int udp = args.Length > 3 ? int.Parse(args[3]) : tcp + 1;
        var server = new FormServer(host, tcp, udp);
        int ma = 0;
        if (vaiTro == "demo")
        {
            string anh = args.Length > 4 ? args[4] : "ket_qua_gui.png";
            server.Shown += async (_, _) =>
            {
                try { await DemoGui.RunAsync(server, anh); }
                catch (Exception e) { Console.WriteLine("[demo] Lỗi kịch bản: " + e); ma = 1; }
                server.Close();
            };
        }
        Application.Run(server);                                  // vòng lặp thông điệp trên luồng giao diện
        return ma;
    }
}
