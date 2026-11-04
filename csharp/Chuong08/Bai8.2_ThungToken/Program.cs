// Bài 8.2 – Thùng token giới hạn tần suất (Mã giả 8.3; Mục 8.2.4 Giáo trình Lập trình mạng)
// Nhiều tác vụ đồng thời gửi một chuỗi yêu cầu qua cùng một thùng; chương trình in thời điểm mỗi yêu cầu
// được phép và so sánh với lý thuyết: C yêu cầu đầu đi ngay, yêu cầu thứ m > C được phép sau (m − C)/R giây.
// Chạy:  dotnet run -- [C] [R] [số_yêu_cầu] [số_tác_vụ]      (mặc định 10 2 14 2)
using System.Diagnostics;
using System.Globalization;
using Bai8_2;

CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
Console.OutputEncoding = System.Text.Encoding.UTF8;
double cap = args.Length > 0 ? double.Parse(args[0]) : 10;
double rate = args.Length > 1 ? double.Parse(args[1]) : 2;
int total = args.Length > 2 ? int.Parse(args[2]) : 14;
int tasks = args.Length > 3 ? int.Parse(args[3]) : 2;
var b = new Bucket(cap, rate);
Console.WriteLine($"[bucket] Thùng: dung lượng C = {cap:G}, tốc độ nạp R = {rate:G} đơn vị/giây, mỗi yêu cầu n = 1");
Console.WriteLine($"[bucket] Gửi {total} yêu cầu từ {tasks} tác vụ đồng thời (thùng đầy lúc bắt đầu)");
var clock = Stopwatch.StartNew();
var granted = new List<(double T, int Worker)>();

// Một tác vụ gửi `count` yêu cầu, mỗi yêu cầu cần 1 đơn vị
async Task Worker(int wid, int count)
{
    for (int i = 0; i < count; i++)
    {
        await b.AcquireTokens(1);
        lock (granted) granted.Add((clock.Elapsed.TotalSeconds, wid));   // ghi lại thời điểm được phép
        await Task.Yield();                                                // nhường lượt: giả lập gửi yêu cầu
    }
}

var all = Enumerable.Range(0, tasks).Select(w => Worker(w + 1, total / tasks + (w < total % tasks ? 1 : 0)));
await Task.WhenAll(all);
int m = 0;
foreach (var (t, wid) in granted.OrderBy(g => g.T))
{
    m++;
    double theory = Math.Max(0, (m - cap) / rate);
    Console.WriteLine($"[bucket] Yêu cầu {m:D2} (tác vụ {wid}) được phép tại t = {t,6:F3} s   lý thuyết {theory,6:F3} s");
}
try
{
    Console.WriteLine($"[bucket] Thử lấy n = {cap + 1:G} > C = {cap:G} đơn vị");
    await b.AcquireTokens(cap + 1);
}
catch (ArgumentException e)                                              // dòng 3: báo lỗi ngay, không chờ mãi
{
    Console.WriteLine($"[bucket] Lỗi ArgumentError: {e.Message}");
}
