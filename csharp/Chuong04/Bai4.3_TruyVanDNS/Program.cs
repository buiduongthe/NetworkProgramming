// Bài 4.3 – Tự tạo và phân tích truy vấn DNS qua UDP (Mã giả 4.5; Mục 4.3.2 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- <ip_dns_server> <ten_mien> [A|AAAA] [cong]      (mặc định A, cổng 53)
// Ví dụ: dotnet run -- 8.8.8.8 example.com AAAA
using Bai4_3;

return DnsQuery.Run(args);
