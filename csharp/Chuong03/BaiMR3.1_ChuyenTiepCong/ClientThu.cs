using System.Net.Sockets;
using System.Text;

namespace BaiMR3_1;

// Client thử: gửi dữ liệu, đóng chiều gửi, đọc phản hồi tới EOF, so khớp với bản chữ hoa.
static class ClientThu
{
    public static int Run(string[] a)
    {
        string host = a[0], text = a[2];
        int port = int.Parse(a[1]);
        int repeat = a.Length > 3 ? int.Parse(a[3]) : 1;
        byte[] data = Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat(text, repeat)));
        var reply = new MemoryStream();
        using (var s = new Socket(SocketType.Stream, ProtocolType.Tcp))
        {
            s.ReceiveTimeout = s.SendTimeout = 5000;
            s.Connect(host, port);
            try
            {
                int sent = 0;
                while (sent < data.Length) sent += s.Send(data, sent, data.Length - sent, SocketFlags.None);
                s.Shutdown(SocketShutdown.Send);               // báo hết dữ liệu: server mới bắt đầu trả lời
                var buf = new byte[65536];
                int n;
                while ((n = s.Receive(buf)) > 0)               // 0 = EOF: phía bên kia đã đóng chiều gửi
                    reply.Write(buf, 0, n);
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset)
            {   // bên kia đóng khi còn dữ liệu chưa đọc -> RST
                Console.WriteLine("[client] Kết nối bị đặt lại (RST) trước khi có phản hồi");
            }
        }
        byte[] got = reply.ToArray();
        Console.WriteLine($"[client] Gửi {data.Length} byte, nhận {got.Length} byte");
        if (got.Length == 0)
        {
            Console.WriteLine("[client] Kết nối bị đóng mà không có dữ liệu trả về");
            return 1;
        }
        string str = Encoding.UTF8.GetString(got);
        string preview = str[..Math.Min(40, str.Length)];
        byte[] expect = Encoding.UTF8.GetBytes(string.Concat(Enumerable.Repeat(text.ToUpperInvariant(), repeat)));
        bool ok = got.AsSpan().SequenceEqual(expect);
        Console.WriteLine($"[client] Phản hồi: '{preview}'{(str.Length > 40 ? "..." : "")}");
        Console.WriteLine($"[client] So khớp với bản chữ hoa: {(ok ? "KHỚP" : "SAI")}");
        return 0;
    }
}
