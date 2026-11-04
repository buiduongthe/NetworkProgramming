// Bài 7.4 – Bộ đọc luồng Server-Sent Events và server SSE nhỏ (Mã giả 7.5; Mục 7.3.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]               (mặc định 0.0.0.0 8080)
//        dotnet run -- client [host] [port] [đường_dẫn]   (mặc định 127.0.0.1 8080 /events)
using Bai7_4;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [đường_dẫn]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? SseServer.Run(rest) : SseClient.Run(rest);
