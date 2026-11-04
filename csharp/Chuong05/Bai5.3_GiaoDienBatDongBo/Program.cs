// Bài 5.3 – Ứng dụng tra cứu có giao diện WinForms (Mã giả 5.3, 5.4; Mục 5.3.2 Giáo trình Lập trình mạng)
// Chạy (Windows):  dotnet run -- server [host] [port] [delay_ms]   (mặc định 0.0.0.0 9100 0)
//                  dotnet run -- gui [host] [port]                 (mặc định 127.0.0.1 9100)
using System.Windows.Forms;
using Bai5_3;

static class Program
{
    [STAThread]                                                          // WinForms yêu cầu luồng giao diện STA
    static int Main(string[] args)
    {
        if (args.Length == 0 || (args[0] != "server" && args[0] != "gui"))
        {
            Console.WriteLine("Cách dùng: dotnet run -- server|gui [host] [port] ...");
            return 1;
        }
        var rest = args.Skip(1).ToArray();
        if (args[0] == "server")
            return LookupServer.RunAsync(rest).GetAwaiter().GetResult();
        var (host, port) = Net.ParseEndpoint(rest, "127.0.0.1", 9100);
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new LookupForm(host, port));                     // vòng lặp thông điệp trên luồng giao diện
        return 0;
    }
}
