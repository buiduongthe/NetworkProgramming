using System.Net.Sockets;
using System.Text;

namespace Bai3_1;

// Mã giả 3.2 – TCP Echo Client (phiên bản đầu, chưa định khung)
// Lưu ý: Receive() giả định nhận đủ phản hồi trong một lần; khắc phục ở Bài 3.2 (định khung)
static class EchoClient
{
    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args, "127.0.0.1", 9000);
        using Socket s = SafeConnect(host, port, 3000);
        s.ReceiveTimeout = 5000;                                        // RECV_TIMEOUT 5000 ms
        Console.WriteLine($"[client] Đã kết nối {host}:{port}. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
        var buf = new byte[4096];
        string? line;
        while ((line = Console.ReadLine()) != null)                     // READ_CONSOLE()
        {
            byte[] data = Encoding.UTF8.GetBytes(line);                 // ENCODE(line, "UTF-8")
            NetUtil.SendAll(s, data, data.Length);                      // SEND_ALL
            int n = s.Receive(buf);                                     // giả định nhận đủ một lần
            if (n == 0) break;
            Console.WriteLine($"[client] Gửi: '{line}' -> Nhận: '{Encoding.UTF8.GetString(buf, 0, n)}'");
        }
        s.Shutdown(SocketShutdown.Send);                                // SHUTDOWN(WRITE): báo hết dữ liệu
        Console.WriteLine("[client] Đã đóng kết nối");
        return 0;
    }

    // Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối
    static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        Exception? last = null;
        foreach (var ip in System.Net.Dns.GetHostAddresses(host))
        {
            var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                using var cts = new CancellationTokenSource(timeoutMs);
                s.ConnectAsync(ip, port, cts.Token).AsTask().GetAwaiter().GetResult();
                return s;
            }
            catch (Exception e) when (e is SocketException || e is OperationCanceledException)
            {
                last = e;
                s.Dispose();
            }
        }
        throw new IOException($"Không kết nối được {host}:{port}: {last?.Message}");
    }
}
