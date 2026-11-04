// Bài 3.4 – Tải tệp lên server có kiểm tra SHA-256 (Mã giả 3.9, 3.10; Mục 3.5.2 Giáo trình Lập trình mạng)
// Chạy trong thư mục bài (server đọc glossary.json, lưu tệp vào uploads/ ở thư mục hiện hành):
//        dotnet run -- server  [host] [port]      (mặc định 0.0.0.0 9100)
//        dotnet run -- client  [host] [port]      (mặc định 127.0.0.1 9100; lệnh: put <tệp> | <thuật ngữ>)
//        dotnet run -- kiemthu [host] [port]      (client kiểm thử, mặc định 127.0.0.1 9100)
using Bai3_4;

string[] roles = ["server", "client", "kiemthu"];
if (args.Length == 0 || !roles.Contains(args[0]))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|kiemthu [host] [port]");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] switch
{
    "server" => PutServer.Run(rest),
    "client" => PutClient.Run(rest),
    _ => KiemThu.Run(rest),
};
