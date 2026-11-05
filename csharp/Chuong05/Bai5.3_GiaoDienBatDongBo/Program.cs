// Bài 5.3 – Ứng dụng tra cứu có giao diện WinForms (Mã giả 5.3, 5.4; Mục 5.3.2 Giáo trình Lập trình mạng)
// Chạy (Windows):  dotnet run -- server [host] [port] [delay_ms]   (mặc định 0.0.0.0 9100 0)
//                  dotnet run -- gui [host] [port]                 (mặc định 127.0.0.1 9100)
//                  dotnet run -- demo host port bad_port [tệp_ảnh] (kịch bản tự động, chụp cửa sổ)
using System.Windows.Forms;
using Bai5_3;

static class Program
{
    [STAThread]                                                          // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        Console.OutputEncoding = System.Text.Encoding.UTF8;               // in tiếng Việt đúng trên cmd
        if (args.Length == 0 || (args[0] != "server" && args[0] != "gui" && args[0] != "demo"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server|gui|demo [host] [port] ...");
            return 1;
        }
        var rest = args.Skip(1).ToArray();
        if (args[0] == "server")
            return LookupServer.RunAsync(rest).GetAwaiter().GetResult();
        var (host, port) = Net.ParseEndpoint(rest, "127.0.0.1", 9100);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        var form = new LookupForm(host, port);
        int code = 0;
        if (args[0] == "demo")                                           // demo host port bad_port [tệp_ảnh]
        {
            int bad = int.Parse(rest[2]);
            string png = rest.Length > 3 ? rest[3] : "ket_qua_gui.png";
            form.Shown += async (_, _) =>
            {
                try { await DemoGui.RunAsync(form, port, bad, png); }
                catch (Exception e) { Console.WriteLine("[gui] Lỗi kịch bản: " + e.Message); code = 1; }
                form.Close();
            };
        }
        Application.Run(form);                                           // vòng lặp thông điệp trên luồng giao diện
        return code;
    }
}
