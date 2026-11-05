// Bài mở rộng MR5.1 – Truyền nhiều tệp đồng thời: start/pause/resume/stop, giới hạn tốc độ theo phiên và
// theo tệp (thùng token), tải tiếp bằng offset + SHA-256. Bài mở rộng, không có trong giáo trình;
// nối tiếp Bài 5.5, 5.1 và Bài 3.9, 3.10.
//
// Chạy:  dotnet run -- server <host> <port> [thư_mục_lưu]
//        dotnet run -- client <host> <port> [thư_mục_tải]     (nhập lệnh start/pause/resume/stop/speed/status/quit)
//        dotnet run -- demo <host> <port>                     (tự dựng server cục bộ, chạy kịch bản tự động)
using System.Text;
using BaiMR5_1;

Console.OutputEncoding = Encoding.UTF8;
if (args.Length < 3)
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client|demo <host> <port> [thư_mục]");
    return 2;
}
string host = args[1];
int port = int.Parse(args[2]);
return args[0] switch
{
    "server" => TruyenServer.Run(args.Skip(1).ToArray()),
    "client" => TruyenClient.ChayGiaoDien(host, port, args.Length > 3 ? args[3] : "tai_ve"),
    "demo" => TruyenClient.ChayDemo(host, port),
    _ => 2,
};
