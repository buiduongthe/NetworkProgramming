using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR4_1;

// Máy xuất NetFlow v5 giả lập (exporter) gửi qua UDP.
// Sinh các bản ghi luồng (flow) giả lập theo một bộ sinh số giả ngẫu nhiên đơn giản (LCG)
// có hạt giống cố định, nên mọi bản ngôn ngữ sinh ra đúng cùng một dữ liệu.
// Mỗi gói UDP chứa tối đa 30 bản ghi. Tùy chọn --bo-goi K cố ý KHÔNG gửi gói thứ K
// (mô phỏng mất gói trên đường truyền); cuối cùng gửi thêm một datagram rác để thử bộ thu.
static class MayXuat
{
    static readonly string[] Hosts = ["10.20.0.11", "10.20.0.12", "10.20.0.13", "10.20.0.14", "10.20.1.5", "10.20.1.6"];
    static readonly (string Dst, int Port, int Proto)[] Services =
        [("10.20.1.194", 443, 6), ("10.20.1.194", 80, 6), ("10.20.1.195", 5432, 6),
         ("10.20.1.196", 53, 17), ("203.0.113.10", 443, 6)];

    /// <summary>x = (x * 1103515245 + 12345) mod 2^31, lấy 15 bit cao (các bit thấp của LCG kém ngẫu nhiên).
    /// Đủ để sinh dữ liệu thử, KHÔNG dùng cho bảo mật.</summary>
    class Lcg(long seed)
    {
        long x = seed;
        public long Next()
        {
            x = (x * 1103515245 + 12345) & 0x7FFFFFFF;
            return x >> 16;
        }
    }

    static List<Flow> MakeFlows(int n, long seed = 2026)
    {
        var rng = new Lcg(seed);
        var flows = new List<Flow>();
        for (int i = 0; i < n; i++)
        {
            string src = Hosts[rng.Next() % Hosts.Length];
            var (dst, dport, proto) = Services[rng.Next() % Services.Length];
            long pkts = 1 + rng.Next() % 200;
            long octets = pkts * (60 + rng.Next() % 1400);
            long sport = 32768 + rng.Next() % 28000;
            long first = rng.Next() % 60000;
            long last = first + rng.Next() % 5000;
            flows.Add(new Flow(src, dst, (int)sport, dport, proto, (uint)pkts, (uint)octets, (uint)first, (uint)last));
        }
        return flows;
    }

    public static int Run(string[] args)
    {
        var pos = new List<string>();
        int soLuong = 100, boGoi = 0;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--so-luong") soLuong = int.Parse(args[++i]);
            else if (args[i] == "--bo-goi") boGoi = int.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        if (pos.Count < 2)
        {
            Console.WriteLine("Cách dùng: dotnet run -- xuat <host> <port> [--so-luong N] [--bo-goi K]");
            return 2;
        }
        var target = new IPEndPoint(IPAddress.Parse(pos[0]), int.Parse(pos[1]));
        var flows = MakeFlows(soLuong);
        using var sock = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        uint seq = 0;
        int k = 0;
        for (int start = 0; start < flows.Count; start += NetflowV5.MaxRecords)
        {
            k++;
            var chunk = flows.Skip(start).Take(NetflowV5.MaxRecords).ToList();
            byte[] data = NetflowV5.Pack(chunk, seq, (uint)(120000 + k * 1000));
            if (k == boGoi)
                Console.WriteLine($"[xuat] Gói #{k}: {chunk.Count} bản ghi, seq={seq} -> BỎ (giả lập mất gói)");
            else
            {
                sock.SendTo(data, target);
                Console.WriteLine($"[xuat] Gói #{k}: {chunk.Count} bản ghi, seq={seq}, {data.Length} byte");
            }
            seq += (uint)chunk.Count;              // flow_sequence đếm tổng số luồng đã xuất
            Thread.Sleep(50);
        }
        sock.SendTo(Encoding.ASCII.GetBytes("xin chao, day khong phai NetFlow"), target);
        Console.WriteLine("[xuat] Gửi thêm 1 datagram rác để thử bộ thu");
        return 0;
    }
}
