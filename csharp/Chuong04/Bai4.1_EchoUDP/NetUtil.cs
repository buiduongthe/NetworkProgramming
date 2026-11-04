namespace Bai4_1;

// Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định
static class Endpoint
{
    public static (string Host, int Port) Parse(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
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
