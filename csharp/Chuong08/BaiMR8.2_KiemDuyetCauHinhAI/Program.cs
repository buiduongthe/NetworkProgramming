// Bài mở rộng MR8.2 – Kiểm duyệt cấu hình do LLM sinh: JSON, lược đồ, ngữ nghĩa, chính sách, vòng phản hồi
// (LLM giả lập cục bộ). Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1, Bài 3.3, MR1.1, MR3.2.
//
// Chạy:  dotnet run -- <ha_tang.json> "<yêu cầu>" [--so-lan N]     (biến môi trường LLM_BASE_URL, LLM_API_KEY, LLM_MODEL)
using System.Text;
using BaiMR8_2;

Console.OutputEncoding = Encoding.UTF8;
return KiemDuyet.Run(args);
