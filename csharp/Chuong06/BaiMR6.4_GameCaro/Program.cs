// Bài mở rộng MR6.4 – Caro 15×15 nhiều phòng qua TCP: sảnh, phòng, server làm trọng tài, phát nước đi tới người
// xem, hết giờ, chơi lại; khám phá server bằng UDP broadcast. Bài mở rộng, không có trong giáo trình; nối tiếp
// Bài 6.4, 6.5, Bài 3.2 và MR3.3.
//
// Chạy:  dotnet run -- server [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]
//        dotnet run -- choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P]
//                           [--bot KIỂU] [--seed 1] [--thu-loi] [--so-van 1] [--tre-ms 300]
//        dotnet run -- tim [cổng_khám_phá] [thời_gian_chờ_ms] [địa_chỉ_broadcast]   (mặc định 9999 1000 255.255.255.255)
using System.Text;
using BaiMR6_4;

Console.OutputEncoding = Encoding.UTF8;
Console.InputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return ServerCaro.Run(rest);
    case "choi": return NguoiChoi.Run(rest);
    case "tim":
        // Tìm server Caro trong LAN bằng UDP broadcast (DISCOVER của Bài 6.5)
        int port = rest.Length > 0 ? int.Parse(rest[0]) : 9999;
        int waitMs = rest.Length > 1 ? int.Parse(rest[1]) : 1000;
        string bcast = rest.Length > 2 ? rest[2] : "255.255.255.255";
        Console.WriteLine($"[tim] Phát discover tới {bcast}:{port}, chờ {waitMs} ms");
        var found = GiaoThuc.Discover(port, waitMs, bcast);
        Console.WriteLine($"[tim] Tìm thấy {found.Count} server:");
        foreach (var (ip, sport, name, rooms, players) in found)
            Console.WriteLine($"[tim]   {name} tại {ip}:{sport} – {rooms} phòng, {players} người");
        return 0;
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [...] | choi <tên> [...] | tim [cổng] [ms] [broadcast]");
        return 2;
}
