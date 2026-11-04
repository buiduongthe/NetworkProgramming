// Bài 6.6 – Phát và nhận thông báo dịch vụ qua multicast (Mã giả 6.8: ANNOUNCER, LISTENER; Mục 6.4.3)
// Chạy:  dotnet run -- announcer [service_port] [tên] [period_ms] [count] [mport]  (mặc định 9200 server-chat 5000 0 9998)
//        dotnet run -- listener [run_ms] [mport]                                   (mặc định 0 9998)
using Bai6_6;

string mode = args.Length > 0 ? args[0] : "";
var a = args.Skip(1).ToArray();
int Arg(int i, int def) => a.Length > i ? int.Parse(a[i]) : def;
switch (mode)
{
    case "announcer":
        Multicast.Announcer(Arg(0, 9200), a.Length > 1 ? a[1] : "server-chat", Arg(2, 5000), Arg(3, 0), Arg(4, Multicast.MPort));
        return 0;
    case "listener":
        var cache = new Dictionary<string, (string Ip, int Port, long Expires)>();
        Multicast.Listener(cache, Arg(0, 0), Arg(1, Multicast.MPort));
        string items = cache.Count == 0 ? "(trống)" : string.Join(", ", cache.Select(kv => $"{kv.Key} tại {kv.Value.Ip}:{kv.Value.Port}"));
        Console.WriteLine($"[listener] Bộ nhớ đệm: {items}");
        return 0;
    default:
        Console.WriteLine("Cách dùng: dotnet run -- announcer|listener [...]");
        return 1;
}
