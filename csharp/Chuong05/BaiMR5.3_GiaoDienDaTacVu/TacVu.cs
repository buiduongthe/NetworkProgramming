using System.Net.Sockets;
using System.Text;

namespace BaiMR5_3;

// Các tác vụ mạng và bộ giới hạn đồng thời – KHÔNG dùng WinForms (biên dịch, kiểm thử được độc lập).
// Mọi tác vụ là hàm async, được giao diện gọi qua Task.Run nên chạy trên thread pool, không chiếm luồng giao diện.
// Tiến trình được báo qua IProgress<(phần_trăm, chữ)>: đối tượng Progress<T> tạo trên luồng giao diện
// tự đưa mỗi lần báo về luồng giao diện (qua SynchronizationContext), nên tác vụ không chạm vào điều khiển.
// Hủy theo kiểu hợp tác: CancellationToken được truyền xuống mọi lời gọi mạng.

/// <summary>Giới hạn số tác vụ chạy cùng lúc; giới hạn đổi được khi đang chạy (thanh trượt).
/// Chỉ được gọi trên luồng giao diện (mọi continuation của tác vụ đều về đó) nên không cần khóa.</summary>
class Limiter(int n)
{
    int limit = n, running;
    readonly List<TaskCompletionSource> waiters = [];

    public int Running => running;

    public void Set(int n)
    {
        limit = n;
        Wake();
    }

    /// <summary>Chờ tới lượt; trả false nếu bị hủy trong lúc chờ.</summary>
    public async Task<bool> AcquireAsync(CancellationToken ct)
    {
        while (running >= limit)
        {
            if (ct.IsCancellationRequested) return false;
            var tcs = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
            waiters.Add(tcs);
            using (ct.Register(() => tcs.TrySetResult()))
                await tcs.Task;                         // được đánh thức khi có chỗ trống, đổi giới hạn hoặc hủy
            waiters.Remove(tcs);
        }
        if (ct.IsCancellationRequested) return false;
        running++;
        return true;
    }

    public void Release()
    {
        running--;
        Wake();
    }

    void Wake()
    {
        foreach (var w in waiters.ToList()) w.TrySetResult();
    }
}

static class TacVu
{
    // máy chủ thử cục bộ: không đi qua proxy hệ thống; hạn chót do token của từng tác vụ quyết định
    static readonly HttpClient Http = new(new SocketsHttpHandler { UseProxy = false }) { Timeout = Timeout.InfiniteTimeSpan };

    /// <summary>Tải /tep?kb=..&tre=.. qua HTTP, đọc từng phần và báo tiến trình theo Content-Length.</summary>
    public static async Task<string> DownloadAsync(string host, int port, int kb, int delay,
                                                   IProgress<(int, string)> progress, CancellationToken ct)
    {
        using var resp = await Http.GetAsync($"http://{host}:{port}/tep?kb={kb}&tre={delay}",
                                             HttpCompletionOption.ResponseHeadersRead, ct);   // chỉ chờ tiêu đề
        resp.EnsureSuccessStatusCode();
        long total = resp.Content.Headers.ContentLength ?? 0, got = 0;
        await using var body = await resp.Content.ReadAsStreamAsync(ct);
        var buf = new byte[16 * 1024];
        while (got < total)
        {
            int n = await body.ReadAsync(buf, ct);      // đọc phần đã có, tối đa 16 KB
            if (n == 0) throw new IOException("máy chủ đóng kết nối sớm");
            got += n;
            progress.Report(((int)(got * 100 / total), $"{got / 1024}/{total / 1024} KB"));
        }
        return $"xong {total / 1024} KB";
    }

    /// <summary>CÁCH SAI (chỉ để minh họa): cùng việc tải nhưng dùng lời gọi CHẶN – gọi trên luồng giao diện
    /// thì cả cửa sổ đứng cho tới khi xong.</summary>
    public static long DownloadBlocking(string host, int port, int kb, int delay)
    {
        using var req = new HttpRequestMessage(HttpMethod.Get, $"http://{host}:{port}/tep?kb={kb}&tre={delay}");
        using var resp = Http.Send(req, HttpCompletionOption.ResponseHeadersRead);   // Send: phiên bản đồng bộ
        resp.EnsureSuccessStatusCode();
        using var body = resp.Content.ReadAsStream();
        var buf = new byte[16 * 1024];
        long got = 0;
        int n;
        while ((n = body.Read(buf, 0, buf.Length)) > 0) got += n;
        return got;
    }

    /// <summary>Kiểm tra lần lượt từng cổng TCP (như Bài 1.2): kết nối được = mở.</summary>
    public static async Task<string> CheckPortsAsync(string host, int[] ports,
                                                     IProgress<(int, string)> progress, CancellationToken ct)
    {
        int opened = 0;
        for (int i = 0; i < ports.Length; i++)
        {
            ct.ThrowIfCancellationRequested();
            bool ok;
            using (var s = new Socket(SocketType.Stream, ProtocolType.Tcp))
            using (var cts = CancellationTokenSource.CreateLinkedTokenSource(ct))
            {
                cts.CancelAfter(300);                   // thời gian chờ kết nối 300 ms
                try
                {
                    await s.ConnectAsync(host, ports[i], cts.Token);
                    ok = true;
                }
                catch (SocketException) { ok = false; }
                catch (OperationCanceledException) when (!ct.IsCancellationRequested) { ok = false; }
            }
            if (ok) opened++;
            await Task.Delay(300, ct);                  // quét chậm cho dễ quan sát
            progress.Report(((i + 1) * 100 / ports.Length, $"cổng {ports[i]} {(ok ? "mở" : "đóng")}"));
        }
        return $"{opened} mở, {ports.Length - opened} đóng";
    }

    /// <summary>Tra cứu tên qua máy chủ "DNS" giả lập trên UDP, có thời gian chờ 1 s.</summary>
    public static async Task<string> LookupNamesAsync(string host, int port, string[] names,
                                                      IProgress<(int, string)> progress, CancellationToken ct)
    {
        int found = 0;
        using var udp = new UdpClient();
        udp.Connect(host, port);
        for (int i = 0; i < names.Length; i++)
        {
            ct.ThrowIfCancellationRequested();
            await udp.SendAsync(Encoding.UTF8.GetBytes($"A {names[i]}"), ct);
            string answer;
            using (var cts = CancellationTokenSource.CreateLinkedTokenSource(ct))
            {
                cts.CancelAfter(1000);
                try
                {
                    var r = await udp.ReceiveAsync(cts.Token);
                    answer = Encoding.UTF8.GetString(r.Buffer);
                }
                catch (OperationCanceledException) when (!ct.IsCancellationRequested)
                {
                    answer = $"{names[i]} (hết thời gian chờ)";
                }
            }
            if (!answer.Contains("NXDOMAIN")) found++;
            progress.Report(((i + 1) * 100 / names.Length, answer));
        }
        return $"{found}/{names.Length} tên có địa chỉ";
    }
}
