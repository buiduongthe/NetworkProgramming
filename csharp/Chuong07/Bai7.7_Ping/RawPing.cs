using System.Buffers.Binary;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;

namespace Bai7_7;

// Mã giả 7.8 – INET_CHECKSUM (RFC 1071) và PING trên socket RAW (cần quyền quản trị)
static class RawPing
{
    public static ushort InetChecksum(byte[] b)                         // dòng 1–11
    {
        if (b.Length % 2 == 1) b = [.. b, 0];
        uint sum = 0;
        for (int i = 0; i < b.Length; i += 2) sum += BinaryPrimitives.ReadUInt16BigEndian(b.AsSpan(i, 2));
        while (sum > 0xFFFF) sum = (sum & 0xFFFF) + (sum >> 16);       // cộng phần tràn
        return (ushort)(~sum & 0xFFFF);
    }

    // dòng 13–35: trả về (địa chỉ, RTT ms, TTL); hết thời gian chờ ném SocketException(TimedOut)
    static (string Addr, double RttMs, int Ttl) Ping(string host, int seq, int timeoutMs)
    {
        IPAddress dst = Dns.GetHostAddresses(host).First(a => a.AddressFamily == AddressFamily.InterNetwork);
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Raw, ProtocolType.Icmp);   // cần quyền quản trị
        ushort id = (ushort)(Environment.ProcessId & 0xFFFF);
        byte[] p = new byte[8 + 32];
        p[0] = 8; p[1] = 0;                                             // kiểu 8 (Echo Request), mã 0
        BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(4), id);
        BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(6), (ushort)seq);
        RandomNumberGenerator.Fill(p.AsSpan(8));                        // phần dữ liệu 32 byte
        BinaryPrimitives.WriteUInt16BigEndian(p.AsSpan(2), InetChecksum(p));   // điền tổng kiểm tra
        if (seq == 1)
            Console.WriteLine($"[ping] Tiêu đề ICMP gửi đi: {BitConverter.ToString(p, 0, 8).Replace('-', ' ')} (kiểu, mã, checksum, id, seq)");
        var sw = Stopwatch.StartNew();                                  // NOW_PRECISE()
        s.SendTo(p, new IPEndPoint(dst, 0));
        long deadline = Environment.TickCount64 + timeoutMs;            // dòng 21: hạn chót chung
        var buf = new byte[1500];
        while (true)                                                    // bỏ qua gói ICMP khác
        {
            s.ReceiveTimeout = (int)Math.Max(1, deadline - Environment.TickCount64);   // dòng 23: phần còn lại
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            int n = s.ReceiveFrom(buf, ref from);                       // RAW: có cả tiêu đề IP
            int ihl = (buf[0] & 0x0F) * 4;                              // độ dài tiêu đề IPv4
            var m = buf.AsSpan(ihl, n - ihl);
            if (m[0] == 0 && BinaryPrimitives.ReadUInt16BigEndian(m[4..]) == id
                          && BinaryPrimitives.ReadUInt16BigEndian(m[6..]) == seq)
                return (((IPEndPoint)from).Address.ToString(), sw.Elapsed.TotalMilliseconds, buf[8]);   // buf[8]: TTL
            Console.WriteLine($"[ping]   bỏ qua gói ICMP kiểu {m[0]} (không phải Echo Reply của tiến trình này)");
        }
    }

    public static int Run(string host, int count, int timeoutMs)
    {
        Console.WriteLine($"[ping] PING {host}: {count} gói Echo Request, dữ liệu 32 byte, chờ tối đa {timeoutMs} ms");
        var rtts = new List<double>();
        for (int seq = 1; seq <= count; seq++)
        {
            try
            {
                var (addr, rtt, ttl) = Ping(host, seq, timeoutMs);
                rtts.Add(rtt);
                Console.WriteLine($"[ping] Trả lời từ {addr}: seq={seq} ttl={ttl} thời gian={rtt:F3} ms");
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)
            {
                Console.WriteLine($"[ping] seq={seq}: hết thời gian chờ");
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.AccessDenied)
            {
                Console.WriteLine("[ping] Socket RAW cần quyền quản trị (sudo hoặc Run as administrator)");
                return 1;
            }
            if (seq < count) Thread.Sleep(1000);
        }
        string avg = rtts.Count > 0 ? $"{rtts.Average():F3} ms" : "không có";
        Console.WriteLine($"[ping] Đã gửi {count}, nhận {rtts.Count}, mất {100 * (count - rtts.Count) / count}%; RTT trung bình {avg}");
        return 0;
    }
}
