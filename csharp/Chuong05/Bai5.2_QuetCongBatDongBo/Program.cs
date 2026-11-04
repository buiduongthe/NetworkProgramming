// Bài 5.2 – Kiểm tra nhiều cổng đồng thời có giới hạn (Mã giả 5.2; Mục 5.2.5 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- scan [host] [cổng_đầu] [cổng_cuối] [max_parallel]   (mặc định 127.0.0.1 9000 9019 10)
//        dotnet run -- ports <cổng_mở,...> <cổng_lọc,...>                  (mở cổng minh họa trên 127.0.0.1)
using Bai5_2;

if (args.Length == 0 || (args[0] != "scan" && args[0] != "ports"))
{
    Console.WriteLine("Cách dùng: dotnet run -- scan|ports ...");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "scan" ? await ScanAsync.RunAsync(rest) : DemoPorts.Run(rest);
