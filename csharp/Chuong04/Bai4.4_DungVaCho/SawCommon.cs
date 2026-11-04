using System.Security.Cryptography;

namespace Bai4_4;

// Hằng số dùng chung của Mã giả 4.6, 4.7
static class Saw
{
    public const byte DATA = 0, ACK = 1, FIN = 2;           // byte loại trong tiêu đề 5 byte
    public const int MAX_TRIES = 8, MAX_DGRAM = 1200, CHUNK = MAX_DGRAM - 5;   // khối ≤ 1195 byte
    public const int RTO_INIT = 200, RTO_MIN = 100;                            // ms, cho mạng nội bộ (Mục 4.4.2)

    // Mã giả 4.6 dòng 1: MAX_RTO ← 60000 ms (trần của RTO, RFC 6298). Biến môi trường SAW_MAX_RTO (ms) ghi đè
    // giá trị này CHỈ để chạy demo nhanh; không đặt biến thì bên nhận chờ 2 × 60000 ms = 120 s sau FIN.
    static readonly string? MaxRtoEnv = Environment.GetEnvironmentVariable("SAW_MAX_RTO");
    public static readonly int MAX_RTO = string.IsNullOrEmpty(MaxRtoEnv) ? 60000 : int.Parse(MaxRtoEnv);
    public static string MaxRtoNote() => string.IsNullOrEmpty(MaxRtoEnv)
        ? $"MAX_RTO = {MAX_RTO} ms"
        : $"MAX_RTO = {MAX_RTO} ms (ghi đè cho demo bằng biến môi trường SAW_MAX_RTO; mặc định 60000 ms)";

    public static string Sha256File(string path) => Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(path)));
}

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

// Mô phỏng mất gói ở chiều gửi đi (thêm cho bài thực hành, không có trong mã giả):
// "" không mất; "0.1" mất ngẫu nhiên 10%; "3,7" mất datagram gửi đi thứ 3 và 7 (đếm từ 1, kể cả gửi lại)
class LossSim
{
    public string Spec { get; }
    readonly double rate;
    readonly HashSet<int> picks = new();
    int count;

    public LossSim(string? spec)
    {
        Spec = spec ?? "";
        if (Spec.Contains('.')) rate = double.Parse(Spec, System.Globalization.CultureInfo.InvariantCulture);
        else foreach (var x in Spec.Split(',', StringSplitOptions.RemoveEmptyEntries)) picks.Add(int.Parse(x));
    }

    public bool Drop() => picks.Contains(++count) | Random.Shared.NextDouble() < rate;
}
