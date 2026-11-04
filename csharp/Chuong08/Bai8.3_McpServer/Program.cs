// Bài 8.3 – MCP server công cụ mạng qua kênh stdio (đặc tả MCP 2026-07-28 như Mục 8.3 trình bày)
// Mã giả 8.4 (mcp-server), 8.5 (mcp-tools), 8.7 (validate); Mục 8.3.5, 8.5.4 Giáo trình Lập trình mạng
// JSON-RPC 2.0, mỗi thông điệp một dòng UTF-8 trên stdin/stdout; nhật ký chỉ ghi ra stderr.
// Biến môi trường ALLOWED_NETS: các mạng được phép (CIDR, phân cách dấu phẩy), ví dụ 10.10.0.0/16.
// Chạy (do host khởi chạy như tiến trình con):  dotnet run --no-build -c Release
// Thử nhanh:  python3 mcp_test_client.py 9000 9001 dotnet run --no-build -c Release
using System.Globalization;
using Bai8_3;

CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
Console.OutputEncoding = System.Text.Encoding.UTF8;     // stderr (nhật ký) dùng UTF-8
McpServer.Run();
