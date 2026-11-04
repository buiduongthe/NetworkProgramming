// Bài 4.4 – Truyền tệp dừng và chờ trên UDP (Mã giả 4.6, 4.7; Mục 4.4.1–4.4.2 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- recv [host] [port] [tep_ra] [mat_goi]       (mặc định 0.0.0.0 9000 tep_nhan.bin)
//        dotnet run -- send <tep> [host] [port] [mat_goi]         (mặc định 127.0.0.1 9000)
// mat_goi: "0.1" (ngẫu nhiên 10%) hoặc "3,7" (mất datagram gửi đi thứ 3 và 7); bỏ trống: không mất gói
using Bai4_4;

if (args.Length == 0 || (args[0] != "send" && args[0] != "recv") || (args[0] == "send" && args.Length < 2))
{
    Console.WriteLine("Cách dùng: dotnet run -- recv [host] [port] [tep_ra] [mat_goi] | send <tep> [host] [port] [mat_goi]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "send" ? SawSender.Run(rest) : SawReceiver.Run(rest);
