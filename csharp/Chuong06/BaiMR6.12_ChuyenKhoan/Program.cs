// Bài mở rộng MR6.12 – Chuyển khoản: 5–20 tài khoản, nhiều người chuyển chéo cùng lúc (A→B và B→A). Máy chủ TCP đa
// luồng (một Thread mỗi kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ hai tài khoản theo thứ tự mã –
// hoặc "ngây thơ" nguồn rồi đích để thấy khóa chết, lạc quan phiên bản + CAS, hàng đợi một luồng), sổ kiểm toán đếm
// tổng tiền lệch, tài khoản lệch sổ và thấu chi.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng), Bài 6.7 (kiểm thử tải).
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-tk 10] [--so-du 5000000] [--khoa-ngay-tho]
//        dotnet run -- mo_phong [host] [cổng] [--n 500] [--k 200] [--lan 5] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_12;

Console.OutputEncoding = Encoding.UTF8;
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số in dạng 1.5 (không phải 1,5) mọi máy
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
WinMM.TangDoChinhXacHenGio();                // Windows: Thread.Sleep chính xác 1 ms (Linux: bỏ qua)
var rest = args.Skip(1).ToArray();
switch (args.FirstOrDefault())
{
    case "server": return Server.Run(rest);
    case "mo_phong": return MoPhong.Run(rest);
    case "khach": return QuanTri.Run(rest);
    case "ngan_hang": return KhachHang.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | ngan_hang <tên> [host] [cổng] [--tk TK01]");
        return 2;
}
