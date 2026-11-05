using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR5_2;

// Máy chủ thử có độ trễ giả lập (chỉ chạy cục bộ).
// Giao thức dòng văn bản trên TCP: client gửi "GET <k>\n", máy chủ chờ <trễ> mili giây (giả lập thời gian
// xử lý/đường truyền) rồi trả "OK <k>\n". Máy chủ dùng async/await nên phục vụ được nhiều kết nối cùng lúc
// mà không cần một luồng cho mỗi kết nối. Sau mỗi "đợt" yêu cầu (máy chủ rỗi 300 ms), in số yêu cầu và số
// kết nối đồng thời lớn nhất của đợt – nhờ đó thấy được mỗi mô hình phía client gửi đồng thời bao nhiêu.
static class MayChuTre
{
    static int active, peak, count, total, burst;
    static readonly object khoa = new();       // các continuation chạy trên nhiều luồng của ThreadPool

    static void Log(string text) => Console.WriteLine($"[server] {text}");

    static async Task ReportWhenIdle()
    {
        await Task.Delay(300);
        lock (khoa)
        {
            if (active == 0 && count > 0)
            {
                burst++;
                Log($"đợt {burst,2}: {count,2} yêu cầu, đồng thời tối đa {peak,2}");
                count = peak = 0;
            }
        }
    }

    static async Task Handle(Socket conn, int delayMs)
    {
        lock (khoa)
        {
            active++; count++; total++;
            peak = Math.Max(peak, active);
        }
        try
        {
            using var stream = new NetworkStream(conn, ownsSocket: true);
            using var reader = new StreamReader(stream, Encoding.ASCII);
            string line = (await reader.ReadLineAsync() ?? "").Trim();
            await Task.Delay(delayMs);                                    // giả lập độ trễ
            string k = line.StartsWith("GET ") ? line[4..] : line;
            await stream.WriteAsync(Encoding.ASCII.GetBytes($"OK {k}\n"));
        }
        catch (IOException) { }
        finally
        {
            bool idle;
            lock (khoa) idle = --active == 0;
            if (idle) _ = ReportWhenIdle();
        }
    }

    public static int Run(string[] a)
    {
        string host = a[0];
        int port = int.Parse(a[1]);
        int delayMs = a.Length > 2 ? int.Parse(a[2]) : 200;
        var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(128);
        Log($"lắng nghe {host}:{port}, mỗi yêu cầu trễ {delayMs} ms");
        var stop = new TaskCompletionSource();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.TrySetResult(); };
        _ = Task.Run(async () =>
        {
            while (true)
            {
                Socket conn = await srv.AcceptAsync();
                _ = Handle(conn, delayMs);                                // không chờ: phục vụ song song
            }
        });
        stop.Task.Wait();
        Log($"dừng; đã phục vụ {total} yêu cầu");
        return 0;
    }
}
