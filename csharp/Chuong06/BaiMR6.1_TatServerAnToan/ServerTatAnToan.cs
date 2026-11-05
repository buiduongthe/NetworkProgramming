using System.Collections.Concurrent;
using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;

namespace BaiMR6_1;

// Tắt server an toàn (graceful shutdown) với async/await và CancellationToken.
// Giao thức dòng văn bản: client gửi "LAM <ms>", server mô phỏng công việc dài <ms> mili giây rồi trả "XONG <ms>".
// Khi nhận SIGTERM (lệnh kill, trình quản lý dịch vụ, Docker/Kubernetes) hoặc SIGINT (Ctrl+C):
//   1. Dừng socket nghe: ngừng nhận kết nối mới (client mới bị từ chối).
//   2. Kết nối đang rỗi: gửi "BYE" rồi đóng ngay.
//   3. Kết nối đang làm dở: cho làm xong yêu cầu hiện tại, trả kết quả, gửi "BYE", đóng.
//   4. Quá hạn chờ (--han-cho-ms): hủy các yêu cầu còn dở, báo "HUY" cho client.
// Hai CancellationTokenSource: "stopping" (bắt đầu tắt – đánh thức kết nối đang rỗi) và
// "hard" (hết hạn chờ – hủy công việc đang làm).
// PID được ghi vào server.pid để có thể gửi tín hiệu bằng: kill -TERM $(cat server.pid)
static class ServerTatAnToan
{
    static int counter, xong, huy;
    static readonly CancellationTokenSource stopping = new();   // được hủy khi bắt đầu tắt
    static readonly CancellationTokenSource hard = new();       // được hủy khi hết hạn chờ
    static readonly ConcurrentDictionary<int, Task> connections = new();   // mã kết nối -> tác vụ đang phục vụ

    static void Log(string text) => Console.WriteLine($"[server] {text}");

    static async Task Handle(TcpClient client, int cid)
    {
        int? job = null;
        var stream = client.GetStream();
        var reader = new StreamReader(stream, Encoding.UTF8);
        var writer = new StreamWriter(stream, new UTF8Encoding(false)) { AutoFlush = true, NewLine = "\n" };
        Log($"#{cid} kết nối từ cổng {((IPEndPoint)client.Client.RemoteEndPoint!).Port}");
        try
        {
            while (true)
            {
                string? line;
                try
                {
                    line = await reader.ReadLineAsync(stopping.Token);    // chờ dòng lệnh HOẶC tín hiệu dừng
                }
                catch (OperationCanceledException)                        // rỗi đúng lúc server tắt
                {
                    await writer.WriteLineAsync("BYE server đang tắt");
                    Log($"#{cid} đang rỗi -> gửi BYE, đóng");
                    break;
                }
                if (line == null)                                         // client đã đóng
                {
                    Log($"#{cid} client đóng kết nối");
                    break;
                }
                var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
                if (parts.Length != 2 || parts[0] != "LAM" || !parts[1].All(char.IsAsciiDigit))
                {
                    await writer.WriteLineAsync("LOI lệnh không hợp lệ");
                    continue;
                }
                job = int.Parse(parts[1]);
                Log($"#{cid} bắt đầu LAM {job}");
                await Task.Delay(job.Value, hard.Token);                  // công việc dài (có thể bị hủy)
                await writer.WriteLineAsync($"XONG {job}");
                Interlocked.Increment(ref xong);
                if (stopping.IsCancellationRequested)                     // làm xong việc dở thì mới chào
                {
                    await writer.WriteLineAsync("BYE server đang tắt");
                    Log($"#{cid} XONG {job} -> gửi BYE, đóng");
                    break;
                }
                Log($"#{cid} XONG {job}");
                job = null;
            }
        }
        catch (OperationCanceledException)                                // quá hạn chờ tắt
        {
            Interlocked.Increment(ref huy);
            Log($"#{cid} quá hạn chờ -> hủy LAM {job}");
            try { await writer.WriteLineAsync("HUY yêu cầu bị hủy vì server tắt"); } catch (IOException) { }
        }
        catch (IOException) { }                                           // client mất kết nối
        finally
        {
            client.Close();
            connections.TryRemove(cid, out _);
        }
    }

    public static async Task<int> Run(string[] args)
    {
        var pos = new List<string>();
        int hanCho = 2000;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--han-cho-ms") hanCho = int.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        string host = pos.Count > 0 ? pos[0] : "127.0.0.1";
        int port = pos.Count > 1 ? int.Parse(pos[1]) : 9000;
        var listener = new TcpListener(IPAddress.Parse(host), port);
        listener.Start();

        // Bắt SIGTERM, SIGINT: ctx.Cancel = true để .NET không kết thúc tiến trình ngay (ta tự tắt có trật tự)
        var received = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        using var regTerm = PosixSignalRegistration.Create(PosixSignal.SIGTERM, ctx => { ctx.Cancel = true; received.TrySetResult("SIGTERM"); });
        using var regInt = PosixSignalRegistration.Create(PosixSignal.SIGINT, ctx => { ctx.Cancel = true; received.TrySetResult("SIGINT"); });
        File.WriteAllText("server.pid", Environment.ProcessId.ToString());
        Log($"Lắng nghe tại {host}:{port}, PID ghi vào server.pid");

        var acceptLoop = Task.Run(async () =>
        {
            try
            {
                while (true)
                {
                    var c = await listener.AcceptTcpClientAsync();
                    int cid = Interlocked.Increment(ref counter);
                    connections[cid] = Handle(c, cid);
                }
            }
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }   // socket nghe đã dừng
        });

        string sig = await received.Task;
        var t0 = Stopwatch.StartNew();
        listener.Stop();                                                  // 1. ngừng nhận kết nối mới
        Log($"Nhận {sig}: ngừng nhận kết nối mới");
        var pending = connections.Values.ToArray();
        Log($"Chờ tối đa {hanCho} ms cho {pending.Length} kết nối");
        stopping.Cancel();                                                // 2, 3. báo mọi kết nối: server đang tắt
        var all = Task.WhenAll(pending);
        if (await Task.WhenAny(all, Task.Delay(hanCho)) != all)
        {
            int still = pending.Count(t => !t.IsCompleted);               // 4. quá hạn: hủy phần còn lại
            Log($"Hết hạn chờ: hủy {still} kết nối");
            hard.Cancel();
            await all;
        }
        File.Delete("server.pid");
        Log($"Đã tắt sau {t0.Elapsed.TotalSeconds.ToString("F1", CultureInfo.InvariantCulture)} s: xong {xong} yêu cầu, hủy {huy}");
        return 0;
    }
}
