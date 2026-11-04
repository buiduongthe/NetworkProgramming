// Bài 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành
// (Mã giả 2.3, kế thừa Mã giả 1.1 RESOLVE_HOST; Mục 2.5 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- <tên miền | địa chỉ IP> ...
// Ví dụ: dotnet run -- localhost 127.0.0.1 8.8.8.8
using Bai2_3;

return DnsClient.Run(args);
