// Bài mở rộng MR1.2 – Bộ mô phỏng mạng theo sự kiện rời rạc (discrete-event simulation).
//
// Tệp này là "lõi" của bộ mô phỏng: topo mạng, hàng đợi sự kiện, định tuyến theo chi phí
// (Dijkstra) và quy tắc chuyển tiếp của từng loại nút. Không dùng socket thật: mỗi gói tin là một
// đối tượng C#, "thời gian" là đồng hồ mô phỏng tính bằng micro giây (µs).
//
// Topo (hai đoạn LAN nối qua ba router):
//
//     H1 ─┐                                   ┌─ H4
//     H2 ─┼─ SW1 ── R1 ── R2 ── R3 ── SW2 ─┼─ H5      (R1 ── R3: đường dự phòng, chi phí 3)
//     H3 ─┘   LAN A 192.168.1.0/24   LAN B 192.168.2.0/24  └─ H6
//
// Quy tắc chuyển tiếp:
// - host: nhận gói nếu gói dành cho mình (unicast đúng tên, quảng bá trong LAN, nhóm đã tham gia,
//   địa chỉ anycast mình phục vụ); host không chuyển tiếp gói của người khác;
// - switch: quảng bá → gửi tràn (flood) mọi cổng trừ cổng vào; multicast → chỉ cổng có thành viên
//   (IGMP snooping) và cổng router; unicast → đúng một cổng theo bảng định tuyến;
// - router: chặn quảng bá (biên của miền quảng bá); unicast/anycast → giảm TTL, chọn chặng kế theo
//   chi phí nhỏ nhất; multicast → chỉ chuyển tới các LAN có thành viên.
namespace BaiMR1_2;

static class Fmt
{
    /// <summary>Định dạng thời gian mô phỏng (µs) thành mili giây, một chữ số thập phân.</summary>
    public static string T(long us) => $"{us / 1000}.{us % 1000 / 100}";
}

/// <summary>Bộ sinh số giả ngẫu nhiên xorshift32: cùng hạt giống (seed) → cùng dãy số ở mọi ngôn ngữ.</summary>
class Rng
{
    uint x;
    public Rng(long seed)
    {
        x = (uint)((ulong)seed * 2654435761UL + 1);
        if (x == 0) x = 1;
    }
    public uint Next()
    {
        uint v = x;
        v ^= v << 13;
        v ^= v >> 17;
        v ^= v << 5;
        x = v;
        return v;
    }
    /// <summary>Số nguyên ngẫu nhiên trong [0, n).</summary>
    public long Below(long n) => Next() % n;
}

class Node(string name, string kind, string ip, string lan, int x, int y)
{
    public string Name = name, Kind = kind, Ip = ip, Lan = lan;
    public int X = x, Y = y;                       // tọa độ để vẽ hoạt hình HTML
    public bool Up = true;
    public int Sent, Recv, Deliv, Fwd, Drop;
}

class Link(string a, string b, int cost, long delay, long bw)
{
    public string A = a, B = b;
    public int Cost = cost;                        // chi phí định tuyến (số bước, có trọng số)
    public long Delay = delay;                     // trễ lan truyền (µs)
    public long Bw = bw;                           // băng thông: byte mỗi mili giây
    public bool Up = true;
    public int Loss;                               // xác suất mất gói, phần nghìn
    public long Jitter;                            // trễ ngẫu nhiên thêm tối đa (µs)
    public Dictionary<string, long> Free = new() { [a] = 0, [b] = 0 };   // thời điểm mỗi chiều rảnh (FIFO)
}

/// <summary>Gói tin. Các trường dữ liệu của tầng trên (TCP, ứng dụng) đặt thẳng trong gói cho gọn.</summary>
class Packet(string cast, string kind, string src, string dst, string label, int size = 100)
{
    public string Cast = cast;     // uni | bc | mc | igmp | any
    public string Kind = kind;     // nhãn hiển thị: TCP, UDP, UNI, BC, MC, IGMP, ANY
    public string Src = src, Dst = dst, Label = label;
    public int Size = size;
    public int Ttl = 64;
    public (string, string)? DropOn;   // kịch bản ép mất gói trên liên kết a→b
    // dữ liệu tầng trên
    public string Flags = "", Msg = "";
    public long Seq, Ack, Len, T;
    public int N;
    public bool Reply, Join;
    public Packet Copy() => (Packet)MemberwiseClone();
}

class Net
{
    // Topo: (tên, loại, địa chỉ IP, LAN, x, y)
    public static readonly (string Name, string Kind, string Ip, string Lan, int X, int Y)[] Nodes =
    [
        ("H1", "host", "192.168.1.11", "A", 60, 80), ("H2", "host", "192.168.1.12", "A", 60, 210),
        ("H3", "host", "192.168.1.13", "A", 60, 340), ("SW1", "switch", "-", "A", 190, 210),
        ("R1", "router", "192.168.1.1", "A", 330, 210), ("R2", "router", "10.0.0.2", "-", 450, 80),
        ("R3", "router", "192.168.2.1", "B", 570, 210), ("SW2", "switch", "-", "B", 710, 210),
        ("H4", "host", "192.168.2.14", "B", 840, 80), ("H5", "host", "192.168.2.15", "B", 840, 210),
        ("H6", "host", "192.168.2.16", "B", 840, 340),
    ];
    const long LanUs = 500, WanUs = 5000;        // trễ lan truyền: trong LAN 0,5 ms; giữa router 5 ms
    const long LanBw = 12500, WanBw = 1250;      // 100 Mbit/s và 10 Mbit/s (byte/ms)
    // Liên kết: (a, b, chi phí, trễ µs, băng thông)
    public static readonly (string A, string B, int Cost, long Delay, long Bw)[] Links =
    [
        ("H1", "SW1", 1, LanUs, LanBw), ("H2", "SW1", 1, LanUs, LanBw),
        ("H3", "SW1", 1, LanUs, LanBw), ("SW1", "R1", 1, LanUs, LanBw),
        ("R1", "R2", 1, WanUs, WanBw), ("R2", "R3", 1, WanUs, WanBw),
        ("R1", "R3", 3, 8000, WanBw), ("R3", "SW2", 1, LanUs, LanBw),
        ("SW2", "H4", 1, LanUs, LanBw), ("SW2", "H5", 1, LanUs, LanBw),
        ("SW2", "H6", 1, LanUs, LanBw),
    ];

    public readonly Rng Rng;
    public long Now;
    long seq;
    readonly PriorityQueue<Action, (long, long)> q = new();   // (thời điểm, thứ tự thêm vào)
    public readonly List<string> Order = [];
    public readonly Dictionary<string, Node> N = [];
    public readonly Dictionary<string, List<(string To, Link L)>> Adj = [];
    public readonly Dictionary<(string, string), Link> L = [];
    public readonly Dictionary<string, List<string>> Anycast = [];           // địa chỉ anycast -> host phục vụ
    readonly Dictionary<(string, string), List<string>> snoop = [];         // (switch, nhóm) -> host thành viên
    readonly Dictionary<string, List<string>> mroute = [];                  // nhóm -> switch (LAN) có thành viên
    public readonly Dictionary<string, List<string>> Member = [];           // host -> các nhóm đã tham gia
    public bool HopTrace = true;                 // in từng bước chuyển (tắt với TCP/UDP cho gọn)
    public bool Quiet;                           // không in nhật ký (dùng khi chỉ cần chỉ số, ví dụ lệnh so_sanh)
    public Action<string, Packet>? OnDeliver;    // tầng ứng dụng của kịch bản
    public Action<Packet, string, string>? OnLoss;   // kịch bản được báo khi mất gói
    public readonly List<object[]> Hops = [], Notes = [];   // dữ liệu cho hoạt hình HTML
    public readonly List<object[]> Changes = [];            // thay đổi trạng thái nút/liên kết
    public Dictionary<string, Dictionary<string, int>> Dist = [];
    public Dictionary<string, Dictionary<string, string?>> Nh = [];

    public Net(long seed)
    {
        Rng = new Rng(seed);
        foreach (var n in Nodes)
        {
            Order.Add(n.Name);
            N[n.Name] = new Node(n.Name, n.Kind, n.Ip, n.Lan, n.X, n.Y);
            Adj[n.Name] = [];
            Member[n.Name] = [];
        }
        foreach (var (a, b, cost, delay, bw) in Links)
        {
            var link = new Link(a, b, cost, delay, bw);
            L[(a, b)] = L[(b, a)] = link;
            Adj[a].Add((b, link));
            Adj[b].Add((a, link));
        }
        ComputeRoutes();
    }

    // ---------- hàng đợi sự kiện ----------
    public void At(long t, Action fn) => q.Enqueue(fn, (t, ++seq));

    public void Run()
    {
        while (q.TryDequeue(out var fn, out var key))
        {
            Now = key.Item1;
            fn();
        }
    }

    public void Log(string text, string node = "")
    {
        if (!Quiet) Console.WriteLine($"{Fmt.T(Now),7} ms  {text}");
        Notes.Add([Now, node, text]);
    }

    // ---------- định tuyến: Dijkstra từ mọi nút ----------
    public void ComputeRoutes()
    {
        Dist = []; Nh = [];
        foreach (var s in Order)
        {
            if (!N[s].Up) continue;
            var dist = new Dictionary<string, int> { [s] = 0 };
            var first = new Dictionary<string, string?> { [s] = null };
            var done = new HashSet<string>();
            while (true)
            {
                string? u = null;
                foreach (var n in Order)          // chọn nút chưa xét có khoảng cách nhỏ nhất
                    if (dist.ContainsKey(n) && !done.Contains(n) && (u == null || dist[n] < dist[u]))
                        u = n;
                if (u == null) break;
                done.Add(u);
                if (u != s && N[u].Kind == "host") continue;      // host không chuyển tiếp cho nút khác
                foreach (var (v, link) in Adj[u])
                {
                    if (!link.Up || !N[v].Up) continue;
                    int d = dist[u] + link.Cost;
                    if (!dist.TryGetValue(v, out int old) || d < old)
                    {
                        dist[v] = d;
                        first[v] = u == s ? v : first[u];
                    }
                }
            }
            Dist[s] = dist; Nh[s] = first;
        }
    }

    string? NextHop(string from, string? to) =>
        to != null && Nh.TryGetValue(from, out var t) && t.TryGetValue(to, out var h) ? h : null;

    public List<string> Path(string a, string b)
    {
        var p = new List<string> { a };
        while (p[^1] != b && NextHop(p[^1], b) is string h) p.Add(h);
        return p;
    }

    /// <summary>Đích thật của gói: với anycast là máy phục vụ gần nhất (chi phí nhỏ nhất) còn hoạt động.</summary>
    public string? Target(string node, Packet pkt)
    {
        if (pkt.Cast != "any") return pkt.Dst;
        (int D, string H)? best = null;
        foreach (var h in Anycast.GetValueOrDefault(pkt.Dst, []))
        {
            if (N[h].Up && Dist.TryGetValue(node, out var dd) && dd.TryGetValue(h, out int d)
                && (best == null || d < best.Value.D))
                best = (d, h);
        }
        return best?.H;
    }

    // ---------- gửi, truyền trên liên kết, nhận ----------
    /// <summary>Host node phát gói pkt vào mạng.</summary>
    public void Send(string node, Packet pkt)
    {
        N[node].Sent++;
        var nbr = Adj[node][0].To;                // host chỉ có một liên kết (tới switch)
        Transmit(node, nbr, pkt);
    }

    void Transmit(string a, string b, Packet pkt)
    {
        var link = L[(a, b)];
        if (!link.Up || !N[b].Up)
        {
            N[a].Drop++;
            if (HopTrace) Log($"{a}: liên kết tới {b} không hoạt động, bỏ gói", a);
            return;
        }
        var p = pkt.Copy();
        long start = Math.Max(Now, link.Free[a]);
        long tx = p.Size * 1000L / link.Bw;           // thời gian đẩy gói lên đường truyền
        link.Free[a] = start + tx;
        long arr = start + tx + link.Delay;
        if (link.Jitter != 0) arr += Rng.Below(link.Jitter + 1);
        bool lost = false;
        if (p.DropOn == (a, b))
        {
            lost = true; p.DropOn = null;
        }
        else if (link.Loss != 0 && Rng.Below(1000) < link.Loss)
            lost = true;
        Hops.Add([start, arr, a, b, p.Kind, p.Label, lost]);
        if (lost) At((start + arr) / 2, () => Lost(p, a, b));
        else At(arr, () => Arrive(b, p, a));
    }

    void Lost(Packet pkt, string a, string b)
    {
        N[a].Drop++;
        if (OnLoss != null) OnLoss(pkt, a, b);
        else Log($"× {pkt.Label}: mất trên {a}→{b}", a);
    }

    void Arrive(string node, Packet pkt, string frm)
    {
        var nd = N[node];
        nd.Recv++;
        if (HopTrace) Log($"{frm,3} → {node,-3} {pkt.Label}", node);
        if (nd.Kind == "host") HostIn(nd, pkt);
        else if (nd.Kind == "switch") SwitchIn(nd, pkt, frm);
        else RouterIn(nd, pkt, frm);
    }

    void HostIn(Node nd, Packet pkt)
    {
        bool mine = pkt.Cast switch
        {
            "uni" => pkt.Dst == nd.Name,
            "any" => Anycast.GetValueOrDefault(pkt.Dst, []).Contains(nd.Name),
            "bc" => true,                                 // khung quảng bá tới được thì thuộc LAN này
            "mc" => Member[nd.Name].Contains(pkt.Dst),
            _ => false,
        };
        if (mine)
        {
            nd.Deliv++;
            OnDeliver?.Invoke(nd.Name, pkt);
        }
        else
        {
            nd.Drop++;
            if (HopTrace) Log($"{nd.Name} bỏ qua: gói không dành cho mình", nd.Name);
        }
    }

    void SwitchIn(Node nd, Packet pkt, string frm)
    {
        var ports = Adj[nd.Name].Select(e => e.To).ToList();
        List<string> outs;
        if (pkt.Cast == "bc")
            outs = ports.Where(v => v != frm).ToList();                    // gửi tràn
        else if (pkt.Cast == "igmp")
        {
            var key = (nd.Name, pkt.Dst);
            if (!snoop.TryGetValue(key, out var mem)) snoop[key] = mem = [];
            if (pkt.Join && !mem.Contains(frm)) mem.Add(frm);
            if (!pkt.Join && mem.Contains(frm)) mem.Remove(frm);
            outs = ports.Where(v => N[v].Kind == "router").ToList();
        }
        else if (pkt.Cast == "mc")
        {
            var mem = snoop.GetValueOrDefault((nd.Name, pkt.Dst), []);
            outs = ports.Where(v => v != frm && (mem.Contains(v) || N[v].Kind == "router")).ToList();
        }
        else
        {
            var nh = NextHop(nd.Name, Target(nd.Name, pkt));
            outs = nh != null ? [nh] : [];
        }
        foreach (var v in outs)
        {
            nd.Fwd++;
            Transmit(nd.Name, v, pkt);
        }
    }

    void RouterIn(Node nd, Packet pkt, string frm)
    {
        if (pkt.Cast == "bc")
        {
            nd.Drop++;
            Log($"{nd.Name} chặn quảng bá: không chuyển ra ngoài LAN {N[frm].Lan}", nd.Name);
            return;
        }
        if (pkt.Cast == "igmp")
        {
            nd.Deliv++;
            if (!mroute.TryGetValue(pkt.Dst, out var lans)) mroute[pkt.Dst] = lans = [];
            if (pkt.Join && !lans.Contains(frm))
            {
                lans.Add(frm);
                Log($"{nd.Name} ghi nhận: LAN {N[frm].Lan} có thành viên {pkt.Dst}", nd.Name);
            }
            else if (!pkt.Join && snoop.GetValueOrDefault((frm, pkt.Dst), []).Count == 0 && lans.Contains(frm))
            {
                lans.Remove(frm);
                Log($"{nd.Name} ghi nhận: LAN {N[frm].Lan} hết thành viên {pkt.Dst}", nd.Name);
            }
            return;
        }
        pkt.Ttl--;
        if (pkt.Ttl <= 0)
        {
            nd.Drop++;
            Log($"{nd.Name} bỏ gói: TTL = 0", nd.Name);
            return;
        }
        var outs = new List<string>();
        if (pkt.Cast == "mc")
        {
            foreach (var sw in mroute.GetValueOrDefault(pkt.Dst, []))
            {
                var nh = NextHop(nd.Name, sw);
                if (nh != null && nh != frm && !outs.Contains(nh)) outs.Add(nh);
            }
            if (outs.Count == 0) Log($"{nd.Name} không chuyển tiếp: phía sau không có thành viên", nd.Name);
        }
        else
        {
            var nh = NextHop(nd.Name, Target(nd.Name, pkt));
            if (nh != null) outs.Add(nh);
            else
            {
                nd.Drop++;
                Log($"{nd.Name} bỏ gói: không có đường tới {pkt.Dst}", nd.Name);
            }
        }
        foreach (var v in outs)
        {
            nd.Fwd++;
            Transmit(nd.Name, v, pkt);
        }
    }

    // ---------- tiện ích cho kịch bản ----------
    public void SetLink(string a, string b, bool up)
    {
        L[(a, b)].Up = up;
        Changes.Add([Now, "link", a + "-" + b, up]);
        ComputeRoutes();
    }

    public void SetNode(string n, bool up)
    {
        N[n].Up = up;
        Changes.Add([Now, "node", n, up]);
        ComputeRoutes();
    }

    public void Summary()
    {
        Console.WriteLine("Tổng kết theo nút (Gửi = phát ra, Nhận = gói tới, Giao = đưa lên");
        Console.WriteLine("ứng dụng, Ch.tiếp = số bản sao chuyển đi, Bỏ = bỏ qua/chặn/mất):");
        Console.WriteLine($"  {"Nút",-4} {"Địa chỉ",-13} {"Gửi",4} {"Nhận",5} {"Giao",5} {"Ch.tiếp",8} {"Bỏ",4}");
        foreach (var n in Order)
        {
            var x = N[n];
            Console.WriteLine($"  {n,-4} {x.Ip,-13} {x.Sent,4} {x.Recv,5} {x.Deliv,5} {x.Fwd,8} {x.Drop,4}");
        }
    }
}
