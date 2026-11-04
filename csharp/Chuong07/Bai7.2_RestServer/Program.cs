// Bài 7.2 – Server REST tối giản trên HTTP/1.1 (Mã giả 7.2; Mục 7.2.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]      (mặc định 0.0.0.0 8080)
//        dotnet run -- client [host] [port]      (mặc định 127.0.0.1 8080; đọc yêu cầu từ bàn phím)
using Bai7_2;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await RestServer.RunAsync(rest) : await RestClient.RunAsync(rest);
