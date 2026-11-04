namespace Bai7_8;

// Mã giả 7.9 – PROCEDURE SENSOR: di chúc "offline" giữ lại, "online" QoS 1 giữ lại, số đo JSON {v, ts} QoS 0
static class Sensor
{
    public static int Run(string[] all)
    {
        bool lose = all.Contains("--mat-ket-noi");
        string[] a = all.Where(x => x != "--mat-ket-noi").ToArray();
        string deviceId = a.Length >= 1 ? a[0] : "day1";
        string host = a.Length >= 2 ? a[1] : "localhost";
        int port = a.Length >= 3 ? int.Parse(a[2]) : 1883;
        int times = a.Length >= 4 ? int.Parse(a[3]) : 0;
        int periodMs = a.Length >= 5 ? int.Parse(a[4]) : 5000;
        string st = $"nhamay/{deviceId}/trangthai";
        var c = new MqttClient();
        c.Connect(host, port, deviceId, keepAliveS: 30, will: (st, "offline", 1, true));
        Console.WriteLine($"[{deviceId}] Đã kết nối broker {host}:{port} (MQTT 5.0, keep alive 30 s, di chúc '{st}' = offline)");
        c.PublishMsg(st, "online", qos: 1, retain: true);
        Console.WriteLine($"[{deviceId}] PUBLISH {st} = online (QoS 1, giữ lại) -> đã nhận PUBACK");
        for (int n = 1; times == 0 || n <= times; n++)
        {
            double v = Math.Round(24.0 + Random.Shared.NextDouble() * 8.0, 1);   // READ_SENSOR(): giá trị mô phỏng
            string msg = $"{{\"v\":{v.ToString("0.0", System.Globalization.CultureInfo.InvariantCulture)},\"ts\":{DateTimeOffset.UtcNow.ToUnixTimeSeconds()}}}";
            c.PublishMsg($"nhamay/{deviceId}/nhietdo", msg, qos: 0);
            Console.WriteLine($"[{deviceId}] PUBLISH nhamay/{deviceId}/nhietdo = {msg} (QoS 0)");
            Thread.Sleep(periodMs);
        }
        if (lose)
        {
            c.CloseAbruptly();                                          // mô phỏng mất điện
            Console.WriteLine($"[{deviceId}] Mô phỏng mất kết nối: đóng socket, không gửi DISCONNECT");
        }
        else
        {
            c.PublishMsg(st, "offline", qos: 1, retain: true);
            c.Disconnect();
            Console.WriteLine($"[{deviceId}] Tắt đúng thủ tục: phát offline rồi gửi DISCONNECT");
        }
        return 0;
    }
}
