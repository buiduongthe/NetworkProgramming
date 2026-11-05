using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR4_2;

// Phần dùng chung của trò chơi đua xe UDP: hằng số, vật lý, giả lập mất gói, bot, vẽ khung.
// Đơn vị: vị trí x và vận tốc v là số nguyên, tính bằng 1/100 ô (đường đua dài 40 ô = 4000).
// Dùng số nguyên để server và client mọi ngôn ngữ tính ra CÙNG một kết quả (không sai số dấu phẩy động).
// Luật chơi: mỗi tick (50 ms) xe nhận một đầu vào: 1 = ga, 0 = nhả ga, -1 = phanh.
// Đoạn đường từ ô 20 đến ô 26 là khúc cua: đi vào khúc cua nhanh hơn 0,50 ô/tick thì xe bị văng (tụt tốc độ).
static class DuaXe
{
    public const int TickMs = 50;           // server mô phỏng 20 tick mỗi giây
    public const int Start = 20;            // đếm ngược 20 tick (1 s) trước khi xuất phát
    public const long Track = 4000;         // vạch đích
    public const long ZoneStart = 2000, ZoneEnd = 2600;   // khúc cua
    public const long VCua = 50;            // tốc độ tối đa an toàn trong khúc cua
    public const long VVang = 10;           // tốc độ còn lại sau khi văng
    public const long Acc = 6, Drag = 2, Brake = 12, VMax = 90;
    public const int MaxRace = 600;         // giới hạn 30 s
    public const int Lead = 2;              // client gửi đầu vào cho tick k+2 khi nhận ảnh chụp tick k
    public const int Window = 3;            // mỗi gói đầu vào mang kèm 3 đầu vào gần nhất (dư thừa chống mất gói)
    public const int EndTicks = 10;         // sau khi đua xong, server gửi thêm 10 ảnh chụp kết quả rồi mới dừng
    public static readonly Dictionary<int, string> InputText = new() { [1] = "ga", [0] = "nhả", [-1] = "phanh" };

    /// <summary>Một tick vật lý; trả (x, v, có_văng). Server và client dùng chung hàm này.</summary>
    public static (long X, long V, bool Crashed) Step(long x, long v, int inp)
    {
        if (inp > 0) v = Math.Min(v + Acc, VMax);
        else if (inp < 0) v = Math.Max(v - Brake, 0);
        else v = Math.Max(v - Drag, 0);
        x += v;
        bool crashed = ZoneStart <= x && x < ZoneEnd && v > VCua;
        if (crashed) v = VVang;
        return (x, v, crashed);
    }

    /// <summary>Giả lập mất gói: true nếu gói (hướng, xe, số thứ tự) bị "mất".
    /// Quyết định là hàm băm của các tham số, không phụ thuộc thời điểm, nên cùng seed cho cùng các gói bị mất
    /// ở mọi lần chạy và ở mọi bản ngôn ngữ.</summary>
    public static bool Lossy(long seed, int direction, int car, long key, int pct)
    {
        if (pct <= 0) return false;
        const ulong m = 0xFFFFFFFF;
        ulong h = unchecked((ulong)(seed * 2654435761L + direction * 40503L + car * 9973L + key * 2246822519L)) & m;
        h ^= h >> 16;
        h = (h * 0x45D9F3B) & m;
        h ^= h >> 16;
        h = (h * 0x45D9F3B) & m;
        h ^= h >> 16;
        return (long)(h % 100) < pct;
    }

    // ---------------------------------------------------------------- bot
    // (khoảng an toàn trước khúc cua, tốc độ trong cua)
    public static readonly Dictionary<string, (long Safety, long Limit)> Styles = new()
        { ["lieu"] = (0, VCua), ["can_than"] = (300, VCua - 10) };

    /// <summary>Nếu tick này dùng inp rồi phanh liên tục, xe có vào khúc cua quá nhanh không?</summary>
    static bool WouldCrash(long x, long v, int inp, long zoneStart, long limit)
    {
        v = inp switch { 1 => Math.Min(v + Acc, VMax), 0 => Math.Max(v - Drag, 0), _ => Math.Max(v - Brake, 0) };
        x += v;
        while (true)
        {
            if (x >= ZoneEnd) return false;
            if (x >= zoneStart && v > limit) return true;
            if (v <= limit) return false;
            v = Math.Max(v - Brake, 0);
            x += v;
        }
    }

    /// <summary>Chọn đầu vào: ga nhiều nhất có thể mà vẫn kịp phanh trước khúc cua.</summary>
    public static int BotInput(long x, long v, string style)
    {
        var (safety, limit) = Styles[style];
        long zoneStart = ZoneStart - safety;
        if (zoneStart <= x && x < ZoneEnd)          // đang ở khúc cua (hoặc vùng an toàn trước nó)
        {
            if (v + Acc <= limit) return 1;
            return v - Drag <= limit ? 0 : -1;
        }
        foreach (int inp in new[] { 1, 0, -1 })
            if (!WouldCrash(x, v, inp, zoneStart, limit)) return inp;
        return -1;
    }

    // ---------------------------------------------------------------- hiển thị
    /// <summary>Số nguyên 1/100 ô -> chuỗi có hai chữ số thập phân (ví dụ 1234 -> "12.34").</summary>
    public static string Fmt2(long n) => $"{n / 100}.{n % 100:D2}";

    /// <summary>Vẽ một làn đua 40 ký tự: '~' là khúc cua, '|' là vạch đích.</summary>
    public static string TrackLine(char letter, long x)
    {
        long pos = Math.Min(39, x / 100);
        var sb = new StringBuilder();
        for (int i = 0; i < 40; i++)
        {
            if (i == pos) sb.Append(letter);
            else if (ZoneStart / 100 <= i && i < ZoneEnd / 100) sb.Append('~');
            else sb.Append('.');
        }
        return sb.Append('|').ToString();
    }

    // ---------------------------------------------------------------- JSON
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    public static byte[] Encode(JsonNode obj) => Encoding.UTF8.GetBytes(obj.ToJsonString(Opt));

    public static JsonObject? Decode(ReadOnlySpan<byte> data)
    {
        try { return JsonNode.Parse(Encoding.UTF8.GetString(data)) as JsonObject; }
        catch (JsonException) { return null; }
    }

    public static string? Str(this JsonObject o, string k) =>
        o[k] is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;

    public static bool TryLong(JsonNode? n, out long v)
    {
        v = 0;
        return n is JsonValue jv && jv.GetValueKind() == JsonValueKind.Number && jv.TryGetValue(out v);
    }

    public static long Long(this JsonObject o, string k) => TryLong(o[k], out long v) ? v : 0;
}
