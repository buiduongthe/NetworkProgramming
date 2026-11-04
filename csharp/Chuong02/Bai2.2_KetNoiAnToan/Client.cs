using System.Diagnostics;
using System.Net.Sockets;

namespace Bai2_2;

// Client minh họa Mã giả 2.2: gọi SAFE_CONNECT và in kết quả hoặc loại lỗi
static class Client
{
    public static int Run(string[] args)
    {
        string host; int port; int timeoutMs;
        try
        {
            (host, port) = Endpoint.Parse(args, "127.0.0.1", 9000);
            timeoutMs = args.Length >= 3 ? int.Parse(args[2]) : 3000;
        }
        catch (Exception e) when (e is ConfigError || e is FormatException)
        {
            Console.WriteLine($"[client] Lỗi cấu hình: {e.Message}");
            return 2;
        }
        Console.WriteLine($"[client] Kết nối tới {host}:{port}, thời gian chờ {timeoutMs} ms");
        var sw = Stopwatch.StartNew();
        Socket s;
        try
        {
            s = Net.SafeConnect(host, port, timeoutMs);
        }
        catch (NetError e)
        {
            string retry = e.Retry ? "có" : "không";
            Console.WriteLine($"[client] NetError {e.Kind}, thử lại: {retry}, sau {sw.ElapsedMilliseconds} ms");
            Console.WriteLine($"[client] Nguyên nhân: {Net.Describe(e.InnerException)}");
            return 1;
        }
        long ms = sw.ElapsedMilliseconds;
        using (s)                                                         // nơi gọi chịu trách nhiệm đóng socket
        {
            Console.WriteLine($"[client] Thành công: {Fmt(s.LocalEndPoint)} -> {Fmt(s.RemoteEndPoint)}, sau {ms} ms");
        }
        Console.WriteLine("[client] Đã đóng kết nối");
        return 0;
    }

    // In địa chỉ IPv4 dạng a.b.c.d:port (bỏ tiền tố ::ffff: nếu có)
    static string Fmt(System.Net.EndPoint? ep) =>
        ep is System.Net.IPEndPoint ip
            ? $"{(ip.Address.IsIPv4MappedToIPv6 ? ip.Address.MapToIPv4() : ip.Address)}:{ip.Port}"
            : $"{ep}";
}
