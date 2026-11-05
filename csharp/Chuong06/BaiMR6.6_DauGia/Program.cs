// Bài mở rộng MR6.6 – Đấu giá trực tuyến: n người cùng tranh một món hàng. Máy chủ TCP đa luồng (một Thread mỗi
// kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS, hàng đợi một luồng), sổ
// kiểm toán, đồng hồ phiên gia hạn chống bắn tỉa, bộ mô phỏng tải có rào xuất phát và pipelining.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng), Bài 6.7 (kiểm thử tải).
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--thoi-gian 0] [--gia 5000000] [--buoc 100000]
//                             [--gia-han 10]
//        dotnet run -- mo_phong [host] [cổng] [--n 200] [--k 200] [--lan 5] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- dau_gia <tên> [host] [cổng] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_6;

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
    case "dau_gia": return NguoiThamGia.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | dau_gia <tên> [host] [cổng]");
        return 2;
}
