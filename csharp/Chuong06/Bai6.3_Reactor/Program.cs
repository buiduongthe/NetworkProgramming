// Bài 6.3 – Server echo một luồng dùng dồn kênh vào/ra, mô hình reactor (Mã giả 6.4; Mục 6.2.4)
// Chạy:  dotnet run -- server [host] [port]                 (mặc định 0.0.0.0 9000)
//        dotnet run -- client [host] [port] [n] [size]      (mặc định 127.0.0.1 9000 3 1048576)
using Bai6_3;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "server": return ReactorServer.Run(rest);
    case "client": return EchoClients.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [...]");
        return 1;
}
