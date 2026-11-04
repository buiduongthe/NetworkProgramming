// Bài 3.1 – TCP Echo Server tuần tự và TCP Echo Client (Mã giả 3.1, 3.2; Mục 3.2 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]      (mặc định 0.0.0.0 9000)
//        dotnet run -- client [host] [port]      (mặc định 127.0.0.1 9000)
using Bai3_1;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? EchoServer.Run(rest) : EchoClient.Run(rest);
