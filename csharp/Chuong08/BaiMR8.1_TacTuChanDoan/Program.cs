// Bài mở rộng MR8.1 – Tác tử AI chẩn đoán mạng bằng gọi công cụ, bốn lớp kiểm soát (LLM giả lập cục bộ).
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.1 (gọi API LLM) và Bài 8.3 (MCP server).
//
// Chạy:  dotnet run -- "<câu hỏi>"      (biến môi trường LLM_BASE_URL, LLM_API_KEY, LLM_MODEL)
using System.Text;
using BaiMR8_1;

Console.OutputEncoding = Encoding.UTF8;
return TacTu.Run(args);
