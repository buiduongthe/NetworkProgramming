// Bài mở rộng MR8.3 – Phòng thủ DoS mức ứng dụng kiểu nhỏ-giọt-tiêu-đề: server vulnerable/protected,
// client thí nghiệm (chỉ loopback), probe. Bài mở rộng, không có trong giáo trình; nối tiếp Chương 6, Chương 8.
//
// Chạy:  dotnet run -- server vulnerable|protected <host> <port>
//        dotnet run -- slow <host> <port> [N]      (chỉ 127.0.0.1)
//        dotnet run -- probe <host> <port>
using System.Text;
using BaiMR8_3;

Console.OutputEncoding = Encoding.UTF8;
return ChongSlowloris.Run(args);
