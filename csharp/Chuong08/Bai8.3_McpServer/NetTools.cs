using System.Buffers.Binary;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;

namespace Bai8_3;

// Lỗi khi thực thi công cụ (máy không phản hồi...): trả về mô hình với isError = true
class NetError(string message) : Exception(message);

// Thân ba công cụ mạng: Mã giả 1.1 RESOLVE_HOST, 1.2 PORT_CHECK, 7.8 PING (chép gọn).
// PRINT đổi thành LOG ra stderr vì mọi dòng in ra stdout chen vào luồng thông điệp MCP (Mục 8.3.5).
static class NetTools
{
    public static void Log(string text) => Console.Error.WriteLine("[mcp] " + text);   // chỉ ghi ra STDERR

    public static string FamilyName(AddressFamily f) =>
        f == AddressFamily.InterNetwork ? "INET" : f == AddressFamily.InterNetworkV6 ? "INET6" : f.ToString();

    // Mã giả 1.1 – RESOLVE_HOST(name): lỗi phân giải -> danh sách rỗng
    public static IPAddress[] ResolveHost(string name)
    {
        try
        {
            IPAddress[] addrs = Dns.GetHostAddresses(name);                 // RESOLVE (getaddrinfo)
            foreach (var a in addrs) Log($"  resolve: {FamilyName(a.AddressFamily)} {a}");
            return addrs;
        }
        catch (SocketException e)
        {
            Log($"  resolve: Không phân giải được {name} {e.SocketErrorCode}");
            return [];
        }
    }

    // Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms) -> OPEN | CLOSED | FILTERED | UNRESOLVED
    public static string PortCheck(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs = ResolveHost(host);
        if (addrs.Length == 0) return "UNRESOLVED";
        string state = "FILTERED";
        foreach (var a in addrs)
        {
            using var s = new Socket(a.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            try
            {
                if (s.ConnectAsync(a, port).Wait(timeoutMs)) return "OPEN";   // CONNECT có thời gian chờ
            }
            catch (AggregateException e) when (e.InnerException is SocketException { SocketErrorCode: SocketError.ConnectionRefused })
            {
                state = "CLOSED";                                            // nhận gói RST
            }
            catch (AggregateException)
            {
                // NetworkUnreachable, HostUnreachable: thử địa chỉ tiếp theo
            }
        }
        return state;
    }

    // Mã giả 7.8 – INET_CHECKSUM (RFC 1071)
    static ushort InetChecksum(byte[] b)
    {
        long sum = 0;
        for (int i = 0; i + 1 < b.Length; i += 2) sum += BinaryPrimitives.ReadUInt16BigEndian(b.AsSpan(i));
        if (b.Length % 2 == 1) sum += b[^1] << 8;
        while (sum > 0xFFFF) sum = (sum & 0xFFFF) + (sum >> 16);
        return (ushort)(~sum & 0xFFFF);
    }

    // Mã giả 7.8 – PING(host, seq, timeout_ms) bằng socket RAW (cần quyền quản trị) -> (địa chỉ, RTT ms, TTL)
    public static (string Addr, double RttMs, int Ttl) Ping(IPAddress dst, int seq, int timeoutMs)
    {
        Socket s;
        try { s = new Socket(AddressFamily.InterNetwork, SocketType.Raw, ProtocolType.Icmp); }
        catch (SocketException) { throw new NetError("Không mở được socket RAW: cần quyền quản trị"); }
        using (s)
        {
            int id = Environment.ProcessId & 0xFFFF;
            var p = new byte[8 + 32];
            p[0] = 8; p[1] = 0;                                               // kiểu 8 (Echo Request), mã 0
            BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(4), (ushort)id);
            BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(6), (ushort)seq);
            RandomNumberGenerator.Fill(p.AsSpan(8));                          // phần dữ liệu 32 byte
            BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(2), InetChecksum(p));
            var clock = Stopwatch.StartNew();
            s.SendTo(p, new IPEndPoint(dst, 0));
            long deadline = Environment.TickCount64 + timeoutMs;              // Mã giả 7.8 dòng 21: hạn chót chung
            var buf = new byte[1500];
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            while (true)                                                      // bỏ qua gói ICMP khác
            {
                s.ReceiveTimeout = (int)Math.Max(1, deadline - Environment.TickCount64);   // dòng 23: phần còn lại
                try { s.ReceiveFrom(buf, ref from); }                         // RAW: có cả tiêu đề IP
                catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)
                {
                    throw new TimeoutException($"Không nhận được Echo Reply trong {timeoutMs} ms");   // Timeout
                }
                int ihl = (buf[0] & 0x0F) * 4;
                if (buf[ihl] == 0 && BinaryPrimitives.ReadUInt16BigEndian(buf.AsSpan(ihl + 4)) == id
                    && BinaryPrimitives.ReadUInt16BigEndian(buf.AsSpan(ihl + 6)) == seq)
                    return (((IPEndPoint)from).Address.ToString(), clock.Elapsed.TotalMilliseconds, buf[8]);
            }
        }
    }
}
