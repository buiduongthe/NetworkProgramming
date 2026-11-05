// Bài mở rộng MR8.4 – MCP server chống SSRF (stdio, JSON-RPC 2.0). Host khởi chạy như tiến trình con;
// client thử nghiệm mcp_test_client.py (dùng lại từ bản Python) dựng ba server cục bộ rồi gửi kịch bản.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 8.3 (MCP) và MR8.1.
using System.Text;
using BaiMR8_4;

Console.OutputEncoding = Encoding.UTF8;   // nhật ký stderr tiếng Việt
return McpServer.Run();
