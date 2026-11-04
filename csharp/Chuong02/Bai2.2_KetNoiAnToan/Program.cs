// Bài 2.2 – Kết nối có thời gian chờ và phân loại lỗi (Mã giả 2.2 SAFE_CONNECT; Mục 2.2.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- client [host] [port] [timeout_ms]   (mặc định 127.0.0.1 9000 3000)
//        dotnet run -- server [host] [port]                (server thử nghiệm, mặc định 0.0.0.0 9000)
using Bai2_2;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [timeout_ms]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? TestServer.Run(rest) : Client.Run(rest);
