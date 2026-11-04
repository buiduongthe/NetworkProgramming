// Bài 6.1 – Server tra cứu đa luồng (Mã giả 6.1) và server nhóm luồng (Mã giả 6.2); Mục 6.2.1–6.2.2
// Chạy:  dotnet run -- threaded [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
//        dotnet run -- pool [host] [port] [k] [queue_size]     (mặc định 0.0.0.0 9000 8 16)
//        dotnet run -- client [host] [port] [n] [hold_ms]      (mặc định 127.0.0.1 9000 5 1000)
using Bai6_1;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "threaded": return ThreadedServer.Run(rest);
    case "pool": return PoolServer.Run(rest);
    case "client": return MultiClient.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- threaded|pool|client [host] [port] [...]");
        return 1;
}
