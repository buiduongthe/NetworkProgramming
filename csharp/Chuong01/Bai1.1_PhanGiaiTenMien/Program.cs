// Bài 1.1 – Phân giải tên miền thành danh sách địa chỉ (Mã giả 1.1 – RESOLVE_HOST; Mục 1.2.3 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- [tên ...]      (mặc định: localhost)
// Ví dụ: dotnet run -- localhost example.com
using Bai1_1;

string[] names = args.Length > 0 ? args : ["localhost"];
foreach (var name in names)
{
    Console.WriteLine($"[resolve] Phân giải tên: {name}");
    var addrs = Resolver.ResolveHost(name);
    Console.WriteLine($"[resolve] {name} -> {addrs.Length} địa chỉ");
}
return 0;
