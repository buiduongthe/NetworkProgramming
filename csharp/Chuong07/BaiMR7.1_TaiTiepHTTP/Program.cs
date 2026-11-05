// Bài mở rộng MR7.1 – Tải tệp HTTP có tải tiếp: Range, If-Range, 206/416, thử lại, SHA-256.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 7.1 (client HTTP), Bài 5.6 (thử lại có lùi thời gian)
// và Bài 3.4 (kiểm tra SHA-256).
//
// Chạy:  dotnet run -- server [host] [port] [--kich-thuoc B] [--cat-sau B] [--so-lan-cat N]
//        dotnet run -- tai <url> <tệp_lưu> [--so-lan-thu N]
using System.Text;
using BaiMR7_1;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerTep.Run(rest);
    case "tai" when rest.Length >= 2: return TaiTiep.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] [tùy chọn] | tai <url> <tệp_lưu> [--so-lan-thu N]");
        return 2;
}
