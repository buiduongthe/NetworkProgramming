using System.Net;
using System.Net.Sockets;

namespace Bai6_1;

/// <summary>ATOMIC_COUNTER: tăng, giảm bằng Interlocked nên không bị xen ngang giữa các luồng.</summary>
class AtomicCounter(int value)
{
    int _value = value;
    public int Value => Volatile.Read(ref _value);
    public int Increment() => Interlocked.Increment(ref _value);
    public int Decrement() => Interlocked.Decrement(ref _value);
}

// Mã giả 6.1 – Server đa luồng: mỗi kết nối một luồng (Mục 6.2.1).
// Kế thừa Mã giả 3.1 (khung server); nghiệp vụ là HANDLE_LOOKUP của Mã giả 3.7.
static class ThreadedServer
{
    const int MaxClients = 500;                                          // CONST MAX_CLIENTS ← 500

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        int maxClients = NetUtil.IntArg(args, 2, MaxClients);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // REUSEADDR
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(128);                                                                              // LISTEN 128
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        Console.WriteLine($"[server] Mỗi kết nối một luồng, MAX_CLIENTS = {maxClients}");
        var active = new AtomicCounter(0);
        while (true)
        {
            Socket conn = srv.Accept();                                  // ACCEPT
            string addr = conn.RemoteEndPoint!.ToString()!;
            if (active.Value >= maxClients)                              // MỚI: giới hạn
            {
                Console.WriteLine($"[server] Từ chối {addr}: đã đủ {maxClients} kết nối");
                conn.Close();
                continue;
            }
            int n = active.Increment();
            Console.WriteLine($"[server] Nhận {addr}, tạo luồng mới (đang phục vụ: {n})");
            new Thread(() => Serve(conn, addr, active)) { IsBackground = true }.Start();   // SPAWN_THREAD
        }
    }

    // SERVE – khối TRY của Mã giả 3.1, chạy trong luồng riêng của mỗi kết nối
    public static void Serve(Socket conn, string addr, AtomicCounter active)
    {
        try
        {
            conn.ReceiveTimeout = 300000;                                // đóng kết nối nhàn rỗi sau 5 phút
            LookupService.HandleLookup(conn);
        }
        catch (Exception e) when (e is SocketException || e is IOException)   // NetError, Timeout, ProtocolError
        {
            Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");
        }
        finally
        {
            conn.Close();
            active.Decrement();                                          // luôn giảm bộ đếm, kể cả khi lỗi
            Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
        }
    }
}
