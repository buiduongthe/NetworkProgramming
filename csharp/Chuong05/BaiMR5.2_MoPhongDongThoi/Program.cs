// Bài mở rộng MR5.2 – So sánh mô hình đồng thời của .NET trên cùng khối việc (mạng, CPU, mạng + CPU):
// tuần tự, Thread, nhóm luồng, ThreadPool/Task.Run, Parallel.For, async/await; biểu đồ Gantt văn bản,
// .NET không có GIL, vòng lặp một luồng (SynchronizationContext), tranh chấp dữ liệu, bế tắc.
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1, 5.2) và Chương 6 (Bài 6.1, 6.2).
//
// Chạy:  dotnet run -- server <host> <port> [trễ_ms]     máy chủ thử (mặc định trễ 200 ms)
//        dotnet run -- mang <host> <port>
//        dotnet run -- cpu
//        dotnet run -- tron <host> <port>
//        dotnet run -- khoa
using System.Text;
using BaiMR5_2;

Console.OutputEncoding = Encoding.UTF8;
switch (args.FirstOrDefault())
{
    case "server" when args.Length >= 3: return MayChuTre.Run(args[1..]);
    case "mang" when args.Length >= 3: DongThoi.RunNet(args[1], int.Parse(args[2])); return 0;
    case "tron" when args.Length >= 3: DongThoi.RunMixed(args[1], int.Parse(args[2])); return 0;
    case "cpu": DongThoi.RunCpu(); return 0;
    case "khoa": Khoa.Run(); return 0;
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server <host> <port> [trễ_ms] | mang <host> <port> | cpu | tron <host> <port> | khoa");
        return 1;
}
