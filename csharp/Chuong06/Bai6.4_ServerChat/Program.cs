// Bài 6.4 – Server chat nhiều phòng (Mã giả 6.5, 6.6; Mục 6.3) và client dòng lệnh dùng ChatClient (Mã giả 5.5)
// Chạy:  dotnet run -- server [host] [port]          (mặc định 0.0.0.0 9200)
//        dotnet run -- client [host] [port] [tên]    (mặc định 127.0.0.1 9200 user)
using Bai6_4;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "server": return await ChatServer.RunAsync(rest);
    case "client": return await ChatClientApp.RunAsync(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server|client [host] [port] [tên]");
        return 1;
}
