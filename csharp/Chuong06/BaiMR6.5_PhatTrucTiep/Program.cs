// Bài mở rộng MR6.5 – Phát trực tiếp âm thanh + video 1→n (livestream): một máy phát -> máy chủ trung chuyển
// (SFU, nhân bản gói) -> n người xem; khung I/P (zlib + XOR), bộ đệm GOP, xin khung khóa (PLI), đồng bộ tiếng-hình
// với âm thanh làm đồng hồ chủ, đăng ký theo lớp.
// Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2 (âm thanh RTP), MR6.3 (video chia mảnh), Bài 6.1 (server
// đa luồng), Bài 6.6 (một nguồn – nhiều người nhận).
//
// Chạy:  dotnet run -- server [host] [cổng_tcp] [cổng_udp]                 (mặc định cổng_udp = cổng_tcp + 1)
//        dotnet run -- phat <tên> <host> <cổng_tcp> [--camera gia_lap|testsrc|<thiết bị>] [--mic gia_lap|<chỉ số>]
//                           [--tep <tệp video> [--tao-tep-mau]] [--fps 10] [--gop 20] [--thoi-gian 30] [--mat-goi 0.0]
//        dotnet run -- xem <tên> <host> <cổng_tcp> [--thoi-gian 10] [--tre 0] [--dem 5] [--khong-dong-bo]
//                          [--chi-am-thanh] [--chi-video] [--mat-goi 0.0] [--ghep ket_qua_video.png]
using System.Globalization;
using System.Text;
using BaiMR6_5;

Console.OutputEncoding = Encoding.UTF8;
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số in dạng 1.5 (không phải 1,5) mọi máy
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
WinMM.TangDoChinhXacHenGio();                // Windows: Thread.Sleep chính xác 1 ms (Linux: bỏ qua)
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerPhat.Run(rest);
    case "phat" when rest.Length >= 3: return Phat.Run(rest);
    case "xem" when rest.Length >= 3: return Xem.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng_tcp] [cổng_udp] | phat <tên> <host> <cổng_tcp> [tùy chọn]"
                          + " | xem <tên> <host> <cổng_tcp> [tùy chọn]");
        return 2;
}
