// Bài mở rộng MR6.11 – Flash sale: kho 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm "MUA NGAY".
// Máy chủ TCP đa luồng (một Thread mỗi kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên
// bản + CAS, hàng đợi một luồng), sổ kiểm toán đếm bán vượt tồn kho và sổ lệch kho.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng), Bài 6.7 (kiểm thử tải).
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ton 100] [--mo-sau 30]
//        dotnet run -- mo_phong [host] [cổng] [--n 10000] [--k 200] [--lan 1] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- flash_sale <tên> [host] [cổng] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_11;

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
    case "flash_sale": return NguoiMua.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | flash_sale <tên> [host] [cổng]");
        return 2;
}
