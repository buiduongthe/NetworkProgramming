// Bài 4.1 – UDP Echo Server và UDP Echo Client có thời gian chờ (Mã giả 4.1, 4.2; Mục 4.2.1 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]      (mặc định 0.0.0.0 9000)
//        dotnet run -- client [host] [port]      (mặc định 127.0.0.1 9000)
using Bai4_1;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? UdpEchoServer.Run(rest) : UdpEchoClient.Run(rest);
