using System.Diagnostics;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai6_1;

// Mở nhiều client tra cứu đồng thời để quan sát server đa luồng và nhóm luồng.
// Mỗi client chạy trong một luồng, thực hiện LOOKUP_CLIENT (Mã giả 3.8) rút gọn: gửi lookup "TCP",
// in phản hồi kèm thời điểm t, giữ kết nối hold_ms rồi gửi bye. Các client khởi động cách nhau 100 ms.
static class MultiClient
{
    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9000);
        int n = NetUtil.IntArg(args, 2, 5);
        int holdMs = NetUtil.IntArg(args, 3, 1000);
        Console.WriteLine($"[client] {n} client cách nhau 100 ms, giữ kết nối {holdMs} ms");
        var clock = Stopwatch.StartNew();
        var threads = new List<Thread>();
        for (int i = 1; i <= n; i++)
        {
            int id = i;
            var th = new Thread(() => OneClient(id, host, port, holdMs, clock));
            th.Start();
            threads.Add(th);
            Thread.Sleep(100);
        }
        foreach (var th in threads) th.Join();
        Console.WriteLine($"[client] Hoàn tất sau {clock.ElapsedMilliseconds} ms");
        return 0;
    }

    static void OneClient(int i, string host, int port, int holdMs, Stopwatch clock)
    {
        Socket s;
        try { s = NetUtil.SafeConnect(host, port, 3000); }
        catch (IOException e) { Console.WriteLine($"[client {i}] Lỗi: {e.Message}"); return; }
        s.ReceiveTimeout = 5000;                                         // RECV_TIMEOUT 5000 ms
        try
        {
            NetUtil.SendJson(s, new JsonObject { ["op"] = "lookup", ["term"] = "TCP" });
            JsonObject resp = NetUtil.RecvJson(s);
            int status = (int)resp["status"]!;
            if (status != 200)
            {
                Console.WriteLine($"[client {i}] t = {clock.ElapsedMilliseconds} ms: bị từ chối {status} – {resp["error"]}");
                return;
            }
            Console.WriteLine($"[client {i}] t = {clock.ElapsedMilliseconds} ms: 200 {resp["term"]} – {resp["text"]}");
            Thread.Sleep(holdMs);                                        // giữ kết nối
            NetUtil.SendJson(s, new JsonObject { ["op"] = "bye" });
        }
        catch (Exception e) when (e is ConnectionClosedException ||
                                  (e is SocketException se && se.SocketErrorCode == SocketError.ConnectionReset))
        {
            Console.WriteLine($"[client {i}] t = {clock.ElapsedMilliseconds} ms: server đóng kết nối ngay");
        }
        catch (Exception e) when (e is SocketException || e is IOException)
        {
            Console.WriteLine($"[client {i}] Lỗi: {e.Message}");
        }
        finally
        {
            s.Close();
        }
    }
}
