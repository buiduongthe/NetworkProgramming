using System.Diagnostics;
using System.Net;
using System.Net.Sockets;

namespace Bai5_2;

// Mã giả 5.2 – SCAN_PORTS, CHECK_ONE; kế thừa Mã giả 1.2 (PORT_CHECK): CONNECT thay bằng AWAIT ConnectAsync.
// Quét cùng một dải cổng hai lần: tuần tự (max_parallel = 1) và đồng thời (max_parallel = k).
// Chỉ chạy trên máy thuộc quyền quản lý hoặc mạng thí nghiệm cô lập (Mục 1.2.5).
static class ScanAsync
{
    // Mã giả 1.2 + AWAIT: kết quả OPEN, CLOSED, FILTERED hoặc UNRESOLVED
    static async Task<string> PortCheckAsync(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs;
        try { addrs = await Dns.GetHostAddressesAsync(host); }        // RESOLVE_HOST
        catch (SocketException) { return "UNRESOLVED"; }
        if (addrs.Length == 0) return "UNRESOLVED";
        string state = "FILTERED";                                      // chưa địa chỉ nào phản hồi
        foreach (var a in addrs)
        {
            using var s = new Socket(a.AddressFamily, SocketType.Stream, ProtocolType.Tcp); // using = FINALLY CLOSE
            using var cts = new CancellationTokenSource(timeoutMs);    // CONNECT_TIMEOUT
            try
            {
                await s.ConnectAsync(a, port, cts.Token);               // AWAIT CONNECT_ASYNC
                return "OPEN";                                          // bắt tay ba bước thành công
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused)
            {
                state = "CLOSED";                                       // nhận gói RST
            }
            catch (Exception e) when (e is OperationCanceledException || e is SocketException)
            {
                continue;                                               // hết giờ, không tới được: địa chỉ tiếp theo
            }
        }
        return state;
    }

    static async Task<(int Port, string State)> CheckOneAsync(string host, int p, SemaphoreSlim gate) // dòng 9–16
    {
        await gate.WaitAsync();                                         // ACQUIRE_ASYNC(gate)
        try
        {
            return (p, await PortCheckAsync(host, p, 1000));
        }
        finally
        {
            gate.Release();                                             // RELEASE(gate)
        }
    }

    static async Task<(int Port, string State)[]> ScanPortsAsync(string host, IEnumerable<int> ports, int maxParallel) // dòng 1–8
    {
        using var gate = new SemaphoreSlim(maxParallel);                // giới hạn số kết nối
        var tasks = new List<Task<(int, string)>>();
        foreach (int p in ports)
            tasks.Add(CheckOneAsync(host, p, gate));                    // chưa chờ
        return await Task.WhenAll(tasks);                               // WHEN_ALL: chờ tất cả hoàn tất
    }

    public static async Task<int> RunAsync(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int first = a.Length > 1 ? int.Parse(a[1]) : 9000;
        int last = a.Length > 2 ? int.Parse(a[2]) : 9019;
        int k = a.Length > 3 ? int.Parse(a[3]) : 10;
        var ports = Enumerable.Range(first, last - first + 1).ToList();
        Console.WriteLine($"[scan] Quét {ports.Count} cổng {first}–{last} trên {host}, thời gian chờ 1000 ms");
        var sw = Stopwatch.StartNew();
        var seq = await ScanPortsAsync(host, ports, 1);
        double tSeq = sw.Elapsed.TotalSeconds;
        sw.Restart();
        var par = await ScanPortsAsync(host, ports, k);
        double tPar = sw.Elapsed.TotalSeconds;
        foreach (var (p, state) in par)
            if (state != "CLOSED") Console.WriteLine($"[scan]   cổng {p}: {state}");   // chỉ in cổng không đóng
        int Count(string s) => par.Count(x => x.State == s);
        Console.WriteLine($"[scan] Tổng hợp: OPEN={Count("OPEN")}, CLOSED={Count("CLOSED")}, FILTERED={Count("FILTERED")}");
        Console.WriteLine($"[scan] Hai lần quét cho cùng kết quả: {(seq.SequenceEqual(par) ? "có" : "không")}");
        Console.WriteLine($"[scan] Tuần tự   (max_parallel=1):  {tSeq:F2} s");
        Console.WriteLine($"[scan] Đồng thời (max_parallel={k}): {tPar:F2} s");
        return 0;
    }
}
