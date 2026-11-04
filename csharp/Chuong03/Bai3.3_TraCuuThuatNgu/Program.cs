// Bài 3.3 – Dịch vụ tra cứu thuật ngữ dùng khung JSON (Mã giả 3.7, 3.8; Mục 3.5.1, 3.5.3 Giáo trình Lập trình mạng)
// Chạy trong thư mục bài (server đọc glossary.json ở thư mục hiện hành):
//        dotnet run -- server  [host] [port]      (mặc định 0.0.0.0 9100)
//        dotnet run -- client  [host] [port]      (mặc định 127.0.0.1 9100)
//        dotnet run -- kiemthu [host] [port]      (client kiểm thử, mặc định 127.0.0.1 9100)
using Bai3_3;

string[] roles = ["server", "client", "kiemthu"];
if (args.Length == 0 || !roles.Contains(args[0]))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|kiemthu [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] switch
{
    "server" => LookupServer.Run(rest),
    "client" => LookupClient.Run(rest),
    _ => KiemThu.Run(rest),
};
