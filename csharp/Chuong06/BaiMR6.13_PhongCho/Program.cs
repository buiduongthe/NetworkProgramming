// Bài mở rộng MR6.13 – Bán vé có phòng chờ ảo: 500 vé, rất nhiều người cùng mua; máy chủ cấp lượt mua theo thùng
// token (token bucket), lượt có hạn; tắt phòng chờ để thấy quá tải. Máy chủ TCP đa luồng (một Thread mỗi kết nối),
// 4 cách xử lý tranh chấp (không khóa, khóa loại trừ, lạc quan phiên bản + CAS, hàng đợi một luồng), sổ kiểm toán
// đếm vé bán vượt, mua khi không có lượt và đảo thứ tự hàng chờ.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng), Bài 6.7 (kiểm thử tải).
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50] [--tat-phong-cho] …
//        dotnet run -- mo_phong [host] [cổng] [--n 1000] [--k 200] [--lan 1] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- so_sanh_pc [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]
//        dotnet run -- mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_13;

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
    case "so_sanh_pc": return MoPhong.RunSoSanhPhongCho(rest);
    case "mua_ve": return NguoiMuaVe.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | so_sanh_pc [host] [cổng] | mua_ve <tên> [host] [cổng]");
        return 2;
}
