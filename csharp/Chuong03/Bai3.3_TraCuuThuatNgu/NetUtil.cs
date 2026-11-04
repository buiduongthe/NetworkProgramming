using System.Net;
using System.Net.Sockets;

namespace Bai3_3;

// Lỗi mạng đã phân loại (Mã giả 2.2): Kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}, Retry = có nên thử lại
class NetError(string kind, bool retry, Exception? cause)
    : IOException($"{kind} (thử lại: {(retry ? "có" : "không")}): {cause?.Message}", cause)
{
    public string Kind { get; } = kind;
    public bool Retry { get; } = retry;
}

static class NetUtil
{
    // Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định
    public static (string Host, int Port) ParseEndpoint(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p))                                   // TO_INT
            throw new ArgumentException($"Cổng phải là số nguyên: {port}");   // ConfigError
        if (p < 1 || p > 65535)
            throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }

    // Mã giả 2.2 – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối, phân loại lỗi
    public static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs;
        try { addrs = Dns.GetHostAddresses(host); }                           // RESOLVE
        catch (SocketException e) { throw new NetError("NOT_FOUND", false, e); }
        int refused = 0;
        Exception? last = null;
        foreach (var ip in addrs)
        {
            var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                using var cts = new CancellationTokenSource(timeoutMs);        // CONNECT_TIMEOUT
                s.ConnectAsync(ip, port, cts.Token).AsTask().GetAwaiter().GetResult();
                return s;
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused)
            {
                refused++; last = e; s.Dispose();                             // nhận RST
            }
            catch (Exception e) when (e is SocketException || e is OperationCanceledException)
            {
                last = e; s.Dispose();                                        // Timeout, Unreachable
            }
        }
        if (refused > 0 && refused == addrs.Length) throw new NetError("REFUSED", false, last);
        throw new NetError("UNREACHABLE", true, last);
    }
}
