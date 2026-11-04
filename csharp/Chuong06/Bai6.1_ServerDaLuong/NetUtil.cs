using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Unicode;

namespace Bai6_1;

// Các thủ tục kế thừa dùng chung của Bài 6.1 (chép gọn từ Chương 2, 3)

/// <summary>Đối tác đóng kết nối; Received = số byte của khung đang nhận dở.</summary>
class ConnectionClosedException(int received) : IOException($"Kết nối bị đóng sau {received} byte")
{
    public int Received { get; } = received;
}

/// <summary>Vi phạm định dạng khung (ví dụ khung quá lớn).</summary>
class ProtocolException(string message) : IOException(message);

static class NetUtil
{
    public const int MaxFrame = 1048576;                                  // Mã giả 3.6: 1 MiB
    static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.Create(UnicodeRanges.All) };

    // Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định
    public static (string Host, int Port) ParseEndpoint(string[] args, string defaultHost, int defaultPort)
    {
        string host = Environment.GetEnvironmentVariable("APP_HOST") ?? defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p)) throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535) throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }

    /// <summary>Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.</summary>
    public static int IntArg(string[] args, int i, int def) => args.Length > i ? int.Parse(args[i]) : def;

    // Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối
    public static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        Exception? last = null;
        foreach (var ip in Dns.GetHostAddresses(host))
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

    // Mã giả 3.4 – RECV_EXACT: gọi Receive lặp lại đến khi đủ n byte
    public static byte[] RecvExact(Socket s, int n)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = s.Receive(buf, got, n - got, SocketFlags.None);
            if (k == 0) throw new ConnectionClosedException(got);
            got += k;
        }
        return buf;
    }

    // Mã giả 3.6 – SEND_JSON: đối tượng → JSON → UTF-8 → khung 4 byte độ dài (big-endian); Send gửi hết (SEND_ALL)
    public static void SendJson(Socket s, JsonObject obj)
    {
        byte[] payload = Encoding.UTF8.GetBytes(obj.ToJsonString(Opts));
        var frame = new byte[4 + payload.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)payload.Length);   // UINT32_BE
        payload.CopyTo(frame, 4);
        int sent = 0;
        while (sent < frame.Length) sent += s.Send(frame, sent, frame.Length - sent, SocketFlags.None);
    }

    // Mã giả 3.6 – RECV_JSON (gồm RECV_FRAME). JSON sai gây JsonException
    public static JsonObject RecvJson(Socket s)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(RecvExact(s, 4));      // FROM_UINT32_BE
        if (n > MaxFrame) throw new ProtocolException($"Khung quá lớn: {n}");
        byte[] data;
        try { data = RecvExact(s, (int)n); }
        catch (ConnectionClosedException e) { throw new ConnectionClosedException(4 + e.Received); }
        return JsonNode.Parse(Encoding.UTF8.GetString(data)) as JsonObject
               ?? throw new JsonException("Thông điệp không phải đối tượng JSON");
    }
}
