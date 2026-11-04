using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai5_6;

// Đối tác đóng kết nối; Received = số byte của khung đã nhận được (Mã giả 3.4)
class ConnectionClosedException(int received) : Exception($"Kết nối đã đóng (đã nhận {received} byte)")
{
    public int Received { get; } = received;
}

// Vi phạm giao thức, ví dụ khung quá lớn
class ProtocolException(string message) : Exception(message);

// Lỗi mạng đã phân loại (Mã giả 2.2): Code, Retry; RetryAfter (ms) do server đề nghị (Mã giả 5.8)
class NetError(string code, bool retry, Exception? cause = null, int? retryAfter = null)
    : Exception(cause == null ? code : $"{code}: {cause.Message}", cause)
{
    public string Code { get; } = code;
    public bool Retry { get; } = retry;
    public int? RetryAfter { get; } = retryAfter;
}

// Mã giả 5.1 – các hàm gửi, nhận khung JSON bất đồng bộ; kế thừa Mã giả 3.3, 3.4, 3.6
static class Khung
{
    public const int MaxFrame = 1048576;                                // 1 MiB
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static string JsonText(JsonNode obj) => obj.ToJsonString(Opt); // giữ nguyên chữ có dấu

    public static byte[] MakeFrame(JsonNode obj)                        // UINT32_BE(LENGTH(p)) + p
    {
        byte[] p = Encoding.UTF8.GetBytes(JsonText(obj));               // ENCODE(JSON_ENCODE(obj), "UTF-8")
        var frame = new byte[4 + p.Length];
        BinaryPrimitives.WriteUInt32BigEndian(frame, (uint)p.Length);
        p.CopyTo(frame, 4);
        return frame;
    }

    // Mã giả 5.1, dòng 1–11 (NetworkStream.ReadExactlyAsync làm sẵn việc này; viết tường minh để đối chiếu)
    public static async Task<byte[]> RecvExactAsync(Stream s, int n, CancellationToken ct)
    {
        var buf = new byte[n];
        int got = 0;
        while (got < n)
        {
            int k = await s.ReadAsync(buf.AsMemory(got, n - got), ct);  // AWAIT RECV_ASYNC(s, n - LENGTH(buf), ct)
            if (k == 0) throw new ConnectionClosedException(got);       // đối tác đã đóng kết nối
            got += k;
        }
        return buf;
    }

    // Mã giả 5.1, dòng 12–21
    public static async Task<JsonNode> RecvJsonAsync(Stream s, CancellationToken ct)
    {
        uint n = BinaryPrimitives.ReadUInt32BigEndian(await RecvExactAsync(s, 4, ct)); // FROM_UINT32_BE
        if (n > MaxFrame) throw new ProtocolException("Khung quá lớn");
        byte[] data;
        try
        {
            data = await RecvExactAsync(s, (int)n, ct);
        }
        catch (ConnectionClosedException e)                             // đã nhận xong tiêu đề
        {
            throw new ConnectionClosedException(4 + e.Received);
        }
        return JsonNode.Parse(Encoding.UTF8.GetString(data))!;           // JSON_DECODE(DECODE(data, "UTF-8"))
    }

    // Mã giả 5.1, dòng 22–25: WriteAsync của NetworkStream gửi hết dữ liệu (SEND_ALL_ASYNC)
    public static async Task SendJsonAsync(Stream s, JsonNode obj, CancellationToken ct)
        => await s.WriteAsync(MakeFrame(obj), ct);
}

static class Net
{
    // Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định
    public static (string Host, int Port) ParseEndpoint(string[] args, string defaultHost, int defaultPort)
    {
        string host = defaultHost;
        string port = Environment.GetEnvironmentVariable("APP_PORT") ?? defaultPort.ToString();
        if (args.Length >= 1) host = args[0];
        if (args.Length >= 2) port = args[1];
        if (!int.TryParse(port, out int p)) throw new ArgumentException($"Cổng phải là số nguyên: {port}");
        if (p < 1 || p > 65535) throw new ArgumentException($"Cổng không hợp lệ: {p}");
        return (host, p);
    }

    // SAFE_CONNECT_ASYNC = Mã giả 2.2 + AWAIT: thử lần lượt các địa chỉ, có thời gian chờ, phân loại lỗi
    public static async Task<Socket> SafeConnectAsync(string host, int port, int timeoutMs, CancellationToken ct = default)
    {
        IPAddress[] addrs;
        try { addrs = await Dns.GetHostAddressesAsync(host, ct); }          // RESOLVE
        catch (SocketException e) { throw new NetError("NOT_FOUND", false, e); }
        int refused = 0;
        Exception? last = null;
        foreach (var ip in addrs)
        {
            var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
            cts.CancelAfter(timeoutMs);                                     // CONNECT_TIMEOUT
            try
            {
                await s.ConnectAsync(ip, port, cts.Token);
                return s;                                                   // thành công: trả socket
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused)
            {
                refused++; last = e; s.Dispose();
            }
            catch (Exception e) when (e is SocketException || (e is OperationCanceledException && !ct.IsCancellationRequested))
            {
                last = e; s.Dispose();                                      // hết giờ, mạng không tới được
            }
            catch (OperationCanceledException)                              // nơi gọi hủy: đóng socket rồi báo tiếp
            {
                s.Dispose(); throw;
            }
        }
        if (refused > 0 && refused == addrs.Length) throw new NetError("REFUSED", false, last);
        throw new NetError("UNREACHABLE", true, last);
    }
}
