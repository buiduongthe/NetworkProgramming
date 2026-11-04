using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai6_1;

// Mã giả 6.2 – Server dùng nhóm luồng và hàng đợi có giới hạn (Mục 6.2.2).
// Kế thừa Mã giả 6.1: dùng lại nguyên Serve; k luồng thợ dùng chung một hàng đợi; hàng đợi đầy → 503.
static class PoolServer
{
    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        int k = NetUtil.IntArg(args, 2, 8);
        int queueSize = NetUtil.IntArg(args, 3, 16);
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
        Console.WriteLine($"[server] Nhóm {k} luồng thợ, hàng đợi {queueSize} chỗ");
        while (true)
        {
            Socket conn = srv.Accept();                                  // ACCEPT
            string addr = conn.RemoteEndPoint!.ToString()!;
            Console.WriteLine($"[server] Nhận {addr}");
            if (!q.TryAdd((conn, addr)))                                 // q.try_put: hàng đợi đầy
            {
                Console.WriteLine($"[server] Hàng đợi đầy, trả 503 cho {addr}");
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
            Console.WriteLine($"[server] Luồng thợ {id} phục vụ {addr}");
            ThreadedServer.Serve(conn, addr, new AtomicCounter(1));      // bộ đếm tạm
        }
    }
}
