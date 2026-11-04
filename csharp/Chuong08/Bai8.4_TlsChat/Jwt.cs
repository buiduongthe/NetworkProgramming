using System.Buffers.Text;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai8_4;

class JwtError(string message) : Exception(message);

// Cấp và kiểm tra JWT ký HMAC-SHA-256 (HS256). VERIFY_JWT của Mã giả 8.6 theo Mục 7.2.4: đúng thuật toán
// (từ chối alg "none"), chữ ký, exp chưa quá hạn, iss là ISSUER, aud là đối tượng mong đợi.
static class Jwt
{
    public const string Issuer = "auth.lab.local";

    // SECRET(name): bí mật chỉ lấy từ biến môi trường
    public static string Secret(string name) =>
        Environment.GetEnvironmentVariable(name) is { Length: > 0 } v ? v : throw new InvalidOperationException($"Thiếu biến môi trường {name}");

    static string B64(byte[] b) => Base64Url.EncodeToString(b);
    static byte[] Mac(string key, string data) => HMACSHA256.HashData(Encoding.UTF8.GetBytes(key), Encoding.ASCII.GetBytes(data));

    public static string Sign(JsonObject claims, string key)
    {
        string head = B64(Encoding.UTF8.GetBytes("{\"alg\":\"HS256\",\"typ\":\"JWT\"}"));
        string body = B64(Encoding.UTF8.GetBytes(claims.ToJsonString(Frame.Opts)));
        return $"{head}.{body}.{B64(Mac(key, head + "." + body))}";
    }

    public static JsonObject Verify(string? token, string key, string issuer, string audience)
    {
        string[] parts = token?.Split('.') ?? [];
        if (parts.Length != 3) throw new JwtError("định dạng sai");
        JsonObject header, claims;
        byte[] sig;
        try
        {
            header = JsonNode.Parse(Base64Url.DecodeFromChars(parts[0])) as JsonObject ?? throw new FormatException();
            claims = JsonNode.Parse(Base64Url.DecodeFromChars(parts[1])) as JsonObject ?? throw new FormatException();
            sig = Base64Url.DecodeFromChars(parts[2]);
        }
        catch (Exception e) when (e is FormatException or JsonException) { throw new JwtError("định dạng sai"); }
        if (Frame.Str(header["alg"]) != "HS256") throw new JwtError("thuật toán không được chấp nhận");
        if (!CryptographicOperations.FixedTimeEquals(sig, Mac(key, parts[0] + "." + parts[1])))   // thời gian hằng
            throw new JwtError("chữ ký sai");
        if (claims["exp"] is not JsonValue exp || !exp.TryGetValue(out double e2) || e2 <= DateTimeOffset.UtcNow.ToUnixTimeSeconds())
            throw new JwtError("đã hết hạn");
        if (Frame.Str(claims["iss"]) != issuer) throw new JwtError("iss không đúng");
        JsonNode? aud = claims["aud"];
        bool audOk = aud is JsonArray arr ? arr.Any(x => Frame.Str(x) == audience) : Frame.Str(aud) == audience;
        if (!audOk) throw new JwtError("aud không đúng");
        if (string.IsNullOrEmpty(Frame.Str(claims["sub"]))) throw new JwtError("thiếu sub");
        return claims;
    }

    // Công cụ cấp token thử nghiệm (vai trò server ủy quyền giản lược): token <sub> <tệp> [aud] [thời_hạn_giây]
    public static int TokenTool(string[] args)
    {
        if (args.Length < 2) { Console.WriteLine("Cách dùng: dotnet run -- token <sub> <tệp_token> [aud] [thời_hạn_giây]"); return 1; }
        string sub = args[0], path = args[1], aud = args.Length > 2 ? args[2] : "chat";
        int ttl = args.Length > 3 ? int.Parse(args[3]) : 3600;
        long now = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        var claims = new JsonObject { ["iss"] = Issuer, ["sub"] = sub, ["aud"] = aud, ["iat"] = now, ["exp"] = now + ttl };
        string token = Sign(claims, Secret("CHAT_JWT_KEY"));
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(path))!);
        File.WriteAllText(path, token);
        Console.WriteLine($"[token] Đã cấp token sub={sub}, aud={aud}, iss={Issuer}, hết hạn sau {ttl} s -> {path}");
        return 0;
    }
}
