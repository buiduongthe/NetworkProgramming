using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR8_1;

// Tác tử AI chẩn đoán mạng bằng gọi công cụ (tool calling), có lớp kiểm soát.
// Bài 8.3 viết phía server công cụ, bài này viết phía "chủ" (host/agent) – vòng lặp
//     hỏi LLM -> LLM yêu cầu công cụ -> chương trình KIỂM TRA rồi mới chạy -> gửi kết quả -> ...
// cho tới khi LLM trả lời bằng văn bản. LLM không bao giờ tự chạy lệnh; mọi thao tác mạng do mã của ta
// thực hiện, sau khi qua các lớp kiểm soát:
//   1. Chỉ chạy công cụ có trong danh mục Tools (chặn công cụ "bịa" do ảo giác).
//   2. Tham số phải là JSON hợp lệ, đủ trường, đúng kiểu, trong miền giá trị.
//   3. Chính sách đích: chỉ máy thuộc mạng phòng thí nghiệm (loopback, mạng riêng RFC 1918).
//   4. Giới hạn số vòng lặp; ghi nhật ký kiểm toán từng lời gọi.
// Cấu hình qua biến môi trường: LLM_BASE_URL, LLM_API_KEY, LLM_MODEL (như Bài 8.1).
class ToolError(string msg) : Exception(msg);

static class TacTu
{
    static readonly string BaseUrl = (Environment.GetEnvironmentVariable("LLM_BASE_URL") ?? "http://127.0.0.1:9000/v1").TrimEnd('/');
    static readonly string Model = Environment.GetEnvironmentVariable("LLM_MODEL") ?? "mock-model";
    const int MaxRounds = 4;
    // Mạng phòng thí nghiệm: loopback và RFC 1918 (IPv4); thêm loopback, ULA của IPv6 vì Windows thường phân giải
    // localhost thành cả ::1
    static readonly (IPAddress Net, int Len)[] AllowedNets =
        new[] { "127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "::1/128", "fc00::/7" }
        .Select(s => (IPAddress.Parse(s.Split('/')[0]), int.Parse(s.Split('/')[1]))).ToArray();
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    static readonly HttpClient Http = new() { Timeout = TimeSpan.FromSeconds(10) };

    // ---------- Thân công cụ (chạy bằng socket API, không gọi shell) ----------
    static List<string> Resolve(string name)
    {
        try { return Dns.GetHostAddresses(name).Select(a => a.ToString()).Distinct().Order(StringComparer.Ordinal).ToList(); }
        catch (SocketException) { return []; }
    }

    static bool InNet(IPAddress a, (IPAddress Net, int Len) n)
    {
        if (a.AddressFamily != n.Net.AddressFamily) return false;
        byte[] x = a.GetAddressBytes(), y = n.Net.GetAddressBytes();
        for (int i = 0, bits = n.Len; bits > 0; i++, bits -= 8)
        {
            int mask = bits >= 8 ? 0xFF : (0xFF << (8 - bits)) & 0xFF;
            if ((x[i] & mask) != (y[i] & mask)) return false;
        }
        return true;
    }

    /// <summary>Chính sách đích: mọi địa chỉ của host phải thuộc AllowedNets.</summary>
    static List<string> CheckTarget(string host)
    {
        var addrs = Resolve(host);
        if (addrs.Count == 0) throw new ToolError($"không phân giải được {host}");
        foreach (var a in addrs)
            if (!AllowedNets.Any(n => InNet(IPAddress.Parse(a), n)))
                throw new ToolError($"{a} nằm ngoài mạng phòng thí nghiệm");
        return addrs;
    }

    static SortedDictionary<string, object> ToolResolve(JsonObject args) =>
        new(StringComparer.Ordinal) { ["dia_chi"] = Resolve(args["ten"]!.GetValue<string>()) };

    static SortedDictionary<string, object> ToolPort(JsonObject args)
    {
        var addrs = CheckTarget(args["host"]!.GetValue<string>());
        int port = args["cong"]!.GetValue<int>();
        int timeout = args["thoi_gian_cho_ms"]?.GetValue<int>() ?? 1000;
        string state = "FILTERED";
        foreach (var a in addrs)
        {
            using var s = new Socket(IPAddress.Parse(a).AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                using var cts = new CancellationTokenSource(timeout);
                s.ConnectAsync(IPAddress.Parse(a), port, cts.Token).AsTask().GetAwaiter().GetResult();
                return new(StringComparer.Ordinal) { ["trang_thai"] = "OPEN", ["dia_chi"] = a };
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused) { state = "CLOSED"; }   // RST
            catch (Exception e) when (e is SocketException || e is OperationCanceledException) { }                       // hết giờ
        }
        return new(StringComparer.Ordinal) { ["trang_thai"] = state };
    }

    record Tool(Func<JsonObject, SortedDictionary<string, object>> Fn, string Description, JsonObject Schema);

    // Danh mục công cụ: tên -> (hàm, mô tả, lược đồ tham số JSON Schema gửi cho LLM)
    static readonly Dictionary<string, Tool> Tools = new()
    {
        ["phan_giai_ten"] = new(ToolResolve, "Phân giải tên máy thành danh sách địa chỉ IP", new JsonObject
        {
            ["type"] = "object", ["required"] = new JsonArray("ten"),
            ["properties"] = new JsonObject { ["ten"] = new JsonObject { ["type"] = "string", ["description"] = "tên máy cần phân giải" } },
        }),
        ["kiem_tra_cong"] = new(ToolPort, "Kiểm tra cổng TCP: OPEN, CLOSED hoặc FILTERED", new JsonObject
        {
            ["type"] = "object", ["required"] = new JsonArray("host", "cong"),
            ["properties"] = new JsonObject
            {
                ["host"] = new JsonObject { ["type"] = "string" },
                ["cong"] = new JsonObject { ["type"] = "integer", ["minimum"] = 1L, ["maximum"] = 65535L },
                ["thoi_gian_cho_ms"] = new JsonObject { ["type"] = "integer", ["minimum"] = 100L, ["maximum"] = 5000L },
            },
        }),
    };

    /// <summary>Kiểm tra tham số theo lược đồ (tập con nhỏ của JSON Schema: required, type, min, max).</summary>
    static void Validate(JsonObject schema, JsonNode? argsNode)
    {
        if (argsNode is not JsonObject args) throw new ToolError("tham số phải là đối tượng JSON");
        foreach (var key in schema["required"]!.AsArray().Select(k => k!.GetValue<string>()))
            if (!args.ContainsKey(key)) throw new ToolError($"thiếu tham số {key}");
        var props = schema["properties"]!.AsObject();
        foreach (var (key, value) in args)
        {
            if (props[key] is not JsonObject spec) throw new ToolError($"tham số lạ {key}");
            string type = spec["type"]!.GetValue<string>();
            var kind = value?.GetValueKind();
            long n = 0;
            if (type == "integer" && !(kind == JsonValueKind.Number && value!.AsValue().TryGetValue(out n)))
                throw new ToolError($"{key} phải là số nguyên");           // true/false, số thực, chuỗi đều bị loại
            if (type == "string" && kind != JsonValueKind.String) throw new ToolError($"{key} phải là chuỗi");
            if (spec["minimum"] is JsonNode min && (n < min.GetValue<long>() || n > spec["maximum"]!.GetValue<long>()))
                throw new ToolError($"{key} ngoài miền [{min}, {spec["maximum"]}]");
        }
    }

    /// <summary>Ba lớp kiểm soát trước khi chạy: danh mục, tham số, chính sách đích (trong thân công cụ).</summary>
    static SortedDictionary<string, object> RunTool(string name, string arguments)
    {
        if (!Tools.TryGetValue(name, out var tool)) throw new ToolError("công cụ không có trong danh mục");
        JsonNode? args;
        try { args = JsonNode.Parse(arguments); }
        catch (JsonException) { throw new ToolError("tham số không phải JSON"); }
        Validate(tool.Schema, args);
        return tool.Fn(args!.AsObject());
    }

    /// <summary>JSON gọn, khóa theo thứ tự chữ cái (giống sort_keys=True của Python, json.Marshal của Go với map).</summary>
    static string Compact(object obj) => JsonSerializer.Serialize(obj, Opt);

    /// <summary>Ngắt dòng tham lam theo từ, rộng tối đa width ký tự (giống textwrap.wrap của Python).</summary>
    static List<string> Wrap(string text, int width)
    {
        var lines = new List<string>();
        var cur = new StringBuilder();
        foreach (var w in text.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries))
        {
            if (cur.Length > 0 && cur.Length + 1 + w.Length > width)
            {
                lines.Add(cur.ToString());
                cur.Clear();
            }
            if (cur.Length > 0) cur.Append(' ');
            cur.Append(w);
        }
        if (cur.Length > 0) lines.Add(cur.ToString());
        return lines;
    }

    // ---------- Gọi API LLM (không truyền luồng) ----------
    static JsonObject Chat(JsonArray messages)
    {
        string? key = Environment.GetEnvironmentVariable("LLM_API_KEY");
        if (string.IsNullOrEmpty(key)) throw new InvalidOperationException("Thiếu biến môi trường LLM_API_KEY");
        var tools = new JsonArray(Tools.Select(t => (JsonNode)new JsonObject
        {
            ["type"] = "function",
            ["function"] = new JsonObject { ["name"] = t.Key, ["description"] = t.Value.Description, ["parameters"] = t.Value.Schema.DeepClone() },
        }).ToArray());
        var req = new JsonObject { ["model"] = Model, ["messages"] = messages.DeepClone(), ["temperature"] = 0, ["tools"] = tools };
        using var msg = new HttpRequestMessage(HttpMethod.Post, $"{BaseUrl}/chat/completions")
        {
            Content = new StringContent(req.ToJsonString(Opt), Encoding.UTF8, "application/json"),
        };
        msg.Headers.Authorization = new System.Net.Http.Headers.AuthenticationHeaderValue("Bearer", key);
        using var resp = Http.Send(msg);
        resp.EnsureSuccessStatusCode();
        var body = JsonNode.Parse(resp.Content.ReadAsStream())!;
        return body["choices"]![0]!["message"]!.AsObject();
    }

    public static int Run(string[] args)
    {
        string question = args.Length > 0 ? args[0] : "Kiểm tra localhost cổng 80";
        Console.WriteLine($"[tac-tu] Hỏi: {question}");
        var messages = new JsonArray
        {
            new JsonObject { ["role"] = "system", ["content"] = "Bạn là trợ lý chẩn đoán mạng. Chỉ dùng công cụ được cung cấp." },
            new JsonObject { ["role"] = "user", ["content"] = question },
        };
        for (int rnd = 1; rnd <= MaxRounds; rnd++)
        {
            JsonObject msg;
            try { msg = Chat(messages); }
            catch (Exception e) when (e is HttpRequestException || e is TaskCanceledException || e is InvalidOperationException)
            {
                Console.WriteLine($"[tac-tu] Lỗi gọi LLM: {e.Message}");
                return 1;
            }
            messages.Add(msg.DeepClone());
            var calls = msg["tool_calls"] as JsonArray;
            if (calls == null || calls.Count == 0)
            {
                Console.WriteLine($"[tac-tu] Vòng {rnd}: LLM trả lời");
                foreach (var line in Wrap(msg["content"]?.GetValue<string>() ?? "", 58)) Console.WriteLine("  " + line);
                return 0;
            }
            Console.WriteLine($"[tac-tu] Vòng {rnd}: LLM yêu cầu {calls.Count} công cụ");
            foreach (var c in calls)
            {
                string name = c!["function"]!["name"]!.GetValue<string>(), arguments = c["function"]!["arguments"]!.GetValue<string>();
                object result;
                try
                {
                    result = RunTool(name, arguments);
                    Console.WriteLine($"  CHẠY  {name}{arguments} -> {Compact(result)}");
                }
                catch (ToolError e)
                {
                    result = new SortedDictionary<string, object> { ["loi"] = e.Message };
                    Console.WriteLine($"  CHẶN  {name}{arguments}: {e.Message}");
                }
                messages.Add(new JsonObject { ["role"] = "tool", ["tool_call_id"] = c["id"]!.GetValue<string>(), ["content"] = Compact(result) });
            }
        }
        Console.WriteLine($"[tac-tu] Dừng: quá {MaxRounds} vòng mà chưa có câu trả lời");
        return 0;
    }
}
