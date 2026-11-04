// Bài 5.4 – Lớp dịch vụ ChatClient bất đồng bộ (Mã giả 5.5; Mục 5.3.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- server [host] [port]          (server chat tối giản, mặc định 0.0.0.0 9200)
//        dotnet run -- cli <tên> [host] [port]       (giao diện dòng lệnh, mặc định 127.0.0.1 9200)
using Bai5_4;

if (args.Length == 0 || (args[0] != "server" && args[0] != "cli"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|cli ...");
    return 1;
}
var rest = args.Skip(1).ToArray();
return args[0] == "server" ? await ChatServer.RunAsync(rest) : await ChatCli.RunAsync(rest);
