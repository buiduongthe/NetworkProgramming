using System.Text.Json.Nodes;

namespace Bai8_1;

// NetError của Mã giả 2.2: mang cờ có thể thử lại và thời gian chờ server đề nghị (ms)
class NetError(string message, bool retry = false, double? retryAfter = null) : Exception(message)
{
    public bool Retry { get; } = retry;
    public double? RetryAfter { get; } = retryAfter;
}

// Mã giả 8.1 – STREAM_COMPLETION, kế thừa Mã giả 7.5 – SSE_READ (Mục 8.2.2, 7.3.3)
static class LlmStream
{
    public const int IdleTimeoutMs = 30000;       // thời gian chờ nhàn rỗi giữa hai dòng của luồng

    public class State { public string Text = ""; public JsonNode? Usage; public bool Done; }

    // READ_LINE (Mã giả 3.5) trên thân phản hồi; HttpClient đã giải mã khối (chunked).
    // Trả về null khi thân kết thúc hoặc kết nối bị ngắt (SSE_READ chỉ dừng ở hai trường hợp này).
    static async Task<string?> ReadLine(StreamReader r, int maxLen)
    {
        using var cts = new CancellationTokenSource(IdleTimeoutMs);
        string? line;
        try
        {
            line = await r.ReadLineAsync(cts.Token);
        }
        catch (Exception e) when (e is IOException or OperationCanceledException or HttpRequestException)
        {
            return null;                           // ngắt kết nối, khối bị cắt dở, hết thời gian chờ
        }
        if (line != null && line.Length > maxLen) throw new NetError("Dòng SSE quá dài");
        return line;
    }

    // Mã giả 7.5 – SSE_READ(r, on_event)
    public static async Task SseRead(StreamReader r, Action<string, string> onEvent)
    {
        string type = "message";
        var data = new List<string>();
        string? line;
        while ((line = await ReadLine(r, 65536)) != null)
        {
            if (line == "")                                    // dòng trống: hết một sự kiện
            {
                if (data.Count > 0) onEvent(type, string.Join("\n", data));
                type = "message"; data = new List<string>();
            }
            else if (!line.StartsWith(':'))                     // ":": dòng chú thích
            {
                int i = line.IndexOf(':');
                string field = i < 0 ? line : line[..i];
                string value = i < 0 ? "" : line[(i + 1)..];
                if (value.StartsWith(' ')) value = value[1..];  // STRIP_ONE_SPACE
                if (field == "event") type = value;
                else if (field == "data") data.Add(value);
                // "id", "retry": không dùng trong bài này
            }
        }                                                      // null: server đóng luồng
    }

    // Mã giả 8.1 – STREAM_COMPLETION(r, on_token); body: thân phản hồi sau tiêu đề
    public static async Task<State> StreamCompletion(Stream body, Action<string> onToken)
    {
        var st = new State();                                                  // dòng 1
        void OnEvent(string type, string data)                                 // dòng 2
        {
            if (data == "[DONE]") { st.Done = true; return; }                   // dòng 3
            JsonNode chunk = JsonNode.Parse(data)!;                             // dòng 4
            if (chunk["usage"] is JsonNode u) st.Usage = u;                     // dòng 5–6
            if (chunk["choices"] is not JsonArray choices || choices.Count == 0) return;   // dòng 7
            string? d = choices[0]?["delta"]?["content"]?.GetValue<string>();   // dòng 8
            if (d != null)                                                      // dòng 9–12
            {
                st.Text += d;
                onToken(d);
            }
        }
        using var r = new StreamReader(body, System.Text.Encoding.UTF8);
        await SseRead(r, OnEvent);                                             // dòng 14
        if (!st.Done) throw new NetError("Luồng bị ngắt giữa chừng");           // dòng 15
        return st;                                                             // dòng 16
    }
}
