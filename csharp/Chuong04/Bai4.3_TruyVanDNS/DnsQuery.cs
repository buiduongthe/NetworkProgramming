using System.Buffers.Binary;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;

namespace Bai4_3;

class ProtocolError(string message) : Exception(message);

// Mã giả 4.5 – BUILD_QUERY, SKIP_NAME, DNS_QUERY (RFC 1035). Bổ sung so với mã giả (theo đoạn văn sau
// Mã giả 4.5): kiểm tra mọi chỉ số pos không vượt độ dài thông điệp và kiểm tra cờ TC.
static class DnsQuery
{
    static readonly Dictionary<string, ushort> QTYPES = new() { ["A"] = 1, ["AAAA"] = 28 };
    static readonly string[] RCODES = ["NOERROR", "FORMERR", "SERVFAIL", "NXDOMAIN", "NOTIMP", "REFUSED"];

    static string Hex(byte[] b, int n) => string.Join(" ", b.Take(n).Select(x => x.ToString("X2")));

    static ushort U16(byte[] m, int pos) => BinaryPrimitives.ReadUInt16BigEndian(m.AsSpan(pos, 2)); // FROM_UINT16_BE

    static byte[] BuildQuery(ushort id, string name, ushort qtype)                       // dòng 1–9
    {
        var q = new List<byte>();
        void U16BE(int v) { q.Add((byte)(v >> 8)); q.Add((byte)v); }                     // UINT16_BE
        U16BE(id); U16BE(0x0100);                                                        // dòng 2: RD = 1
        U16BE(1); q.AddRange(new byte[6]);                                               // dòng 3: QD=1, AN=NS=AR=0
        foreach (string label in name.TrimEnd('.').Split('.'))                           // dòng 4
        {
            byte[] b = System.Text.Encoding.ASCII.GetBytes(new IdnMapping().GetAscii(label)); // dòng 5: Punycode
            if (b.Length < 1 || b.Length > 63) throw new ArgumentException($"Nhãn không hợp lệ: '{label}'");
            q.Add((byte)b.Length); q.AddRange(b);                                        // dòng 6
        }
        q.Add(0); U16BE(qtype); U16BE(1);                                                // dòng 8: QCLASS = IN
        return q.ToArray();
    }

    // Kiểm tra chỉ số (bổ sung): phản hồi sai định dạng không được làm đọc ngoài thông điệp
    static void Need(int len, int end)
    {
        if (end > len) throw new ProtocolError("Phản hồi sai định dạng: vượt quá độ dài thông điệp");
    }

    static int SkipName(byte[] m, int len, int pos)                                      // dòng 10–17
    {
        while (true)
        {
            Need(len, pos + 1);
            int n = m[pos];
            if (n == 0) return pos + 1;                                                  // hết tên
            if ((n & 0xC0) == 0xC0) { Need(len, pos + 2); return pos + 2; }              // con trỏ nén
            if ((n & 0xC0) != 0) throw new ProtocolError("Phản hồi sai định dạng: nhãn kiểu lạ");
            pos += 1 + n;
        }
    }

    // server = (ip, cổng): IPEndPoint; ip quyết định họ địa chỉ của socket (IPv4 hoặc IPv6)
    static (int Rcode, List<string> Answers) Query(IPEndPoint server, string name, ushort qtype, string tname)
    {
        var m = new byte[4096];
        int len;
        EndPoint addr = new IPEndPoint(server.AddressFamily == AddressFamily.InterNetworkV6
                                       ? IPAddress.IPv6Any : IPAddress.Any, 0);
        ushort id = (ushort)RandomNumberGenerator.GetInt32(65536);                       // dòng 20: RANDOM_UINT16()
        byte[] q = BuildQuery(id, name, qtype);
        Console.WriteLine($"[client] Truy vấn {tname} cho {name} tới {server} (id=0x{id:X4}, {q.Length} byte)");
        Console.WriteLine($"[client] Truy vấn (hex): {Hex(q, q.Length)}");
        using (var s = new Socket(server.AddressFamily, SocketType.Dgram, ProtocolType.Udp))   // dòng 19: FAMILY(server.ip)
        {
            s.ReceiveTimeout = 2000;                                                     // dòng 20: RECV_TIMEOUT
            s.SendTo(q, server);                                                         // dòng 21
            len = s.ReceiveFrom(m, ref addr);                                            // dòng 22; CLOSE khi rời using
        }
        if (!addr.Equals(server)) throw new ProtocolError($"Phản hồi từ địa chỉ lạ {addr}");  // dòng 23
        Need(len, 12);
        if (U16(m, 0) != id) throw new ProtocolError("ID của phản hồi không khớp truy vấn");  // dòng 24
        int rcode = m[3] & 0x0F;                                                         // dòng 25: 3 = NXDOMAIN
        int ancount = U16(m, 6);                                                         // dòng 26
        Console.WriteLine($"[client] Phản hồi {len} byte từ {addr}, RCODE={rcode} ({(rcode < RCODES.Length ? RCODES[rcode] : "?")}), ANCOUNT={ancount}");
        Console.WriteLine($"[client] Phản hồi (hex): {Hex(m, len)}");
        if ((m[2] & 0x02) != 0)                                                          // bổ sung: cờ TC
            throw new ProtocolError("Phản hồi bị cắt (TC = 1): cần hỏi lại qua TCP cổng 53");
        int pos = SkipName(m, len, 12) + 4;                                              // dòng 27: bỏ QTYPE, QCLASS
        var answers = new List<string>();
        for (int i = 0; i < ancount; i++)                                                // dòng 29
        {
            pos = SkipName(m, len, pos);                                                 // dòng 30
            Need(len, pos + 10);
            int type = U16(m, pos);                                                      // dòng 31
            int rdlen = U16(m, pos + 8);                                                 // dòng 32
            Need(len, pos + 10 + rdlen);
            byte[] rdata = m.AsSpan(pos + 10, rdlen).ToArray();                          // dòng 33
            if (type == 1 && rdlen == 4) answers.Add(new IPAddress(rdata).ToString());   // dòng 34: IPV4_TEXT
            if (type == 28 && rdlen == 16) answers.Add(new IPAddress(rdata).ToString()); // dòng 35: IPV6_TEXT
            pos += 10 + rdlen;                                                           // dòng 36
        }
        return (rcode, answers);
    }

    public static int Run(string[] args)
    {
        if (args.Length < 2)
        {
            Console.WriteLine("Cách dùng: dotnet run -- <ip_dns_server> <ten_mien> [A|AAAA] [cong]");
            return 1;
        }
        string tname = args.Length > 2 ? args[2].ToUpperInvariant() : "A";
        int port = args.Length > 3 ? int.Parse(args[3]) : 53;
        if (!QTYPES.TryGetValue(tname, out ushort qtype))
        {
            Console.WriteLine("Kiểu bản ghi chỉ hỗ trợ A hoặc AAAA");
            return 1;
        }
        try
        {
            var (_, answers) = Query(new IPEndPoint(IPAddress.Parse(args[0]), port), args[1], qtype, tname);
            foreach (var a in answers) Console.WriteLine($"[client] Địa chỉ: {a}");
            if (answers.Count == 0) Console.WriteLine($"[client] Không có bản ghi {tname}");
            return 0;
        }
        catch (SocketException e) when (e.SocketErrorCode == SocketError.TimedOut)
        {
            Console.WriteLine("[client] Lỗi: hết thời gian chờ phản hồi (2000 ms)");
        }
        catch (ProtocolError e)
        {
            Console.WriteLine($"[client] Lỗi giao thức: {e.Message}");
        }
        return 1;
    }
}
