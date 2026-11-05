// Bài mở rộng MR4.1 – Thu thập, giải mã NetFlow v5 trên UDP, phát hiện mất gói, thống kê lưu lượng.
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 4 (server UDP, giải mã nhị phân, số thứ tự).
//
// Chạy:  dotnet run -- thu_thap [host] [port] [--nhan-roi S]            bộ thu thập (mặc định 127.0.0.1 2055)
//        dotnet run -- xuat <host> <port> [--so-luong N] [--bo-goi K]   máy xuất giả lập
using System.Text;
using BaiMR4_1;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "thu_thap": return BoThuThap.Run(rest);
    case "xuat": return MayXuat.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- thu_thap [host] [port] [--nhan-roi S]");
        Console.WriteLine("           dotnet run -- xuat <host> <port> [--so-luong N] [--bo-goi K]");
        return 2;
}
