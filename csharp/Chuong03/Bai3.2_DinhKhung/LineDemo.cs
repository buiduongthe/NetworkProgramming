using System.Net;
using System.Net.Sockets;
using System.Text;

namespace Bai3_2;

// Minh họa READ_LINE (Mã giả 3.5): luồng "gửi" chia dữ liệu thành các mảnh cắt ngang dòng (kể cả giữa
// hai byte của chữ "à"); luồng chính đọc bằng READ_LINE. Mảnh cuối 100 byte không có '\n' -> "Dòng quá dài"
static class LineDemo
{
    const int MaxLen = 64;
    static readonly byte[][] Pieces =
    [
        "HELLO\nWOR"u8.ToArray(),
        [.. "LD\r\nXin ch"u8, 0xC3],
        [0xA0, .. "o\n"u8],
        Enumerable.Repeat((byte)'A', 100).ToArray(),
    ];

    static string Show(byte[] data)                                     // ASCII in được giữ nguyên, còn lại \xNN
    {
        var sb = new StringBuilder();
        foreach (byte b in data)
            sb.Append(b switch
            {
                0x0A => "\\n",
                0x0D => "\\r",
                >= 0x20 and < 0x7F => ((char)b).ToString(),
                _ => $"\\x{b:x2}",
            });
        string t = sb.ToString();
        return t.Length <= 24 ? t : t[..21] + "...";
    }

    static void Sender(string host, int port)
    {
        using var s = NetUtil.SafeConnect(host, port, 3000);
        s.NoDelay = true;                                               // NODELAY: mỗi mảnh đi riêng
        foreach (var p in Pieces)
        {
            Console.WriteLine($"[gửi] {p.Length} byte: {Show(p)}");
            Framing.SendAll(s, p);
            Thread.Sleep(300);
        }
    }

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(1);
        Console.WriteLine($"[đọc] Đang lắng nghe tại {host}:{port}, max_len = {MaxLen}");
        var t = new Thread(() => Sender(host, port)) { IsBackground = true };
        t.Start();
        using Socket conn = srv.Accept();
        var r = new Reader(conn);                                       // Reader { sock, pending ← EMPTY_BYTES }
        try
        {
            while (true)
            {
                string? line = Framing.ReadLine(r, MaxLen);
                if (line == null) { Console.WriteLine("[đọc] Đối tác đã đóng kết nối"); break; }
                Console.WriteLine($"[đọc] READ_LINE -> '{line}' (pending còn {r.Pending.Length} byte)");
            }
        }
        catch (ProtocolError e)
        {
            Console.WriteLine($"[đọc] ProtocolError: {e.Message} (pending {r.Pending.Length} byte > {MaxLen})");
        }
        conn.Close();
        t.Join();
        return 0;
    }
}
