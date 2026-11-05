using System.Diagnostics;
using System.Globalization;
using System.Net.Sockets;
using System.Text;

namespace BaiMR6_1;

// Client thử cho server tắt an toàn: gửi lần lượt các yêu cầu "LAM <ms>", in phản hồi; sau đó tiếp tục giữ
// kết nối và in mọi dòng server gửi (BYE, HUY) cho tới khi server đóng kết nối.
static class Khach
{
    public static int Run(string[] a)
    {
        string name = a[0], host = a[1];
        int port = int.Parse(a[2]);
        var jobs = a.Skip(3).ToArray();
        string tag = $"[khach {name}]";
        var s = new Socket(SocketType.Stream, ProtocolType.Tcp);
        try
        {
            using var cts = new CancellationTokenSource(3000);
            s.ConnectAsync(host, port, cts.Token).AsTask().GetAwaiter().GetResult();
        }
        catch (SocketException e)
        {
            string why = e.SocketErrorCode == SocketError.ConnectionRefused ? "Connection refused" : e.Message;
            Console.WriteLine($"{tag} Không kết nối được: {why}");
            return 1;
        }
        s.ReceiveTimeout = 10_000;
        var sw = Stopwatch.StartNew();
        string T() => sw.Elapsed.TotalSeconds.ToString("F1", CultureInfo.InvariantCulture);
        using (s)
        using (var stream = new NetworkStream(s))
        using (var reader = new StreamReader(stream, Encoding.UTF8))
        {
            foreach (var ms in jobs)
            {
                s.Send(Encoding.UTF8.GetBytes($"LAM {ms}\n"));
                Console.WriteLine($"{tag} Gửi LAM {ms}");
                string? reply = reader.ReadLine();
                if (reply == null)
                {
                    Console.WriteLine($"{tag} Server đóng kết nối khi chưa trả lời");
                    return 0;
                }
                Console.WriteLine($"{tag} t={T()}s nhận: {reply.Trim()}");
            }
            string? line;
            while ((line = reader.ReadLine()) != null)                // giữ kết nối, chờ thông báo của server
                Console.WriteLine($"{tag} t={T()}s nhận: {line.Trim()}");
            Console.WriteLine($"{tag} t={T()}s server đóng kết nối");
        }
        return 0;
    }
}
