// Bài 7.7 – Gửi ICMP Echo Request và đo thời gian khứ hồi (Mã giả 7.8; Mục 7.5.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- raw [host] [số_lần] [thời_gian_chờ_ms]   socket RAW theo mã giả, cần quyền quản trị
//        dotnet run -- lop [host] [số_lần] [thời_gian_chờ_ms]   lớp System.Net.NetworkInformation.Ping
//        (mặc định 127.0.0.1 4 1000)
using Bai7_7;

if (args.Length == 0 || (args[0] != "raw" && args[0] != "lop"))
{
    Console.WriteLine("Cách dùng: dotnet run -- raw|lop [host] [số_lần] [thời_gian_chờ_ms]");
    return 1;
}
string host = args.Length >= 2 ? args[1] : "127.0.0.1";
int count = args.Length >= 3 ? int.Parse(args[2]) : 4;
int timeoutMs = args.Length >= 4 ? int.Parse(args[3]) : 1000;
return args[0] == "raw" ? RawPing.Run(host, count, timeoutMs) : ClassPing.Run(host, count, timeoutMs);
