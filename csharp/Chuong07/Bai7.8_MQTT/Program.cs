// Bài 7.8 – Thiết bị cảm biến và bảng theo dõi dùng MQTT 5.0 (Mã giả 7.9; Mục 7.5.5 Giáo trình Lập trình mạng)
// Chạy:  dotnet run -- sensor <mã_thiết_bị> [host] [port] [số_lần] [chu_kỳ_ms] [--mat-ket-noi]
//        dotnet run -- dashboard [host] [port] [keep_alive_s] [client_id]
//        (mặc định localhost 1883; sensor: số_lần 0 = chạy mãi, chu kỳ 5000 ms; dashboard: 60 s, dashboard-01)
using Bai7_8;

string mode = args.Length > 0 ? args[0] : "";
var rest = args.Skip(1).ToArray();
return mode switch
{
    "sensor" => Sensor.Run(rest),
    "dashboard" => Dashboard.Run(rest),
    _ => Usage(),
};

static int Usage()
{
    Console.WriteLine("Cách dùng: dotnet run -- sensor|dashboard ...");
    return 1;
}
