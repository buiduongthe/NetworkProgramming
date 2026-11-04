using System.Net.Sockets;
using System.Text;

namespace Bai7_1;

// Đối tác đóng kết nối giữa chừng một thông điệp
sealed class ConnectionClosedException(string message) : IOException(message);

// Mã giả 3.5 – STRUCT Reader { sock, pending }: giữ phần byte nhận dư giữa các lần đọc
sealed class Reader(Socket sock)
{
    public Socket Sock { get; } = sock;
    byte[] pending = [];

    // Mã giả 3.5 – READ_LINE: trả về dòng (bỏ CRLF) hoặc null khi đối tác đã đóng
    public string? ReadLine(int maxLen)
    {
        while (true)
        {
            int i = Array.IndexOf(pending, (byte)'\n');
            if (i > maxLen) throw new IOException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0)
            {
                int len = (i > 0 && pending[i - 1] == '\r') ? i - 1 : i;   // STRIP_CR
                string line = Encoding.UTF8.GetString(pending, 0, len);
                pending = pending[(i + 1)..];
                return line;
            }
            if (pending.Length > maxLen) throw new IOException("Dòng quá dài");
            var buf = new byte[4096];
            int n = Sock.Receive(buf);
            if (n == 0) return null;
            pending = [.. pending, .. buf.AsSpan(0, n)];
        }
    }

    // READ_EXACT: lấy phần dư pending trước, thiếu thì RECV_EXACT (Mã giả 3.4)
    public byte[] ReadExact(int n)
    {
        var data = new byte[n];
        int have = Math.Min(n, pending.Length);
        Array.Copy(pending, data, have);
        pending = pending[have..];
        while (have < n)
        {
            int k = Sock.Receive(data, have, n - have, SocketFlags.None);
            if (k == 0) throw new IOException($"Kết nối đóng khi mới nhận {have}/{n} byte");
            have += k;
        }
        return data;
    }

    // READ_UNTIL_CLOSE: đọc đến khi server đóng kết nối
    public byte[] ReadUntilClose()
    {
        var ms = new MemoryStream();
        ms.Write(pending);
        pending = [];
        var buf = new byte[4096];
        int n;
        while ((n = Sock.Receive(buf)) > 0) ms.Write(buf, 0, n);
        return ms.ToArray();
    }
}

static class NetUtil
{
    // Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối
    public static Socket SafeConnect(string host, int port, int timeoutMs)
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
