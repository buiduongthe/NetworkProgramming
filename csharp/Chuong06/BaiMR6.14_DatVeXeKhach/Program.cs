// Bài mở rộng MR6.14 – Đặt vé xe khách theo chặng: xe giường nằm 40 chỗ hoặc xe ghế ngồi 50 chỗ chạy Sài Gòn – Bảo Lộc
// – Đà Lạt – Nha Trang; tài nguyên là cặp (chỗ, chặng) nên một chỗ bán được cho nhiều người đi các đoạn không chồng
// nhau; giữ chỗ có thời hạn (TTL) rồi thanh toán. Máy chủ TCP đa luồng (một Thread mỗi kết nối), 4 cách xử lý tranh
// chấp (không khóa, khóa loại trừ theo từng ô, lạc quan phiên bản + CAS, hàng đợi một luồng), sổ kiểm toán đếm chỗ
// trùng và thanh toán sai. Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1, Bài 6.7 và MR6.8.
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ttl 30] [--loai-xe giuong] [--ghe-nong D01]
//        dotnet run -- mo_phong [host] [cổng] [--n 200] [--k 200] [--lan 1] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_14;

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
    case "dat_ve_xe": return NguoiDatVeXe.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | dat_ve_xe <tên> [host] [cổng]");
        return 2;
}
