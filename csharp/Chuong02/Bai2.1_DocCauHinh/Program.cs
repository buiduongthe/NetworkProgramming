// Bài 2.1 – Đọc địa chỉ và cổng từ tham số dòng lệnh, biến môi trường
// (Mã giả 2.1 PARSE_ENDPOINT; Mục 2.1.3 Giáo trình Lập trình mạng). Chương trình chỉ in cấu hình, không mở socket.
// Chạy:  dotnet run -- [host] [port]                      (mặc định 0.0.0.0 9000)
// Ví dụ: APP_PORT=9100 dotnet run                         (Linux/macOS)
//        $env:APP_PORT="9100"; dotnet run                 (Windows PowerShell)
using Bai2_1;

string envHost = Environment.GetEnvironmentVariable("APP_HOST") ?? "(không đặt)";
string envPort = Environment.GetEnvironmentVariable("APP_PORT") ?? "(không đặt)";
string argText = args.Length > 0 ? string.Join(" ", args) : "(không có)";
// In dữ liệu vào để thấy rõ giá trị nào thắng
Console.WriteLine($"[cấu hình] APP_HOST={envHost}, APP_PORT={envPort}, tham số: {argText}");
try
{
    var (host, port) = Endpoint.Parse(args, "0.0.0.0", 9000);
    Console.WriteLine($"[cấu hình] Kết quả: host={host}, port={port}");
    return 0;
}
catch (ConfigError e)
{
    Console.WriteLine($"[cấu hình] ConfigError: {e.Message}");
    return 2;
}
