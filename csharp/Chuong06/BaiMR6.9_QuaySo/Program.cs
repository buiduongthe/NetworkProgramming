// Bài mở rộng MR6.9 – Quay số trúng thưởng: 10 món quà, hàng chục nghìn lượt quay, mỗi người đúng một lượt. Máy chủ
// TCP đa luồng (một Thread mỗi kết nối), 4 cách xử lý tranh chấp (không khóa, khóa loại trừ theo từng ô, lạc quan
// phiên bản + CAS, hàng đợi một luồng), sổ kiểm toán đếm quà trao thừa và người quay hai lần.
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng), Bài 6.7 (kiểm thử tải).
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]
//        dotnet run -- mo_phong [host] [cổng] [--n 100000] [--k 200] [--lan 1] [--che-do x] [--dat-lai] [--so-sanh] [--tre-ms 2]
//        dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
//        dotnet run -- quay_so <tên> [host] [cổng] [--thoi-gian 0]
using System.Globalization;
using System.Text;
using BaiMR6_9;

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
    case "quay_so": return NguoiQuay.Run(rest);
    default:
        Console.WriteLine("Cách dùng: dotnet run -- server [host] [cổng] [tùy chọn] | mo_phong [host] [cổng] [tùy chọn]"
                          + " | khach <host> <cổng> <lệnh> | quay_so <tên> [host] [cổng]");
        return 2;
}
