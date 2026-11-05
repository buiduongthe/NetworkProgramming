// Bài mở rộng MR6.2 – Phòng hội thoại âm thanh nhiều client: RTP trên UDP, điều khiển JSON trên TCP, trộn
// mix-minus, bộ đệm jitter, đo dBFS, tắt mic/độ lợi/âm lượng, ghi WAV, phân tích Goertzel.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1, 6.4 (server nhiều client, chat), Bài 4.1 (UDP)
// và Bài 3.3 (định khung thông điệp). Bản C# chỉ dùng âm tổng hợp (sin) hoặc tệp WAV – không có micro/loa.
//
// Chạy:  dotnet run -- server [host] [cổng_tcp] [cổng_udp]
//        dotnet run -- khach <tên> <host> <cổng_tcp> [--tan-so 440] [--bien-do 0.25] [--wav tệp.wav]
//                            [--ghi out/ghi.wav] [--thoi-gian 10] [--mat-goi 0.05] [--dem 3]
//        dotnet run -- phantich <tệp.wav> <tần_số:nhãn> ... [--cua-so 1.0] [--nguong -50]
using System.Text;
using BaiMR6_2;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerAmThanh.Run(rest);
    case "khach" when rest.Length >= 3: return KhachAmThanh.Run(rest);
    case "phantich" when rest.Length >= 2: return PhanTichWav.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng_tcp] [cổng_udp] | khach <tên> <host> <cổng_tcp> [tùy chọn] | phantich <tệp.wav> <tần_số:nhãn> ...");
        return 2;
}
