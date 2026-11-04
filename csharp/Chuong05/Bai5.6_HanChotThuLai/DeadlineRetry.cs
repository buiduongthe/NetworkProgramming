using System.Diagnostics;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai5_6;

// Mã giả 5.7 REQUEST_WITH_DEADLINE (Mục 5.4.2) và Mã giả 5.8 RETRY_ASYNC (Mục 5.4.3).
// Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ), 2.2 (SAFE_CONNECT, NetError có Retry), 4.4 (vòng gửi lại).
// Ánh xạ: CANCEL_AFTER ↔ CancellationTokenSource(timeoutMs); LINK ↔ CreateLinkedTokenSource.
static class DeadlineRetry
{
    static readonly Stopwatch Clock = new();
    static void Log(string text) => Console.WriteLine($"[client] [t={Clock.ElapsedMilliseconds,5} ms] {text}");

    // Mã giả 5.7
    public static async Task<JsonNode> RequestWithDeadlineAsync(NetworkStream sock, JsonNode req, int timeoutMs, CancellationToken userCt)
    {
        using var deadline = new CancellationTokenSource(timeoutMs);    // CANCEL_AFTER(timeout_ms)
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(userCt, deadline.Token);  // LINK: một trong hai
        try
        {
            await Khung.SendJsonAsync(sock, req, linked.Token);
            return await Khung.RecvJsonAsync(sock, linked.Token);
        }
        catch (OperationCanceledException)
        {
            sock.Close();                                               // trạng thái khung không xác định
            if (userCt.IsCancellationRequested) throw;                  // RAISE Cancelled
            throw new TimeoutException($"Quá hạn {timeoutMs} ms");
        }
    }

    // Mã giả 5.8 – op(ct) tạo một lần thử mới; ct là tín hiệu hủy của nơi gọi
    public static async Task<T> RetryAsync<T>(Func<CancellationToken, Task<T>> op, int maxTries, int baseMs, int capMs,
                                              CancellationToken ct)
    {
        if (maxTries < 1) throw new ArgumentException("maxTries phải ≥ 1");
        for (int k = 0; ; k++)
        {
            try
            {
                return await op(ct);                                    // dòng 4
            }
            catch (Exception e) when (e is TimeoutException || e is NetError)   // dòng 5: Cancelled không bắt
            {
                bool ok = e is TimeoutException || ((NetError)e).Retry;  // dòng 6: Timeout luôn đáng thử lại
                if (!ok || k == maxTries - 1) throw;                    // dòng 7
                int limit = (int)Math.Min(capMs, baseMs * Math.Pow(2, k));
                double delay = Random.Shared.NextDouble() * limit;       // dòng 8: jitter, RANDOM(0, MIN(cap, base·2^k))
                if (e is NetError { RetryAfter: not null } ne)          // dòng 9–11: server đề nghị
                    delay = Math.Max(delay, ne.RetryAfter.Value);
                string code = e is NetError n ? n.Code : "TIMEOUT";
                Log($"  lỗi {code} (có thể thử lại); chờ {delay:F0} ms, giới hạn trên {limit} ms");
                await Task.Delay(TimeSpan.FromMilliseconds(delay), ct); // dòng 12: DELAY_ASYNC(delay, ct)
            }
        }
    }

    // Tạo hàm op(ct): mỗi lần gọi là một lần thử mới (kết nối mới, yêu cầu mới)
    static Func<CancellationToken, Task<JsonNode>> MakeOp(string host, int port, JsonNode req, int deadlineMs)
    {
        int tries = 0;
        return async ct =>
        {
            Log($"Lần thử {++tries}: gửi {Khung.JsonText(req)}");
            Socket s = await Net.SafeConnectAsync(host, port, 3000, ct);
            using var sock = new NetworkStream(s, ownsSocket: true);
            JsonNode resp = await RequestWithDeadlineAsync(sock, req, deadlineMs, ct);   // có thể ném TimeoutException
            if ((int?)resp["status"] == 503)
                throw new NetError("UNAVAILABLE", true, null, (int?)resp["retry_after_ms"]);
            return resp;
        };
    }

    static async Task ScenarioAsync(string title, Func<CancellationToken, Task<JsonNode>> run, int cancelMs = 0)
    {
        Clock.Restart();
        Console.WriteLine($"[client] === {title} ===");
        using var user = new CancellationTokenSource();                 // nguồn tín hiệu hủy của nút Hủy
        if (cancelMs > 0) user.CancelAfter(cancelMs);
        try { Log($"Thành công: {Khung.JsonText(await run(user.Token))}"); }
        catch (OperationCanceledException) { Log("Người dùng đã hủy (Cancelled) – không thử lại"); }
        catch (NetError e) { Log($"Thất bại, không thử lại: {e.Code} (retry={e.Retry})"); }
        catch (TimeoutException e) { Log($"Thất bại sau lần thử cuối: {e.Message}"); }
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args.Take(2).ToArray(), "127.0.0.1", 9100);
        int bad = args.Length > 2 ? int.Parse(args[2]) : 9101;
        await ScenarioAsync("Kịch bản 1: hạn chót 1000 ms, thử lại tối đa 5 lần (base 200, cap 5000)",
            ct => RetryAsync(MakeOp(host, port, new JsonObject { ["op"] = "lookup", ["term"] = "TCP" }, 1000), 5, 200, 5000, ct));
        await ScenarioAsync("Kịch bản 2: server xử lý 3000 ms, hạn chót 5000 ms, người dùng bấm Hủy sau 500 ms",
            ct => RetryAsync(MakeOp(host, port, new JsonObject { ["op"] = "lookup", ["term"] = "UDP", ["delay_ms"] = 3000 }, 5000), 5, 200, 5000, ct),
            cancelMs: 500);
        await ScenarioAsync($"Kịch bản 3: cổng {bad} không có server (REFUSED)",
            ct => RetryAsync(MakeOp(host, bad, new JsonObject { ["op"] = "lookup", ["term"] = "TCP" }, 1000), 5, 200, 5000, ct));
        return 0;
    }
}
