namespace Bai2_1;

// Lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được
class ConfigError(string message) : Exception(message);

// Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định
static class Endpoint
{
    public static (string Host, int Port) Parse(string[] args, string defaultHost, int defaultPort)
    {
        string host = defaultHost;                                        // dòng 2
        string port = defaultPort.ToString();                             // dòng 3
        string? envHost = Environment.GetEnvironmentVariable("APP_HOST");
        string? envPort = Environment.GetEnvironmentVariable("APP_PORT");
        if (envHost != null) host = envHost;                              // dòng 4: biến môi trường ghi đè mặc định
        if (envPort != null) port = envPort;                              // dòng 5
        if (args.Length >= 1) host = args[0];                             // dòng 6: tham số dòng lệnh ghi đè tất cả
        if (args.Length >= 2) port = args[1];                             // dòng 7
        if (!int.TryParse(port, out int p))                               // dòng 8–12: TO_INT, "abc" → lỗi
            throw new ConfigError($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535)                                           // dòng 13–15: cổng 16 bit, khác 0
            throw new ConfigError($"Cổng không hợp lệ: {p}");
        return (host, p);                                                 // dòng 16
    }
}
