// Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (WinForms, chỉ Windows).
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp có
// tiến trình và hủy), Bài 5.2 (giới hạn đồng thời), Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên qua UDP), MR5.2.
//
// Chạy (Windows):  dotnet run -- server [host] [port]             máy chủ thử (mặc định 127.0.0.1 9530)
//                  dotnet run -- gui [host] [port]                ứng dụng giao diện
//                  dotnet run -- demo host port [tệp_ảnh]         kịch bản tự động, chụp cửa sổ rồi thoát
using System.Globalization;
using System.Windows.Forms;
using BaiMR5_3;

static class Program
{
    [STAThread]                                                       // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = System.Text.Encoding.UTF8;            // in tiếng Việt đúng trên cmd
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số thập phân dùng dấu chấm
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        if (args.Length == 0 || (args[0] != "server" && args[0] != "gui" && args[0] != "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server|gui|demo [host] [port] [tệp_ảnh]");
            return 1;
        }
        var rest = args.Skip(1).ToArray();
        if (args[0] == "server") return MayChuThu.Run(rest);
        string host = rest.Length > 0 ? rest[0] : "127.0.0.1";
        int port = rest.Length > 1 ? int.Parse(rest[1]) : 9530;
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var form = new UngDung(host, port);
        int code = 0;
        if (args[0] == "demo")
        {
            string png = rest.Length > 2 ? rest[2] : "ket_qua_gui.png";
            form.Shown += async (_, _) =>
            {
                try { await DemoGui.RunAsync(form, png); }
                catch (Exception e) { Console.WriteLine("[gui] Lỗi kịch bản: " + e.Message); code = 1; }
                form.Close();
            };
        }
        Application.Run(form);                                        // vòng lặp thông điệp trên luồng giao diện
        return code;
    }
}
