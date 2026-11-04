// Bài 6.7 – Kiểm thử tải: bộ sinh tải đo thông lượng và p50/p95/p99 (Mã giả 6.9; Mục 6.5.1)
// kèm bản sao hai server của Bài 6.1 (Mã giả 6.1, 6.2) có độ trễ giả lập.
// Chạy:  dotnet run -- threaded [host] [port] [max_clients] [delay_ms]   (mặc định 0.0.0.0 9000 500 50)
//        dotnet run -- pool [host] [port] [k] [queue_size] [delay_ms]    (mặc định 0.0.0.0 9000 8 16 50)
//        dotnet run -- load [host] [port] [n_requests] [levels]          (mặc định 127.0.0.1 9000 20 1,10,100)
using Bai6_7;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "threaded": return ThreadedServer.Run(rest);
    case "pool": return PoolServer.Run(rest);
    case "load": return await LoadTest.RunAsync(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- threaded|pool|load [host] [port] [...]");
        return 1;
}
