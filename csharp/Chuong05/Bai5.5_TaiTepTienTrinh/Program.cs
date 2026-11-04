// Bài 5.5 – Tải tệp bất đồng bộ có báo cáo tiến trình và hủy (Mã giả 5.6; Mục 5.3.4 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port] [rate_kib]               (mặc định 0.0.0.0 9100 0 = không giới hạn)
//        dotnet run -- client [host] [port] [tệp] [hủy_sau_ms]       (mặc định 127.0.0.1 9100 du_lieu_thu.bin)
using Bai5_5;

if (args.Length == 0 || (args[0] != "server" && args[0] != "client"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|client ...");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await PutServer.RunAsync(rest) : await PutClient.RunAsync(rest);
