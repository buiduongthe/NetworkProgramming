using System.Diagnostics;
using System.Net.Http.Headers;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai8_1;

// Mã giả 8.3 – thùng token (chép gọn từ Bài 8.2): khóa bất đồng bộ, chờ ngoài khóa
class Bucket(double cap, double rate)
{
    public double Cap = cap, Rate = rate, Tokens = cap;
    public double Last = Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;
    public readonly SemaphoreSlim Lock = new(1, 1);                       // ASYNC_LOCK()

    public async Task AcquireTokens(double n)
    {
        if (n > Cap) throw new ArgumentException("n vượt dung lượng");
        while (true)
        {
            double waitMs;
            await Lock.WaitAsync();
            try
            {
                double now = Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;
                Tokens = Math.Min(Cap, Tokens + (now - Last) * Rate);
                Last = now;
                if (Tokens >= n) { Tokens -= n; return; }
                waitMs = (n - Tokens) / Rate * 1000;
            }
            finally { Lock.Release(); }
            await Task.Delay(TimeSpan.FromMilliseconds(waitMs));            // không giữ khóa khi chờ
        }
    }
}

// Mã giả 8.2 – CALL_LLM: giới hạn tần suất, thử lại có lùi thời gian, thời gian chờ (Mục 8.2.3)
static class LlmCall
{
    public static readonly string BaseUrl =
        (Environment.GetEnvironmentVariable("LLM_BASE_URL") ?? "http://127.0.0.1:9000/v1").TrimEnd('/');
    static readonly Bucket TokenBucket = new(20000, 2000);   // hạn mức tự đặt: 20 000 token, nạp 2 000/giây
    // Dùng chung một HttpClient cho cả ứng dụng (Mục 5.4.4); thời gian chờ kết nối 3 giây
    static readonly HttpClient Http = new(new SocketsHttpHandler { ConnectTimeout = TimeSpan.FromMilliseconds(3000) })
    { Timeout = Timeout.InfiniteTimeSpan };
    public static readonly JsonSerializerOptions JsonOpts = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    // SECRET(name): bí mật chỉ lấy từ biến môi trường, không ghi trong mã nguồn, không in ra
    public static string Secret(string name) =>
        Environment.GetEnvironmentVariable(name) is { Length: > 0 } v ? v
        : throw new InvalidOperationException($"Thiếu biến môi trường {name}");

    // ESTIMATE_TOKENS: ước lượng thô ~4 ký tự/token cho đầu vào, cộng max_tokens cho đầu ra
    static int EstimateTokens(JsonObject req) =>
        req["messages"]!.AsArray().Sum(m => m!["content"]!.GetValue<string>().Length) / 4
        + req["max_tokens"]!.GetValue<int>();

    // PARSE_RETRY_AFTER: số giây hoặc ngày giờ HTTP (RFC 9110); trả về ms
    static double? ParseRetryAfter(HttpResponseMessage resp)
    {
        RetryConditionHeaderValue? ra = resp.Headers.RetryAfter;
        if (ra?.Delta is TimeSpan d) return d.TotalMilliseconds;
        if (ra?.Date is DateTimeOffset t) return Math.Max(0, (t - DateTimeOffset.UtcNow).TotalMilliseconds);
        return null;
    }

    // HTTPS_POST: HttpClient tự bọc TLS khi url là https://; chờ tối đa timeoutMs tới khi có tiêu đề
    static async Task<HttpResponseMessage> HttpsPost(string url, JsonObject req, Dictionary<string, string> hdrs, int timeoutMs,
                                                     CancellationToken ct)
    {
        var msg = new HttpRequestMessage(HttpMethod.Post, url)
        { Content = new StringContent(req.ToJsonString(JsonOpts), Encoding.UTF8, "application/json") };
        foreach (var (k, v) in hdrs) msg.Headers.TryAddWithoutValidation(k, v);
        msg.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("text/event-stream"));
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);   // hủy của người dùng hoặc hết hạn
        cts.CancelAfter(timeoutMs);
        try
        {
            return await Http.SendAsync(msg, HttpCompletionOption.ResponseHeadersRead, cts.Token);
        }
        catch (OperationCanceledException) when (ct.IsCancellationRequested)
        {
            throw;                                                      // Cancelled: không đổi thành NetError
        }
        catch (HttpRequestException e) when (e.InnerException is SocketException { SocketErrorCode: SocketError.ConnectionRefused })
        {
            throw new NetError("REFUSED", retry: false);
        }
        catch (HttpRequestException e)
        {
            throw new NetError($"UNREACHABLE ({e.Message})", retry: true);
        }
        catch (TaskCanceledException)
        {
            throw new NetError("Hết thời gian chờ phản hồi", retry: true);
        }
    }

    // Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct)
    static async Task<T> RetryAsync<T>(Func<CancellationToken, Task<T>> op, int maxTries, double baseMs, double capMs,
                                       CancellationToken ct)
    {
        for (int k = 0; ; k++)
        {
            try
            {
                return await op(ct);                                    // dòng 4
            }
            catch (Exception e) when ((e is TimeoutException || e is NetError { Retry: true })   // dòng 5–7;
                                      && k < maxTries - 1)                                       // Cancelled không bắt
            {
                double delay = Random.Shared.NextDouble() * Math.Min(capMs, baseMs * Math.Pow(2, k));  // jitter
                if (e is NetError { RetryAfter: double ra }) delay = Math.Max(delay, ra);           // server đề nghị
                Console.WriteLine($"[llm] Lần thử {k + 1}/{maxTries}: {e.Message} – chờ {delay:F0} ms rồi thử lại");
                await Task.Delay(TimeSpan.FromMilliseconds(delay), ct); // DELAY_ASYNC(delay, ct)
            }
        }
    }

    // Mã giả 8.2 – CALL_LLM(req, on_token, ct)
    public static Task<LlmStream.State> CallLlm(JsonObject req, Action<string> onToken, CancellationToken ct)
    {
        bool started = false;
        async Task<LlmStream.State> OneAttempt(CancellationToken ct)    // dòng 3
        {
            await TokenBucket.AcquireTokens(EstimateTokens(req));                  // 8.2.4
            var hdrs = new Dictionary<string, string> { ["Authorization"] = "Bearer " + Secret("LLM_API_KEY") };
            using HttpResponseMessage resp = await HttpsPost(BaseUrl + "/chat/completions", req, hdrs, 30000, ct);   // dòng 6–7
            int status = (int)resp.StatusCode;
            if (status is 408 or 429 or 500 or 502 or 503 or 504)
                throw new NetError($"HTTP {status}", retry: !started, retryAfter: ParseRetryAfter(resp));
            if (status >= 400) throw new NetError($"HTTP {status}", retry: false);
            void Tok(string d) { started = true; onToken(d); }
            return await LlmStream.StreamCompletion(await resp.Content.ReadAsStreamAsync(), Tok);
        }
        return RetryAsync(OneAttempt, 5, 500, 20000, ct);               // dòng 16
    }
}
