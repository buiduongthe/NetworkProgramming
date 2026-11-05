// Bài mở rộng MR4.2 – Đua xe thời gian thực qua UDP: vòng tick có thẩm quyền, đầu vào có số thứ tự và
// dư thừa, ảnh chụp mới nhất thắng, dự đoán phía client, so sánh mất gói 0 % và 20 %.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 4.1, 4.2, 4.4 và MR4.1.
//
// Chạy:  dotnet run -- server [host] [port] [--so-xe 2] [--mat 0] [--dao 0] [--seed 1] [--so-van 0]
//        dotnet run -- xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]
//        Người lái: gõ g (ga), n (nhả), p (phanh) rồi Enter; đầu vào giữ nguyên tới khi đổi.
using System.Text;
using BaiMR4_2;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerDuaXe.Run(rest);
    case "xe": return Xe.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [port] [tùy chọn]  |  dotnet run -- xe <tên> [host] [port] [tùy chọn]");
        return 2;
}
