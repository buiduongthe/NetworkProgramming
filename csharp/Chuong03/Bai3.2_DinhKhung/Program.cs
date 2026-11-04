// Bài 3.2 – Thư viện định khung và chương trình minh họa (Mã giả 3.3–3.6; Mục 3.2.4, 3.4 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]               (mặc định 0.0.0.0 9000)
//        dotnet run -- client [host] [port] [so_ky_tu]    (mặc định 127.0.0.1 9000 0)
//        dotnet run -- line   [host] [port]               (minh họa READ_LINE, mặc định 127.0.0.1 9000)
using Bai3_2;

string[] roles = ["server", "client", "line"];
if (args.Length == 0 || !roles.Contains(args[0]))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|line [host] [port] [so_ky_tu]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] switch
{
    "server" => FrameServer.Run(rest),
    "client" => FrameClient.Run(rest),
    _ => LineDemo.Run(rest),
};
