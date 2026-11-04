// Bài 5.6 – Yêu cầu có hạn chót, thử lại có lùi theo cấp số nhân và nhiễu ngẫu nhiên
// (Mã giả 5.7, 5.8; Mục 5.4.2, 5.4.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]                          (server "khó tính", mặc định 0.0.0.0 9100)
//        dotnet run -- client [host] [port] [cổng_không_có_server]   (mặc định 127.0.0.1 9100 9101)
using Bai5_6;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client ...");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await FlakyServer.RunAsync(rest) : await DeadlineRetry.RunAsync(rest);
