using System.Buffers.Binary;
using System.Net;

namespace BaiMR4_1;

// Định dạng NetFlow phiên bản 5 – dùng chung cho máy xuất và bộ thu thập.
// Một gói xuất (export packet) = tiêu đề 24 byte + count bản ghi, mỗi bản ghi 48 byte,
// mọi trường theo thứ tự byte mạng (big-endian).
// Tiêu đề: version(2) count(2) sys_uptime(4) unix_secs(4) unix_nsecs(4) flow_sequence(4)
//          engine_type(1) engine_id(1) sampling_interval(2).
// Bản ghi: srcaddr(4) dstaddr(4) nexthop(4) input(2) output(2) dPkts(4) dOctets(4) first(4) last(4)
//          srcport(2) dstport(2) pad1(1) tcp_flags(1) prot(1) tos(1) src_as(2) dst_as(2)
//          src_mask(1) dst_mask(1) pad2(2).
record Flow(string Src, string Dst, int Sport, int Dport, int Proto, uint Pkts, uint Octets, uint First, uint Last);
record Header(int Version, int Count, uint Uptime, uint Secs, uint Nsecs, uint Seq, byte EngineType, byte EngineId);

static class NetflowV5
{
    public const int HeaderSize = 24, RecordSize = 48;
    public const int MaxRecords = 30;                       // giới hạn của phiên bản 5
    public static readonly Dictionary<int, string> ProtoName = new() { [6] = "TCP", [17] = "UDP", [1] = "ICMP" };

    static void PutIp(Span<byte> b, string ip) => IPAddress.Parse(ip).GetAddressBytes().CopyTo(b);
    static string GetIp(ReadOnlySpan<byte> b) => new IPAddress(b[..4]).ToString();

    public static byte[] Pack(IReadOnlyList<Flow> flows, uint seq, uint uptimeMs)
    {
        var buf = new byte[HeaderSize + flows.Count * RecordSize];
        var now = DateTimeOffset.UtcNow;
        long ticks = now.ToUnixTimeMilliseconds();
        var h = buf.AsSpan();
        BinaryPrimitives.WriteUInt16BigEndian(h[0..], 5);                     // version
        BinaryPrimitives.WriteUInt16BigEndian(h[2..], (ushort)flows.Count);   // count
        BinaryPrimitives.WriteUInt32BigEndian(h[4..], uptimeMs);              // sys_uptime
        BinaryPrimitives.WriteUInt32BigEndian(h[8..], (uint)(ticks / 1000));  // unix_secs
        BinaryPrimitives.WriteUInt32BigEndian(h[12..], (uint)(ticks % 1000 * 1_000_000));   // unix_nsecs
        BinaryPrimitives.WriteUInt32BigEndian(h[16..], seq);                  // flow_sequence
        h[20] = 0; h[21] = 1;                                                 // engine_type, engine_id
        BinaryPrimitives.WriteUInt16BigEndian(h[22..], 0);                    // sampling_interval
        for (int i = 0; i < flows.Count; i++)
        {
            var f = flows[i];
            var r = buf.AsSpan(HeaderSize + i * RecordSize, RecordSize);
            PutIp(r[0..], f.Src); PutIp(r[4..], f.Dst); PutIp(r[8..], "0.0.0.0");
            BinaryPrimitives.WriteUInt16BigEndian(r[12..], 1);                // input
            BinaryPrimitives.WriteUInt16BigEndian(r[14..], 2);                // output
            BinaryPrimitives.WriteUInt32BigEndian(r[16..], f.Pkts);
            BinaryPrimitives.WriteUInt32BigEndian(r[20..], f.Octets);
            BinaryPrimitives.WriteUInt32BigEndian(r[24..], f.First);
            BinaryPrimitives.WriteUInt32BigEndian(r[28..], f.Last);
            BinaryPrimitives.WriteUInt16BigEndian(r[32..], (ushort)f.Sport);
            BinaryPrimitives.WriteUInt16BigEndian(r[34..], (ushort)f.Dport);
            r[36] = 0;                                                        // pad1
            r[37] = (byte)(f.Proto == 6 ? 0x1B : 0);                          // tcp_flags
            r[38] = (byte)f.Proto; r[39] = 0;                                 // prot, tos
            // src_as, dst_as = 0 (đã là 0); src_mask = 24, dst_mask = 27; pad2 = 0
            r[44] = 24; r[45] = 27;
        }
        return buf;
    }

    /// <summary>Trả về (tiêu đề, bản ghi) hoặc ném FormatException nếu gói không hợp lệ.</summary>
    public static (Header H, List<Flow> Records) Parse(ReadOnlySpan<byte> d)
    {
        if (d.Length < HeaderSize) throw new FormatException($"quá ngắn ({d.Length} byte)");
        var h = new Header(BinaryPrimitives.ReadUInt16BigEndian(d), BinaryPrimitives.ReadUInt16BigEndian(d[2..]),
            BinaryPrimitives.ReadUInt32BigEndian(d[4..]), BinaryPrimitives.ReadUInt32BigEndian(d[8..]),
            BinaryPrimitives.ReadUInt32BigEndian(d[12..]), BinaryPrimitives.ReadUInt32BigEndian(d[16..]), d[20], d[21]);
        if (h.Version != 5) throw new FormatException($"phiên bản {h.Version}, không phải 5");
        if (h.Count < 1 || h.Count > MaxRecords || d.Length != HeaderSize + h.Count * RecordSize)
            throw new FormatException($"độ dài {d.Length} không khớp count={h.Count}");
        var records = new List<Flow>();
        for (int i = 0; i < h.Count; i++)
        {
            var r = d.Slice(HeaderSize + i * RecordSize, RecordSize);
            records.Add(new Flow(GetIp(r), GetIp(r[4..]),
                BinaryPrimitives.ReadUInt16BigEndian(r[32..]), BinaryPrimitives.ReadUInt16BigEndian(r[34..]), r[38],
                BinaryPrimitives.ReadUInt32BigEndian(r[16..]), BinaryPrimitives.ReadUInt32BigEndian(r[20..]),
                BinaryPrimitives.ReadUInt32BigEndian(r[24..]), BinaryPrimitives.ReadUInt32BigEndian(r[28..])));
        }
        return (h, records);
    }
}
