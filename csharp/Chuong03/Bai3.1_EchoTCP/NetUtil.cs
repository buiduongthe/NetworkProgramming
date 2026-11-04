using System.Net.Sockets;

namespace Bai3_1;

// Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định
static class Endpoint
{
    public static (string Host, int Port) Parse(string[] args, string defaultHost, int defaultPort)
    {
        string host = defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p))
            throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535)
            throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }
}

// Mã giả 3.3 – SEND_ALL: Send() có thể chỉ gửi được một phần nên phải lặp đến khi hết
static class NetUtil
{
    public static void SendAll(Socket s, byte[] data, int count)
    {
        int sent = 0;
        while (sent < count)
            sent += s.Send(data, sent, count - sent, SocketFlags.None);
    }
}
