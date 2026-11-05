// Bài mở rộng MR6.3 – Phòng hội thoại video nhiều client: khung nén zlib chia mảnh ≤ 1200 byte trên UDP,
// server ghép lưới (MCU), tắt/bật camera, giới hạn fps/độ phân giải gửi và nhận, chụp lưới PNG.
// Bài mở rộng, không có trong giáo trình; nối tiếp MR6.2, Bài 6.1, 6.4 và Bài 4.1.
// Bản C# chỉ dùng camera tổng hợp (BCL không có API camera); PNG ghi tay bằng ZLibStream + CRC-32.
//
// Chạy:  dotnet run -- server [host] [cổng_tcp] [cổng_udp]
//        dotnet run -- khach <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10] [--mat-goi 0.02]
//                            [--ghep ket_qua_video.png]
using System.Text;
using BaiMR6_3;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerVideo.Run(rest);
    case "khach" when rest.Length >= 3: return KhachVideo.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng_tcp] [cổng_udp] | khach <tên> <host> <cổng_tcp> [tùy chọn]");
        return 2;
}
