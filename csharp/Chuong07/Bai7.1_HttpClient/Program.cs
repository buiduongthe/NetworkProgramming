// Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP (Mã giả 7.1; Mục 7.2.1 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- client [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
//        dotnet run -- server [host] [port]                  (server thử, mặc định 0.0.0.0 8080)
using Bai7_1;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- client|server [host] [port] [đường_dẫn ...]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? ServerThu.Run(rest) : MiniHttpClient.Run(rest);
