using System.Collections.Concurrent;
using System.Diagnostics;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai6_7;

// Mã giả 6.9 – Bộ sinh tải vòng kín đo thông lượng và phân vị độ trễ (Mục 6.5.1).
// Kế thừa Mã giả 5.2: khởi động N tác vụ rồi WHEN_ALL (Task.WhenAll); khung JSON bất đồng bộ của Mã giả 5.1.
static class LoadTest
{
    // PERCENTILE theo hạng gần nhất: giá trị mà p% số mẫu không vượt quá
    static double Percentile(List<double> values, double p)
    {
        if (values.Count == 0) return 0;
        values.Sort();
        return values[Math.Max(0, (int)Math.Ceiling(p / 100 * values.Count) - 1)];
    }

    static async Task<(int Ok, int Errors, double Rps, double P50, double P95, double P99)> Run(
        string host, int port, int nClients, int nRequests)
    {
        var lat = new ConcurrentBag<double>();                           // dòng 2: CONCURRENT_LIST()
        int errors = 0;                                                  // ATOMIC_COUNTER(0), tăng bằng Interlocked
        var req = new JsonObject { ["op"] = "lookup", ["term"] = "TCP" };   // dòng 3

        async Task OneClient()                                           // dòng 4–20
        {
            using var s = new Socket(SocketType.Stream, ProtocolType.Tcp);
            try
            {
                using (var cts = new CancellationTokenSource(3000))      // SAFE_CONNECT_ASYNC, 3000 ms
                    await s.ConnectAsync(host, port, cts.Token);
                await using var stream = new NetworkStream(s);
                for (int j = 1; j <= nRequests; j++)
                {
                    long t0 = Stopwatch.GetTimestamp();                  // NOW_PRECISE()
                    await AsyncFrame.SendJsonAsync(stream, req, CancellationToken.None);
                    var r = await AsyncFrame.RecvJsonAsync(stream, CancellationToken.None);
                    if ((int?)r["status"] != 200) { Interlocked.Increment(ref errors); break; }   // dòng 12: ví dụ 503
                    lat.Add(Stopwatch.GetElapsedTime(t0).TotalMilliseconds);
                }
            }
            catch (Exception e) when (e is SocketException || e is IOException || e is JsonException || e is OperationCanceledException)
            {                                                            // dòng 15–16
                Interlocked.Increment(ref errors);                       // ghi nhận, không dừng cả bài
            }
        }                                                                // 'using' đóng socket (CLOSE)

        var clock = Stopwatch.StartNew();
        await Task.WhenAll(Enumerable.Range(0, nClients).Select(_ => OneClient()));   // WHEN_ALL
        double t = clock.Elapsed.TotalSeconds;
        var list = lat.ToList();
        return (list.Count, errors, list.Count / t, Percentile(list, 50), Percentile(list, 95), Percentile(list, 99));
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9000);
        int nRequests = NetUtil.IntArg(args, 2, 20);
        int[] levels = (args.Length > 3 ? args[3] : "1,10,100").Split(',').Select(int.Parse).ToArray();
        Console.WriteLine($"[load] Server {host}:{port}, {nRequests} yêu cầu/client, vòng kín");
        Console.WriteLine($"[load] {"client",6} {"OK",6} {"lỗi",5} {"yc/giây",8} {"p50",7} {"p95",7} {"p99",7}");
        foreach (int n in levels)
        {
            var r = await Run(host, port, n, nRequests);
            Console.WriteLine($"[load] {n,6} {r.Ok,6} {r.Errors,5} {r.Rps,8:F1} {r.P50,7:F1} {r.P95,7:F1} {r.P99,7:F1}");
        }
        Console.WriteLine("[load] OK: số yêu cầu thành công; lỗi: số client gặp lỗi (bị từ chối, mất kết nối, mã ≠ 200)");
        Console.WriteLine("[load] p50, p95, p99: phân vị độ trễ, tính bằng ms");
        return 0;
    }
}
