using System.Net;
using System.Net.Sockets;

namespace BaiMR3_1;

// Bộ chuyển tiếp cổng TCP (TCP port forwarder).
// Mỗi client nối vào cổng nghe được ghép với một kết nối mới tới máy đích; hai luồng
// bơm dữ liệu theo hai chiều. Khi một chiều gặp EOF, bộ chuyển tiếp gọi Shutdown(Send)
// trên socket bên kia để báo "hết dữ liệu" (chuyển tiếp gói FIN), chứ không đóng hẳn,
// vì chiều ngược lại có thể vẫn đang truyền.
// Đây là nguyên lý của "ssh -L", proxy TCP và cân bằng tải lớp 4.
static class ChuyenTiep
{
    static int counter;

    public static int Run(string[] a)
    {
        string lhost = a[0], thost = a[2];
        int lport = int.Parse(a[1]), tport = int.Parse(a[3]);
        var srv = new TcpListener(IPAddress.Parse(lhost), lport);
        srv.Start();
        Console.WriteLine($"[chuyen-tiep] {lhost}:{lport} -> {thost}:{tport}");
        // Ctrl+C (SIGINT): dừng Accept bằng cách đóng socket nghe, rồi in "Dừng" như bản Python
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; srv.Stop(); };
        try
        {
            while (true)
            {
                Socket client = srv.AcceptSocket();
                new Thread(() => Serve(client, thost, tport)) { IsBackground = true }.Start();
            }
        }
        catch (SocketException) { }
        catch (ObjectDisposedException) { }
        Console.WriteLine("[chuyen-tiep] Dừng");
        return 0;
    }

    /// <summary>Chép dữ liệu src -> dst tới khi src hết dữ liệu, rồi báo EOF cho dst.</summary>
    static long Pump(Socket src, Socket dst, string label)
    {
        long total = 0;
        var buf = new byte[65536];
        try
        {
            while (true)
            {
                int n = src.Receive(buf);
                if (n == 0) break;
                int sent = 0;
                while (sent < n) sent += dst.Send(buf, sent, n - sent, SocketFlags.None);   // SEND_ALL
                total += n;
            }
        }
        catch (SocketException e)
        {
            Console.WriteLine($"[chuyen-tiep] {label}: lỗi {e.Message}");
        }
        finally
        {
            try { dst.Shutdown(SocketShutdown.Send); }   // chuyển tiếp FIN, không đóng hẳn socket
            catch (SocketException) { }
        }
        return total;
    }

    static void Serve(Socket client, string thost, int tport)
    {
        int cid = Interlocked.Increment(ref counter);
        var addr = (IPEndPoint)client.RemoteEndPoint!;
        Socket upstream;
        try
        {
            upstream = new Socket(SocketType.Stream, ProtocolType.Tcp);
            using var cts = new CancellationTokenSource(3000);          // thời gian chờ kết nối 3 s
            upstream.ConnectAsync(thost, tport, cts.Token).AsTask().GetAwaiter().GetResult();
        }
        catch (Exception e) when (e is SocketException || e is OperationCanceledException)
        {
            string why = e is SocketException se ? Errors.Text(se) : "hết thời gian chờ";
            Console.WriteLine($"[chuyen-tiep] #{cid} {addr.Address}:{addr.Port}: không nối được đích " +
                              $"{thost}:{tport} ({why})");
            client.Close();                                                // client nhận EOF ngay
            return;
        }
        Console.WriteLine($"[chuyen-tiep] #{cid} {addr.Address}:{addr.Port} <-> đích {thost}:{tport}");
        long up = 0, down = 0;
        var t1 = new Thread(() => up = Pump(client, upstream, $"#{cid} lên"));
        var t2 = new Thread(() => down = Pump(upstream, client, $"#{cid} xuống"));
        t1.Start(); t2.Start();
        t1.Join(); t2.Join();                                              // chờ cả hai chiều kết thúc
        client.Close(); upstream.Close();
        Console.WriteLine($"[chuyen-tiep] #{cid} xong: lên {up} byte, xuống {down} byte");
    }
}

static class Errors
{
    /// <summary>Thông điệp lỗi ngắn giống strerror của Python (thông điệp của .NET có thể kèm địa chỉ).</summary>
    public static string Text(SocketException e) => e.SocketErrorCode switch
    {
        SocketError.ConnectionRefused => "Connection refused",
        SocketError.ConnectionReset => "Connection reset by peer",
        SocketError.TimedOut => "Connection timed out",
        _ => e.Message,
    };
}
