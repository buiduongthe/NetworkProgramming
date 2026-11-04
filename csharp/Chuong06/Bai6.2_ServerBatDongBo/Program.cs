// Bài 6.2 – Server tra cứu bất đồng bộ: mỗi kết nối một tác vụ (Mã giả 6.3; Mục 6.2.3)
// Chạy:  dotnet run -- server [host] [port]               (mặc định 0.0.0.0 9000)
//        dotnet run -- client [host] [port] [n] [hold_ms] (mặc định 127.0.0.1 9000 5 1000)
using Bai6_2;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "server": return await AsyncServer.RunAsync(rest);
    case "client": return MultiClient.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [...]");
        return 1;
}
