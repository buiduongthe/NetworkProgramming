// Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ (Mã giả 7.6; Mục 7.4.3 Giáo trình Lập trình mạng)
// Cần gói NuGet: Grpc.AspNetCore, Grpc.Net.Client, Google.Protobuf, Grpc.Tools (xem README)
// Chạy:  dotnet run -- server [host] [port] [độ_trễ_ms]   (mặc định 0.0.0.0 5001 0)
//        dotnet run -- client [host] [port] [hạn_chót_ms]  (mặc định 127.0.0.1 5001 2000)
using Bai7_5;

Console.OutputEncoding = System.Text.Encoding.UTF8;               // in tiếng Việt đúng trên Windows

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [ms]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await GlossaryHost.RunAsync(rest) : await GlossaryCaller.RunAsync(rest);
