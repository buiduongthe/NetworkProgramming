// Bài mở rộng MR6.1 – Tắt server an toàn khi nhận SIGTERM/SIGINT: ngừng nhận, làm nốt, hạn chờ, hủy.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.2 (server bất đồng bộ) và Bài 5.6 (hạn chót, hủy).
//
// Chạy:  dotnet run -- server [host] [port] [--han-cho-ms 2000]
//        dotnet run -- khach <tên> <host> <port> [ms ...]
using System.Text;
using BaiMR6_1;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return await ServerTatAnToan.Run(rest);
    case "khach" when rest.Length >= 3: return Khach.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] [--han-cho-ms 2000]  |  dotnet run -- khach <tên> <host> <port> [ms ...]");
        return 2;
}
