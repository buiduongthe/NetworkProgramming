using System.Diagnostics;
using System.Net.Sockets;
using System.Text;

namespace Bai6_3;

// Nhiều client echo đồng thời để kiểm tra server reactor.
// Mỗi client (một luồng) dựa trên Mã giả 3.2 nhưng nhận bằng RECV_EXACT (Mã giả 3.4): gửi một dòng chào,
// nhận lại đúng số byte; sau đó gửi một khối lớn ở luồng phụ, đồng thời nhận lại đủ khối và so khớp.
// Cuối cùng gửi thêm 200 000 byte rồi đóng chiều gửi và đọc đến hết: server phải gửi nốt rồi mới đóng.
// Khối lớn hơn MAX_BUF của server nên kiểm tra được việc gửi một phần và áp lực ngược.
static class EchoClients
{
    static readonly object PrintLock = new();

    static void Log(string msg)
    {
        lock (PrintLock) Console.WriteLine(msg);
    }

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9000);
        int n = NetUtil.IntArg(args, 2, 3);
        int size = NetUtil.IntArg(args, 3, 1048576);
        Console.WriteLine($"[client] Mở {n} kết nối đồng thời, mỗi kết nối gửi thêm {size} byte");
        var clock = Stopwatch.StartNew();
        var threads = Enumerable.Range(1, n).Select(i => new Thread(() => OneClient(i, host, port, size))).ToList();
        threads.ForEach(t => t.Start());
        threads.ForEach(t => t.Join());
        Console.WriteLine($"[client] Hoàn tất sau {clock.ElapsedMilliseconds} ms");
        return 0;
    }

    const int Tail = 200000;             // khối cuối gửi trước khi đóng chiều gửi (lớn hơn MAX_BUF của server)

    static void OneClient(int i, string host, int port, int size)
    {
        Socket s;
        try { s = NetUtil.SafeConnect(host, port, 3000); }
        catch (IOException e) { Log($"[client {i}] Lỗi: {e.Message}"); return; }
        s.ReceiveTimeout = 10000;
        s.SendTimeout = 10000;
        try
        {
            byte[] hello = Encoding.UTF8.GetBytes($"Xin chào reactor từ client {i}");
            s.Send(hello);                                               // Send của socket chặn gửi hết
            byte[] back = NetUtil.RecvExact(s, hello.Length);
            Log($"[client {i}] Nhận lại: '{Encoding.UTF8.GetString(back)}'");
            byte[] block = new byte[size];
            Random.Shared.NextBytes(block);
            var sender = new Thread(() => s.Send(block));                // gửi song song với nhận
            sender.Start();
            byte[] got = NetUtil.RecvExact(s, size);
            sender.Join();
            Log($"[client {i}] Gửi {size} byte, nhận lại {got.Length} byte, khớp: {(got.AsSpan().SequenceEqual(block) ? "có" : "không")}");
            byte[] tail = new byte[Tail];                                // phần 3: gửi khối cuối rồi đóng chiều gửi
            Random.Shared.NextBytes(tail);
            sender = new Thread(() => { s.Send(tail); s.Shutdown(SocketShutdown.Send); });   // SHUTDOWN(s, WRITE)
            sender.Start();
            var rest = new List<byte>();
            var buf = new byte[65536];
            int k;
            while ((k = s.Receive(buf)) > 0) rest.AddRange(buf.AsSpan(0, k).ToArray());     // đọc đến FIN của server
            sender.Join();
            Log($"[client {i}] Gửi {Tail} byte rồi SHUTDOWN(WRITE), nhận lại {rest.Count} byte trước FIN của server, khớp: {(rest.SequenceEqual(tail) ? "có" : "không")}");
        }
        catch (Exception e) when (e is SocketException || e is IOException)
        {
            Log($"[client {i}] Lỗi: {e.Message}");
        }
        finally
        {
            s.Close();
        }
    }
}
