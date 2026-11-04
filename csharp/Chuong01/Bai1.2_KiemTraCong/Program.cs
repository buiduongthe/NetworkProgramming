// Bài 1.2 – Kiểm tra trạng thái một cổng TCP (Mã giả 1.2 – PORT_CHECK, kế thừa Mã giả 1.1; Mục 1.2.5 Giáo trình Lập trình mạng)
// Kết quả: OPEN, CLOSED, FILTERED hoặc UNRESOLVED. Chỉ kiểm tra trên máy và mạng thuộc quyền quản lý của người thực hiện.
// Chạy:  dotnet run -- <host> <port> [timeout_ms]      (mặc định timeout_ms = 2000)
// Ví dụ: dotnet run -- localhost 9000
using Bai1_2;

if (args.Length < 2)
{
    Console.WriteLine("Cách dùng: dotnet run -- <host> <port> [timeout_ms]");
    return 1;
}
string host = args[0];
int port = int.Parse(args[1]);
int timeoutMs = args.Length > 2 ? int.Parse(args[2]) : 2000;
Console.WriteLine($"[port-check] Kiểm tra {host}:{port} (thời gian chờ {timeoutMs} ms)");
string state = PortChecker.PortCheck(host, port, timeoutMs);
Console.WriteLine($"[port-check] Kết quả: {host}:{port} -> {state}");
return 0;
