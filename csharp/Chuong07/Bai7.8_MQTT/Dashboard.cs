namespace Bai7_8;

// Mã giả 7.9 – PROCEDURE DASHBOARD: đăng ký bộ lọc có ký tự đại diện rồi chạy vòng sự kiện
static class Dashboard
{
    public static int Run(string[] a)
    {
        string host = a.Length >= 1 ? a[0] : "localhost";
        int port = a.Length >= 2 ? int.Parse(a[1]) : 1883;
        int keepAlive = a.Length >= 3 ? int.Parse(a[2]) : 60;
        string clientId = a.Length >= 4 ? a[3] : "dashboard-01";      // hai client trùng id: broker ngắt client cũ
        var c = new MqttClient();
        c.Connect(host, port, clientId, keepAlive);
        Console.WriteLine($"[dashboard] Đã kết nối broker {host}:{port} là '{clientId}' (MQTT 5.0, keep alive {keepAlive} s)");
        Console.CancelKeyPress += (_, e) =>                             // Ctrl+C: ngắt kết nối đúng thủ tục
        {
            e.Cancel = true;
            c.Disconnect();
            Console.WriteLine("[dashboard] Đã gửi DISCONNECT");
            Environment.Exit(0);
        };
        int g1 = c.SubscribeTo("nhamay/+/nhietdo", 0, OnTemp);
        Console.WriteLine($"[dashboard] SUBSCRIBE nhamay/+/nhietdo (QoS 0) -> SUBACK, QoS được cấp {g1}");
        int g2 = c.SubscribeTo("nhamay/+/trangthai", 1, OnStatus);
        Console.WriteLine($"[dashboard] SUBSCRIBE nhamay/+/trangthai (QoS 1) -> SUBACK, QoS được cấp {g2}");
        try { c.RunEventLoop(() => Console.WriteLine("[dashboard] PINGREQ -> PINGRESP")); }
        catch (IOException e) { Console.WriteLine($"[dashboard] {e.Message}"); }
        return 0;
    }

    static void OnTemp(string topic, string payload, int qos, bool retain) =>
        Console.WriteLine($"[dashboard] Nhiệt độ  {topic}: {payload}");

    static void OnStatus(string topic, string payload, int qos, bool retain) =>
        Console.WriteLine($"[dashboard] Trạng thái {topic}: {payload} (QoS {qos}{(retain ? ", tin giữ lại" : "")})");
}
