// Bài mở rộng MR3.3 – Trò chơi đoán số nhiều người chơi qua TCP: giao thức yêu cầu/phản hồi khung JSON,
// máy trạng thái, kiểm tra đầu vào, hết giờ, mất kết nối. Bài mở rộng, không có trong giáo trình;
// nối tiếp Bài 3.2 (định khung), 3.3 (giao thức JSON), 6.1 (mỗi kết nối một luồng).
//
// Chạy:  dotnet run -- server [host] [port] [--so-nguoi 2] [--het-gio-ms 30000] [--seed 0]
//                             [--min 1] [--max 100] [--so-van 0]
//        (--seed 0: lấy hạt giống theo thời gian; --so-van 0: chơi mãi)
//        dotnet run -- choi <tên> [host] [port] [--bot chia_doi|ngau_nhien|nghich|im_lang] [--seed N] [--nghi-ms 300]
using System.Text;
using BaiMR3_3;

Console.OutputEncoding = Encoding.UTF8;
Console.InputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerDoanSo.Run(rest);
    case "choi": return NguoiChoi.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] [tùy chọn]  |  dotnet run -- choi <tên> [host] [port] [tùy chọn]");
        return 2;
}
