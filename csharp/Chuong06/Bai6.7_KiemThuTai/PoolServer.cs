using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai6_7;

// Bản sao server nhóm luồng của Bài 6.1 (Mã giả 6.2, Mục 6.2.2) để kiểm thử tải: k luồng thợ, hàng đợi
// có giới hạn, hàng đợi đầy → 503. Không ghi nhật ký từng kết nối; tham số delay_ms là độ trễ giả lập.
static class PoolServer
{
    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        int k = NetUtil.IntArg(args, 2, 8);
        int queueSize = NetUtil.IntArg(args, 3, 16);
        LookupService.DelayMs = NetUtil.IntArg(args, 4, 50);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // như Mã giả 6.1
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(128);
        var q = new BlockingCollection<(Socket Conn, string Addr)>(queueSize);   // BLOCKING_QUEUE(queue_size)
        for (int i = 1; i <= k; i++)                                     // FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
        {
            int id = i;
            new Thread(() => Worker(id, q)) { IsBackground = true }.Start();
        }
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        Console.WriteLine($"[server] Nhóm {k} luồng thợ, hàng đợi {queueSize} chỗ, độ trễ {LookupService.DelayMs} ms");
        while (true)
        {
            Socket conn = srv.Accept();                                  // ACCEPT
            string addr = conn.RemoteEndPoint!.ToString()!;
            if (!q.TryAdd((conn, addr)))                                 // q.try_put: hàng đợi đầy
            {
                try { NetUtil.SendJson(conn, new JsonObject { ["status"] = 503, ["error"] = "Server bận" }); }
                catch (SocketException) { }                              // client đã đi: bỏ qua
                conn.Close();
            }
        }
    }

    // WORKER: lấy kết nối khỏi hàng đợi rồi phục vụ bằng Serve
    static void Worker(int id, BlockingCollection<(Socket Conn, string Addr)> q)
    {
        while (true)
        {
            var (conn, addr) = q.Take();                                 // chờ đến khi có việc
            ThreadedServer.Serve(conn, addr, new AtomicCounter(1));      // bộ đếm tạm
        }
    }
}
