// Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ: ứng dụng giao diện WinForms (chỉ Windows).
//
// Chạy (Windows, trong thư mục GiaoDien):
//     dotnet run                                  ứng dụng giao diện
//     dotnet run -- demo [tệp_ảnh]               kịch bản tự thao tác, chụp cửa sổ (mặc định ..\ket_qua_gui.png)
using System.Globalization;
using System.Windows.Forms;
using BaiMR5_6;

static class Program
{
    [STAThread]                                                       // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = new System.Text.UTF8Encoding(false);  // in tiếng Việt đúng trên cmd
        CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số thập phân dùng dấu chấm
        CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
        bool demo = args.Length > 0 && (args[0] == "demo" || args[0] == "--demo");
        if (args.Length > 0 && !demo)
        {
            Console.WriteLine("Cách dùng: dotnet run [-- demo [tệp_ảnh]]");
            return 1;
        }
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var form = new UngDung();
        int code = 0;
        if (demo)
        {
            string png = args.Length > 1 ? args[1] : Path.Combine("..", "ket_qua_gui.png");
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
