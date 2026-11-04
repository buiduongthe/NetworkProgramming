// Bài 4.5 – Xác thực địa chỉ nguồn bằng mã thách thức không trạng thái (Mã giả 4.8; Mục 4.6.1 Giáo trình Lập trình mạng)
// Chạy (trong thư mục bài, nơi có glossary.json):
//        dotnet run -- server [host] [port]                    (mặc định 0.0.0.0 9100)
//        dotnet run -- client [host] [port] [cookie_ban_dau]   (mặc định 127.0.0.1 9100)
using Bai4_5;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] | client [host] [port] [cookie_ban_dau]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? CookieServer.Run(rest) : CookieClient.Run(rest);
