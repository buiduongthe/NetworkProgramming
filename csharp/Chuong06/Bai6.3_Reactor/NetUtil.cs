using System.Net;
using System.Net.Sockets;

namespace Bai6_3;

// Các thủ tục kế thừa dùng chung của Bài 6.3 (chép gọn từ Chương 2, 3)

/// <summary>Đối tác đóng kết nối; Received = số byte đã nhận.</summary>
class ConnectionClosedException(int received) : IOException($"Kết nối bị đóng sau {received} byte")
{
    public int Received { get; } = received;
}

static class NetUtil
{
    // Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định
    public static (string Host, int Port) ParseEndpoint(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p)) throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535) throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }

    /// <summary>Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.</summary>
    public static int IntArg(string[] args, int i, int def) => args.Length > i ? int.Parse(args[i]) : def;

    // Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối
    public static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        Exception? last = null;
        foreach (var ip in Dns.GetHostAddresses(host))
        {
            var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                using var cts = new CancellationTokenSource(timeoutMs);
                s.ConnectAsync(ip, port, cts.Token).AsTask().GetAwaiter().GetResult();
                return s;
            }
            catch (Exception e) when (e is SocketException || e is OperationCanceledException)
            {
                last = e;
                s.Dispose();
            }
        }
        throw new IOException($"Không kết nối được {host}:{port}: {last?.Message}");
    }

    // Mã giả 3.4 – RECV_EXACT: gọi Receive lặp lại đến khi đủ n byte
    public static byte[] RecvExact(Socket s, int n)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = s.Receive(buf, got, n - got, SocketFlags.None);
            if (k == 0) throw new ConnectionClosedException(got);
            got += k;
        }
        return buf;
    }
}
