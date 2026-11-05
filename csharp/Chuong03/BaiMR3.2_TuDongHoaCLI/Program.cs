// Bài mở rộng MR3.2 – Tự động hóa cấu hình thiết bị qua CLI: đọc tới dấu nhắc, so sánh khác biệt,
// lũy đẳng, chạy thử. Bài mở rộng, không có trong giáo trình; vận dụng Chương 3 (định khung bằng dấu
// phân cách), Chương 6 (server nhiều phiên), Chương 2 (đọc cấu hình, tài khoản từ biến môi trường).
//
// Chạy:  dotnet run -- thiet_bi [host] [port]                          thiết bị mô phỏng R1
//        dotnet run -- tu_dong <host> <port> <muc_tieu.json> [--thu]   client tự động hóa
using System.Text;
using BaiMR3_2;

Console.OutputEncoding = Encoding.UTF8;
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "thiet_bi": return ThietBiGia.Run(rest);
    case "tu_dong": return TuDongHoa.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- thiet_bi [host] [port]");
        Console.WriteLine("           dotnet run -- tu_dong <host> <port> <muc_tieu.json> [--thu]");
        return 2;
}
