using System.Net;
using System.Net.Sockets;

namespace Bai5_2;

// Mở các cổng minh họa trên 127.0.0.1 để chạy thử chương trình quét.
// - Cổng "mở": lắng nghe bình thường, chấp nhận rồi đóng ngay mọi kết nối.
// - Cổng "bị lọc" (mô phỏng): lắng nghe với backlog 1 và không bao giờ Accept; chương trình tự lấp đầy
//   hàng đợi kết nối, sau đó Linux bỏ qua gói SYN mới nên phía quét hết thời gian chờ như khi gặp tường lửa.
//   (Trên Windows hàng đợi đầy thường trả RST, tức cổng đóng.)
static class DemoPorts
{
    static readonly List<Socket> Keep = new();

    static Socket Listen(int port, int backlog)
    {
        var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Loopback, port));
        srv.Listen(backlog);
        Keep.Add(srv);
        return srv;
    }

    static void OpenPort(int port)
    {
        var srv = Listen(port, 16);
        new Thread(() => { while (true) srv.Accept().Dispose(); }) { IsBackground = true }.Start();
    }

    static void FilteredPort(int port)
    {
        Listen(port, 1);                                                // hàng đợi rất ngắn, không bao giờ Accept
        for (int i = 0; i < 8; i++)                                     // lấp đầy hàng đợi
        {
            var c = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
            using var cts = new CancellationTokenSource(300);
            try { c.ConnectAsync(IPAddress.Loopback, port, cts.Token).AsTask().GetAwaiter().GetResult(); Keep.Add(c); }
            catch (OperationCanceledException) { c.Dispose(); break; }  // SYN bị bỏ qua: hàng đợi đã đầy
        }
    }

    static List<int> ParseList(string s) => s.Split(',', StringSplitOptions.RemoveEmptyEntries).Select(int.Parse).ToList();

    public static int Run(string[] args)
    {
        var opened = args.Length > 0 ? ParseList(args[0]) : new List<int> { 9003, 9011, 9017 };
        var filtered = args.Length > 1 ? ParseList(args[1]) : new List<int> { 9005, 9008, 9013, 9019 };
        opened.ForEach(OpenPort);
        filtered.ForEach(FilteredPort);
        Console.WriteLine($"[demo] Cổng mở: {string.Join(", ", opened)}");
        Console.WriteLine($"[demo] Cổng bị lọc (mô phỏng): {string.Join(", ", filtered)}");
        Console.WriteLine("[demo] Nhấn Ctrl+C để dừng");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[demo] Dừng");
        Thread.Sleep(Timeout.Infinite);
        return 0;
    }
}
