// Bài 7.6 – Phiên gửi thư SMTP có STARTTLS và AUTH PLAIN (Mã giả 7.7; Mục 7.5.1 Giáo trình Lập trình mạng)
// Tài khoản lấy từ biến môi trường SMTP_USER, SMTP_PASS. Server thử: sh chay_server_thu.sh [host] [port]
// Chạy:  dotnet run -- [host] [port] [tệp_CA]   (mặc định localhost 587 cert.pem; "-" = kho chứng chỉ hệ thống)
using Bai7_6;

string host = args.Length >= 1 ? args[0] : "localhost";
int port = args.Length >= 2 ? int.Parse(args[1]) : 587;
string caFile = args.Length >= 3 ? args[2] : "cert.pem";
string user = Environment.GetEnvironmentVariable("SMTP_USER") ?? "";
string secret = Environment.GetEnvironmentVariable("SMTP_PASS") ?? "";
const string from = "sinhvien@example.com", to = "giangvien@example.com";
try
{
    SmtpSession.SendMail(host, port, caFile, user, secret, from, to, SmtpSession.BuildMessage(from, to));
    Console.WriteLine("[client] Gửi thư thành công");
    return 0;
}
catch (SmtpException e)
{
    Console.WriteLine($"[client] Lỗi SMTP: {e.Message}");
    return 1;
}
catch (InvalidDataException e)                                       // ProtocolError: phản hồi sai cú pháp
{
    Console.WriteLine($"[client] Lỗi giao thức: {e.Message}");
    return 1;
}
catch (System.Security.Authentication.AuthenticationException e)    // chứng chỉ không tin cậy hoặc sai tên máy
{
    Console.WriteLine($"[client] Lỗi TLS: {e.Message}");
    return 1;
}
