// Bài mở rộng MR3.1 – Bộ chuyển tiếp cổng TCP có chuyển tiếp đóng một chiều (half-close).
//
// Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (vòng đời socket TCP, SEND_ALL,
// SHUTDOWN một chiều) và Chương 6 (mỗi kết nối một luồng).
//
// Chạy:  dotnet run -- dich <host> <port>                                  server đích "chữ hoa"
//        dotnet run -- chuyen_tiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>
//        dotnet run -- client <host> <port> <chuỗi> [số_lần_lặp]           client thử
using System.Text;
using BaiMR3_1;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
return args.FirstOrDefault() switch
{
    "dich" => ServerDich.Run(rest),
    "chuyen_tiep" when rest.Length >= 4 => ChuyenTiep.Run(rest),
    "client" when rest.Length >= 3 => ClientThu.Run(rest),
    _ => Usage(),
};

static int Usage()
{
    Console.WriteLine("Cách dùng: dotnet run -- dich [host] [port]");
    Console.WriteLine("           dotnet run -- chuyen_tiep <host_nghe> <cổng_nghe> <host_đích> <cổng_đích>");
    Console.WriteLine("           dotnet run -- client <host> <port> <chuỗi> [số_lần_lặp]");
    return 2;
}
