using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace Bai8_3;

// Đầu vào bị chính sách từ chối: trả về mô hình với isError = true
class PolicyError(string message) : Exception(message);

// Không phân giải được tên máy (VALIDATE_TARGET): cũng trả về với isError = true
class ResolveError(string message) : Exception(message);

// Mã giả 8.7 – Kiểm soát đích của công cụ mạng (Mục 8.5.4): mọi đích phải nằm trong ALLOWED_NETS
static class Validate
{
    // PARSE_CIDRS(ENV("ALLOWED_NETS")): CIDR phân cách dấu phẩy; biến vắng -> danh sách rỗng (chặn mọi đích)
    public static readonly List<IPNetwork> AllowedNets = ParseCidrs(Environment.GetEnvironmentVariable("ALLOWED_NETS"));

    static List<IPNetwork> ParseCidrs(string? text) =>
        (text ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                    .Select(x => IPNetwork.Parse(x)).ToList();

    public static string ValidateName(JsonNode? host)
    {
        string? h = host is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;
        if (h != null && IPAddress.TryParse(h, out _)) return h;                  // dòng 3: IS_IP_ADDRESS, kể cả "::1"
        if (h == null || h.Length > 253 || !Regex.IsMatch(h, @"^[A-Za-z0-9.-]+\z"))
            throw new PolicyError("Tên máy không hợp lệ");
        return h;
    }

    public static IPAddress ValidateTarget(JsonNode? host)
    {
        string name = ValidateName(host);
        IPAddress[] addrs;
        try { addrs = Dns.GetHostAddresses(name); }                              // RESOLVE
        catch (SocketException) { throw new ResolveError($"Không phân giải được {name}"); }
        foreach (var a in addrs)
            if (!AllowedNets.Any(n => n.Contains(a)))                            // IN_ANY
                throw new PolicyError("Ngoài phạm vi cho phép: " + a);
        if (addrs.Length == 0) throw new ResolveError($"Không phân giải được {name}");
        return addrs[0];                          // dùng đúng địa chỉ đã kiểm tra (chống DNS rebinding)
    }

    public static int ValidatePort(JsonNode? p)
    {
        if (p is not JsonValue v || v.GetValueKind() != JsonValueKind.Number
            || !v.TryGetValue(out long n) || n < 1 || n > 65535)                 // IS_INTEGER, phạm vi
            throw new PolicyError("Cổng không hợp lệ");
        return (int)n;
    }
}
