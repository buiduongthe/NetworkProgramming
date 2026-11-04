// Bài 6.5 – Khám phá server trong LAN bằng broadcast (Mã giả 6.7: DISCOVER, RESPONDER; Mục 6.4.2)
// Chạy:  dotnet run -- responder [service_port] [tên] [disc_port]       (mặc định 9200 server-chat 9999)
//        dotnet run -- discover [wait_ms] [disc_port] [địa_chỉ_broadcast] (mặc định 2000 9999 255.255.255.255)
using Bai6_5;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
switch (mode)
{
    case "responder":
        return await Responder.RunAsync(rest.Length > 0 ? int.Parse(rest[0]) : 9200,
                                        rest.Length > 1 ? rest[1] : "server-chat",
                                        rest.Length > 2 ? int.Parse(rest[2]) : Discovery.DiscPort);
    case "discover":
        int waitMs = rest.Length > 0 ? int.Parse(rest[0]) : 2000;
        int port = rest.Length > 1 ? int.Parse(rest[1]) : Discovery.DiscPort;
        string bcast = rest.Length > 2 ? rest[2] : "255.255.255.255";
        var found = Discovery.Discover(waitMs, port, bcast);
        Console.WriteLine($"[client] Tìm thấy {found.Count} server:");
        foreach (var (ip, p, name) in found) Console.WriteLine($"[client]   {name} tại {ip}:{p}");
        return 0;
    default:
        Console.WriteLine("Cách dùng: dotnet run -- responder|discover [...]");
        return 1;
}
