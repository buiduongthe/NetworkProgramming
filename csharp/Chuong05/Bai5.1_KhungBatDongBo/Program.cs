// Bài 5.1 – Các hàm gửi, nhận khung JSON bất đồng bộ (Mã giả 5.1; Mục 5.2.4 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]      (mặc định 0.0.0.0 9100)
//        dotnet run -- client [host] [port]      (mặc định 127.0.0.1 9100)
using Bai5_1;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await FrameServer.RunAsync(rest) : await FrameClient.RunAsync(rest);
