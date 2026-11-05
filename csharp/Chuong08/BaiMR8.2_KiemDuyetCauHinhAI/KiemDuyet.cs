using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace BaiMR8_2;

// Kiểm duyệt cấu hình do LLM sinh ra trước khi dùng (guardrail + vòng phản hồi).
// Nguyên tắc: đầu ra của LLM là DỮ LIỆU CHƯA TIN CẬY, giống dữ liệu nhận từ mạng; phải kiểm tra như kiểm tra
// gói tin từ client lạ:
//   1. Tách và phân tích JSON (LLM hay bọc JSON trong khối ```json kèm lời dẫn).
//   2. Lược đồ: đủ trường, đúng kiểu.
//   3. Ngữ nghĩa: VLAN 1–4094, không trùng; mạng con hợp lệ, nằm trong khối địa chỉ, không chồng lấn nhau
//      và không đụng hạ tầng đang dùng; cổng mặc định là địa chỉ máy hợp lệ trong mạng.
//   4. Chính sách an toàn: cấm "permit ip any any", cấm bật telnet.
// Nếu có lỗi: gửi danh sách lỗi lại cho LLM để sửa (tối đa --so-lan lần); hết lượt vẫn sai thì TỪ CHỐI.
// Cấu hình đạt mới được chuyển thành lệnh CLI – và vẫn cần người duyệt trước khi áp dụng.
// Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).

/// <summary>Mạng IPv4: địa chỉ mạng + độ dài tiền tố (phép tính trên số 32 bit như MR1.1).</summary>
readonly record struct Net4(uint Addr, int Len)
{
    public uint Mask => Len == 0 ? 0 : uint.MaxValue << (32 - Len);
    public uint Last => Addr | ~Mask;                          // địa chỉ quảng bá
    public bool Contains(uint a) => (a & Mask) == Addr;
    public bool Overlaps(Net4 o) => Addr <= o.Last && o.Addr <= Last;
    public bool SubnetOf(Net4 o) => Addr >= o.Addr && Last <= o.Last;
    public override string ToString() => $"{Ip.Str(Addr)}/{Len}";
}

static class Ip
{
    public static string Str(uint a) => $"{a >> 24}.{(a >> 16) & 255}.{(a >> 8) & 255}.{a & 255}";

    /// <summary>Địa chỉ IPv4 chặt chẽ: đúng 4 phần 0–255 (giống ipaddress.ip_address).</summary>
    public static uint ParseAddr(string s)
    {
        var p = s.Split('.');
        if (p.Length != 4 || p.Any(x => x.Length == 0 || x.Length > 3 || !x.All(char.IsAsciiDigit) || int.Parse(x) > 255 ||
                                        (x.Length > 1 && x[0] == '0')))
            throw new FormatException($"không phải IPv4: {s}");
        return p.Aggregate(0u, (v, x) => (v << 8) | uint.Parse(x));
    }

    /// <summary>Mạng "a.b.c.d/n" chặt chẽ: không chấp nhận bit máy (giống ip_network(strict=True)).</summary>
    public static Net4 ParseNet(string s)
    {
        var p = s.Contains('/') ? s.Split('/') : [s, "32"];       // không có tiền tố = /32 (như ip_network)
        if (p.Length != 2 || !int.TryParse(p[1], out int len) || len < 0 || len > 32 || !p[1].All(char.IsAsciiDigit))
            throw new FormatException($"không phải mạng: {s}");
        var n = new Net4(ParseAddr(p[0]), len);
        if ((n.Addr & n.Mask) != n.Addr) throw new FormatException($"{s} có bit máy");
        return n;
    }
}

static class KiemDuyet
{
    static readonly string BaseUrl = (Environment.GetEnvironmentVariable("LLM_BASE_URL") ?? "http://127.0.0.1:9000/v1").TrimEnd('/');
    static readonly string Model = Environment.GetEnvironmentVariable("LLM_MODEL") ?? "mock-model";
    const string System_ = "Bạn sinh cấu hình mạng. Chỉ trả về một đối tượng JSON có các khóa: " +
                           "vlan (danh sách {id, ten, mang, cong_mac_dinh}), acl (danh sách {ten, quy_tac}), " +
                           "dich_vu ({telnet, ssh}).";
    static readonly Regex ForbiddenRules = new(@"permit\s+(ip\s+)?any\s+any");
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(10) };

    static string Chat(JsonArray messages)
    {
        string? key = Environment.GetEnvironmentVariable("LLM_API_KEY");
        if (string.IsNullOrEmpty(key)) throw new InvalidOperationException("Thiếu biến môi trường LLM_API_KEY");
        var req = new JsonObject
        {
            ["model"] = Model, ["messages"] = messages.DeepClone(), ["temperature"] = 0,
            ["response_format"] = new JsonObject { ["type"] = "json_object" },
        };
        using var msg = new HttpRequestMessage(HttpMethod.Post, $"{BaseUrl}/chat/completions")
        {
            Content = new StringContent(req.ToJsonString(Opt), Encoding.UTF8, "application/json"),
        };
        msg.Headers.Authorization = new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", key);
        using var resp = Http.Send(msg);
        resp.EnsureSuccessStatusCode();
        var body = JsonNode.Parse(resp.Content.ReadAsStream())!;
        return body["choices"]![0]!["message"]!["content"]!.GetValue<string>();
    }

    /// <summary>Lấy JSON từ khối ```json ... ``` nếu có, nếu không thì dùng toàn bộ văn bản.</summary>
    static JsonNode? ExtractJson(string text)
    {
        var m = Regex.Match(text, @"```(?:json)?\s*\n(.*?)```", RegexOptions.Singleline);
        return JsonNode.Parse(m.Success ? m.Groups[1].Value : text);
    }

    static JsonValueKind Kind(JsonNode? n) => n?.GetValueKind() ?? JsonValueKind.Null;
    static string S(JsonNode? n) => n!.GetValue<string>();
    static long L(JsonNode? n) => n!.GetValue<long>();

    static List<string> CheckSchema(JsonNode? cfgNode)
    {
        var errs = new List<string>();
        if (cfgNode is not JsonObject cfg) return ["gốc phải là đối tượng JSON"];
        foreach (var (key, kind) in new[] { ("vlan", JsonValueKind.Array), ("acl", JsonValueKind.Array), ("dich_vu", JsonValueKind.Object) })
            if (Kind(cfg[key]) != kind) errs.Add($"thiếu khóa {key} hoặc sai kiểu");
        if (cfg["vlan"] is JsonArray vlans)
            for (int i = 0; i < vlans.Count; i++)
                foreach (var (key, kind) in new[] { ("id", JsonValueKind.Number), ("ten", JsonValueKind.String),
                                                    ("mang", JsonValueKind.String), ("cong_mac_dinh", JsonValueKind.String) })
                {
                    var v = (vlans[i] as JsonObject)?[key];
                    bool ok = Kind(v) == kind && (kind != JsonValueKind.Number || v!.AsValue().TryGetValue(out long _));   // id: số nguyên
                    if (!ok) errs.Add($"vlan[{i}].{key} thiếu hoặc sai kiểu");
                }
        // quy_tac phải là danh sách chuỗi, nếu không quy tắc cấm có thể lọt qua kiểm tra chính sách
        if (cfg["acl"] is JsonArray acls)
            for (int i = 0; i < acls.Count; i++)
            {
                var a = acls[i] as JsonObject;
                if (Kind(a?["ten"]) != JsonValueKind.String) errs.Add($"acl[{i}].ten thiếu hoặc sai kiểu");
                if (a?["quy_tac"] is not JsonArray q || q.Any(r => Kind(r) != JsonValueKind.String))
                    errs.Add($"acl[{i}].quy_tac thiếu hoặc sai kiểu");
            }
        return errs;
    }

    static List<string> CheckSemantics(JsonObject cfg, JsonObject infra)
    {
        var errs = new List<string>();
        var nets = new List<(string Name, Net4 Net)>();
        var block = Ip.ParseNet(S(infra["khoi_dia_chi"]));
        var daDung = infra["da_dung"]!.AsArray().Select(d => d!.AsObject()).ToList();
        var usedIds = daDung.ToDictionary(d => L(d["vlan"]), d => S(d["ten"]));
        var seenIds = new HashSet<long>();
        foreach (var v in cfg["vlan"]!.AsArray().Select(x => x!.AsObject()))
        {
            string name = S(v["ten"]);
            long id = L(v["id"]);
            if (id < 1 || id > 4094) errs.Add($"{name}: VLAN {id} ngoài miền 1-4094");
            else if (usedIds.TryGetValue(id, out var owner)) errs.Add($"{name}: VLAN {id} đã dùng cho {owner}");
            else if (seenIds.Contains(id)) errs.Add($"{name}: VLAN {id} bị trùng");
            seenIds.Add(id);
            Net4 net;
            try { net = Ip.ParseNet(S(v["mang"])); }                 // chặt chẽ: không chấp nhận bit máy
            catch (FormatException)
            {
                errs.Add($"{name}: mạng {S(v["mang"])} không hợp lệ");
                continue;
            }
            if (!net.SubnetOf(block)) errs.Add($"{name}: mạng {net} ngoài khối {block}");
            try
            {
                uint gw = Ip.ParseAddr(S(v["cong_mac_dinh"]));
                if (!net.Contains(gw) || gw == net.Addr || gw == net.Last)
                    errs.Add($"{name}: cổng mặc định {Ip.Str(gw)} không phải địa chỉ máy của {net}");
            }
            catch (FormatException)
            {
                errs.Add($"{name}: cổng mặc định {S(v["cong_mac_dinh"])} không phải IPv4 hợp lệ");
            }
            foreach (var (other, onet) in nets)
                if (net.Overlaps(onet)) errs.Add($"{name}: mạng {net} chồng lấn {other} ({onet})");
            foreach (var d in daDung)
                if (net.Overlaps(Ip.ParseNet(S(d["mang"])))) errs.Add($"{name}: mạng {net} đụng hạ tầng {S(d["ten"])} ({S(d["mang"])})");
            nets.Add((name, net));
        }
        return errs;
    }

    static List<string> CheckPolicy(JsonObject cfg)
    {
        var errs = new List<string>();
        foreach (var acl in cfg["acl"]!.AsArray().Select(x => x!.AsObject()))
            foreach (var rule in (acl["quy_tac"] as JsonArray ?? []).Select(r => r?.ToString() ?? ""))
                if (ForbiddenRules.IsMatch(rule)) errs.Add($"ACL {acl["ten"]}: cấm quy tắc \"{rule}\"");
        if (Kind(cfg["dich_vu"]!["telnet"]) != JsonValueKind.False)  // chỉ giá trị false tường minh mới đạt
            errs.Add("dich_vu.telnet phải tắt (mật khẩu đi dạng rõ), dùng SSH");
        return errs;
    }

    static (JsonObject? Cfg, List<string> Errs) Validate(string text, JsonObject infra)
    {
        JsonNode? cfg;
        try { cfg = ExtractJson(text); }
        catch (JsonException e) { return (null, [$"JSON hỏng: {e.Message}"]); }
        var errs = CheckSchema(cfg);
        if (errs.Count > 0) return (null, errs);
        var obj = cfg!.AsObject();
        return (obj, [.. CheckSemantics(obj, infra), .. CheckPolicy(obj)]);
    }

    /// <summary>Chuyển cấu hình đã duyệt thành lệnh CLI (kiểu IOS) – mẫu đơn giản.</summary>
    static List<string> Render(JsonObject cfg)
    {
        var lines = new List<string>();
        foreach (var v in cfg["vlan"]!.AsArray().Select(x => x!.AsObject()))
        {
            var net = Ip.ParseNet(S(v["mang"]));
            lines.AddRange([$"vlan {L(v["id"])}", $" name {S(v["ten"])}", $"interface Vlan{L(v["id"])}",
                            $" ip address {S(v["cong_mac_dinh"])} {Ip.Str(net.Mask)}"]);
        }
        foreach (var acl in cfg["acl"]!.AsArray().Select(x => x!.AsObject()))
        {
            lines.Add($"ip access-list extended {acl["ten"]}");
            lines.AddRange(acl["quy_tac"]!.AsArray().Select(r => $" {r}"));
        }
        lines.Add(Kind(cfg["dich_vu"]!["telnet"]) == JsonValueKind.True ? "service telnet" : "no service telnet");
        return lines;
    }

    public static int Run(string[] args)
    {
        var pos = new List<string>();
        int soLan = 3;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--so-lan") soLan = int.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        if (pos.Count < 2)
        {
            Console.WriteLine("Cách dùng: dotnet run -- <ha_tang.json> \"<yêu cầu>\" [--so-lan N]");
            return 2;
        }
        var infra = JsonNode.Parse(File.ReadAllText(pos[0], Encoding.UTF8))!.AsObject();
        var messages = new JsonArray
        {
            new JsonObject { ["role"] = "system", ["content"] = System_ },
            new JsonObject { ["role"] = "user", ["content"] = pos[1] },
        };
        Console.WriteLine($"[kiem-duyet] Yêu cầu: {pos[1]}");
        for (int attempt = 1; attempt <= soLan; attempt++)
        {
            string text;
            try { text = Chat(messages); }
            catch (Exception e) when (e is HttpRequestException || e is TaskCanceledException || e is InvalidOperationException)
            {
                Console.WriteLine($"[kiem-duyet] Lỗi gọi LLM: {e.Message}");
                return 1;
            }
            messages.Add(new JsonObject { ["role"] = "assistant", ["content"] = text });
            var (cfg, errs) = Validate(text, infra);
            if (errs.Count == 0)
            {
                Console.WriteLine($"[kiem-duyet] Lần {attempt}: ĐẠT");
                Console.WriteLine("[kiem-duyet] Lệnh CLI sinh từ cấu hình đã duyệt (chờ người duyệt):");
                foreach (var line in Render(cfg!)) Console.WriteLine("  " + line);
                return 0;
            }
            Console.WriteLine($"[kiem-duyet] Lần {attempt}: {errs.Count} lỗi");
            foreach (var e in errs) Console.WriteLine($"  - {e}");
            messages.Add(new JsonObject
            {
                ["role"] = "user",
                ["content"] = "Cấu hình chưa đạt, hãy sửa và chỉ trả về JSON:\n" + string.Join("\n", errs.Select(e => $"- {e}")),
            });
        }
        Console.WriteLine($"[kiem-duyet] TỪ CHỐI: sau {soLan} lần vẫn chưa đạt, không sinh lệnh");
        return 1;
    }
}
