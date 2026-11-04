using System.Globalization;
using System.Net.Security;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Security.Cryptography.X509Certificates;
using System.Text;

namespace Bai7_6;

sealed class SmtpException(int code, List<string> text) : Exception($"{code} {string.Join(" ", text)}")
{
    public int Code { get; } = code;
}

// Mã giả 3.5 – Reader { sock, pending } và READ_LINE (trên Stream: NetworkStream hoặc SslStream)
sealed class Reader(Stream s)
{
    public Stream Sock { get; } = s;
    byte[] pending = [];

    public string? ReadLine(int maxLen)                                 // null khi server đã đóng
    {
        while (true)
        {
            int i = Array.IndexOf(pending, (byte)'\n');
            if (i > maxLen) throw new InvalidDataException("Dòng quá dài");   // Mã giả 3.5 dòng 5: có LF nhưng dòng quá dài
            if (i >= 0)
            {
                int len = (i > 0 && pending[i - 1] == '\r') ? i - 1 : i;
                string line = Encoding.UTF8.GetString(pending, 0, len);
                pending = pending[(i + 1)..];
                return line;
            }
            if (pending.Length > maxLen) throw new InvalidDataException("Dòng quá dài");
            var buf = new byte[4096];
            int n = Sock.Read(buf);
            if (n == 0) return null;
            pending = [.. pending, .. buf.AsSpan(0, n)];
        }
    }
}

// Mã giả 7.7 – SMTP_REPLY, CMD, SEND_MAIL, DOT_STUFF
static class SmtpSession
{
    static (int, List<string>) SmtpReply(Reader r)                      // dòng 1–11: phản hồi nhiều dòng
    {
        var text = new List<string>();
        while (true)
        {
            string? l = r.ReadLine(1024);
            if (l == null || l.Length < 3)                              // dòng 5: server đóng hoặc dòng quá ngắn
                throw new InvalidDataException($"Phản hồi SMTP không hợp lệ: '{l}'");   // ProtocolError
            Console.WriteLine($"[client] S: {l}");
            text.Add(l.Length > 4 ? l[4..] : "");                       // dòng 6: rỗng nếu chỉ có 3 chữ số
            if (l.Length == 3 || l[3] == ' ') return (int.Parse(l[..3]), text);   // dòng 7: dòng cuối
        }
    }

    static void Cmd(Reader r, string line, int expect, string? shown = null)   // dòng 12–16
    {
        Console.WriteLine($"[client] C: {shown ?? line}");
        r.Sock.Write(Encoding.UTF8.GetBytes(line + "\r\n"));            // Stream.Write gửi hết (SEND_ALL)
        var (code, text) = SmtpReply(r);
        if (code / 100 != expect) throw new SmtpException(code, text);
    }

    static string DotStuff(string msg) =>                               // thêm '.' cho dòng bắt đầu bằng '.'
        string.Join("\r\n", msg.Split('\n').Select(l => l.StartsWith('.') ? "." + l : l));

    public static void SendMail(string host, int port, string caFile, string user, string secret,
                                string from, string to, string msg)       // dòng 17–31
    {
        using var tcp = new TcpClient();
        if (!tcp.ConnectAsync(host, port).Wait(5000)) throw new IOException("Hết thời gian kết nối");   // SAFE_CONNECT
        tcp.ReceiveTimeout = 5000;
        var r = new Reader(tcp.GetStream());
        var (code, _) = SmtpReply(r);                                   // 220: lời chào của server
        if (code != 220) throw new SmtpException(code, ["Không phải lời chào 220"]);
        Cmd(r, "EHLO client.example", 2);
        Cmd(r, "STARTTLS", 2);                                          // 220: sẵn sàng TLS
        var ssl = new SslStream(tcp.GetStream());                       // TLS_WRAP(s, CLIENT, host)
        ssl.AuthenticateAsClient(TlsOptions(host, caFile));             // kiểm tra chứng chỉ và tên máy
        r = new Reader(ssl);                                            // Reader mới: bỏ mọi byte nhận trước TLS
        string ver = ssl.SslProtocol == SslProtocols.Tls13 ? "TLSv1.3" : ssl.SslProtocol.ToString();
        Console.WriteLine($"[client] Đã thiết lập {ver}, chứng chỉ server hợp lệ cho '{host}'");
        Cmd(r, "EHLO client.example", 2);                               // chào lại sau khi có TLS
        string token = Convert.ToBase64String(Encoding.UTF8.GetBytes("\0" + user + "\0" + secret));
        Cmd(r, "AUTH PLAIN " + token, 2, shown: "AUTH PLAIN ****");      // không in thông tin xác thực
        Cmd(r, $"MAIL FROM:<{from}>", 2);
        Cmd(r, $"RCPT TO:<{to}>", 2);
        Cmd(r, "DATA", 3);                                              // 354: bắt đầu nội dung
        string body = DotStuff(msg);
        Cmd(r, body + "\r\n.", 2, shown: $"<nội dung thư {Encoding.UTF8.GetByteCount(body)} byte> + dòng '.'");
        Cmd(r, "QUIT", 2);
    }                                                                   // 'using' đóng kết nối (CLOSE)

    // Tin cậy chứng chỉ tự ký trong tệp CA (không tắt kiểm tra); "-" dùng kho chứng chỉ của hệ điều hành
    static SslClientAuthenticationOptions TlsOptions(string host, string caFile)
    {
        var o = new SslClientAuthenticationOptions { TargetHost = host };
        if (caFile != "-")
        {
            var policy = new X509ChainPolicy { TrustMode = X509ChainTrustMode.CustomRootTrust,
                                               RevocationMode = X509RevocationMode.NoCheck };
            policy.CustomTrustStore.Add(X509Certificate2.CreateFromPem(File.ReadAllText(caFile)));
            o.CertificateChainPolicy = policy;
        }
        return o;
    }

    public static string BuildMessage(string from, string to)          // tiêu đề thư, dòng trống, thân UTF-8
    {
        string subject = "=?utf-8?B?" + Convert.ToBase64String(Encoding.UTF8.GetBytes("Thư thử nghiệm Bài 7.6")) + "?=";
        string date = DateTime.UtcNow.ToString("ddd, dd MMM yyyy HH:mm:ss '+0000'", CultureInfo.InvariantCulture);
        return string.Join("\n",
            $"From: <{from}>", $"To: <{to}>", $"Subject: {subject}", $"Date: {date}",
            $"Message-ID: <{DateTimeOffset.UtcNow.ToUnixTimeSeconds()}@client.example>",
            "MIME-Version: 1.0", "Content-Type: text/plain; charset=utf-8", "Content-Transfer-Encoding: 8bit",
            "", "Xin chào, đây là thư gửi bằng phiên SMTP tự cài đặt.",
            ".Dòng này bắt đầu bằng dấu chấm (được DOT_STUFF thêm một dấu chấm).", "Hết thư.");
    }
}
