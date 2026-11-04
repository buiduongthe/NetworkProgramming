using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.Json;

namespace Bai6_2;

// Mã giả 6.3 – Server bất đồng bộ: mỗi kết nối một tác vụ (Mục 6.2.3).
// Kế thừa Mã giả 6.1 (SOCKET, SETOPT, BIND, LISTEN); lời gọi chặn → AWAIT …Async (Mã giả 5.1).
static class AsyncServer
{
    static int _tasks;                                                   // số tác vụ phiên đang chạy

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // như Mã giả 6.1
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(128);
        Console.WriteLine($"[server] Server bất đồng bộ đang lắng nghe tại {host}:{port}");
        while (true)
        {
            Socket conn = await srv.AcceptAsync();                       // AWAIT ACCEPT_ASYNC(srv)
            string addr = conn.RemoteEndPoint!.ToString()!;
            int n = Interlocked.Increment(ref _tasks);
            _ = ServeAsync(conn, addr);                                  // RUN_BACKGROUND: không chờ
            int threads = Process.GetCurrentProcess().Threads.Count;
            Console.WriteLine($"[server] Nhận {addr} (tác vụ: {n}, luồng: {threads})");
        }
    }

    // SERVE_ASYNC: vòng yêu cầu–phản hồi; khi chờ ở await, tác vụ không chiếm luồng nào
    static async Task ServeAsync(Socket conn, string addr)
    {
        string reason;
        try
        {
            await using var stream = new NetworkStream(conn, ownsSocket: false);
            while (true)
            {
                using var cts = new CancellationTokenSource(300000);     // WITH_TIMEOUT(300000, …)
                var req = await AsyncFrame.RecvJsonAsync(stream, cts.Token);
                await AsyncFrame.SendJsonAsync(stream, LookupService.ProcessRequest(req), CancellationToken.None);
            }
        }
        catch (ConnectionClosedException e)
        {
            reason = e.Received == 0 ? "client đóng kết nối" : e.Message;
        }
        catch (OperationCanceledException)                               // Timeout
        {
            reason = "quá thời gian chờ nhận";
        }
        catch (Exception e) when (e is ProtocolException || e is JsonException)
        {
            reason = $"lỗi giao thức: {e.Message}";
        }
        catch (Exception e) when (e is IOException || e is SocketException)   // NetError
        {
            reason = $"lỗi mạng: {e.Message}";
        }
        finally
        {
            conn.Close();                                                // CLOSE(conn)
            Interlocked.Decrement(ref _tasks);
        }
        Console.WriteLine($"[server] Kết thúc {addr}: {reason}");
    }
}
