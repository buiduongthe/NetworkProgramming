// Bài mở rộng MR1.2 – Các kịch bản UDP, unicast, broadcast, multicast, anycast trên bộ mô phỏng.
namespace BaiMR1_2;

static class KichBan
{
    const long Ms = 1000;          // 1 ms = 1000 µs

    static void ShowPath(Net net, string a, string b)
    {
        var p = net.Path(a, b);
        Console.WriteLine($"  Đường {a}→{b}: {string.Join(" → ", p)} (chi phí {net.Dist[a][b]})");
    }

    // ---------------- UDP ----------------
    /// <summary>H2 gửi 12 datagram tới H6, cách nhau 10 ms. Liên kết R2–R3 mất gói ngẫu nhiên và có trễ
    /// dao động (jitter) nên datagram có thể mất hoặc đến sai thứ tự; UDP không gửi lại.</summary>
    public static void Udp(Net net, bool lossSet)
    {
        var link = net.L[("R2", "R3")];
        if (!lossSet) link.Loss = 200;        // mặc định 20 %
        link.Jitter = 15 * Ms;                // trễ thêm ngẫu nhiên 0..15 ms
        net.HopTrace = false;
        Console.WriteLine($"Liên kết R2–R3: mất gói {link.Loss / 10}.{link.Loss % 10} %, trễ dao động 0–15 ms");
        ShowPath(net, "H2", "H6");
        var got = new List<int>();
        int expect = 1, late = 0;

        net.OnDeliver = (node, pkt) =>
        {
            int n = pkt.N;
            got.Add(n);
            string note;
            if (n == expect) note = "đúng thứ tự";
            else if (n > expect)
                note = "đến sớm, đang thiếu " + string.Join(", ", Enumerable.Range(expect, n - expect).Select(i => $"#{i}"));
            else
            {
                note = "ĐẾN MUỘN – sai thứ tự";
                late++;
            }
            expect = Math.Max(expect, n + 1);
            net.Log($"H6 nhận datagram #{n,-2} {note}", node);
        };
        for (int i = 1; i <= 12; i++)
        {
            int k = i;                        // biến riêng cho từng lambda
            net.At((k - 1) * 10 * Ms, () =>
            {
                net.Log($"H2 gửi datagram #{k} (200 byte)", "H2");
                net.Send("H2", new Packet("uni", "UDP", "H2", "H6", $"UDP #{k}", 200) { N = k });
            });
        }
        net.Run();
        var lost = Enumerable.Range(1, 12).Where(i => !got.Contains(i)).Select(i => $"#{i}").ToList();
        Console.WriteLine();
        Console.WriteLine($"Tóm tắt UDP: gửi 12, nhận {got.Count}, mất {lost.Count} " +
                          $"({(lost.Count > 0 ? string.Join(", ", lost) : "không")}), sai thứ tự {late}");
        Console.WriteLine("  UDP không xác nhận, không gửi lại, không sắp xếp lại:");
        Console.WriteLine("  ứng dụng tự xử lý (đánh số thứ tự, bỏ qua hoặc yêu cầu lại).");
    }

    // ---------------- Unicast ----------------
    public static void Unicast(Net net)
    {
        Console.WriteLine("Bảng định tuyến của R1 (đích → chặng kế, chi phí):");
        foreach (var h in new[] { "H2", "H5" })
            Console.WriteLine($"  {h,-3} → {net.Nh["R1"][h],-3} chi phí {net.Dist["R1"][h]}");
        ShowPath(net, "H1", "H2");
        ShowPath(net, "H1", "H5");

        net.OnDeliver = (node, pkt) =>
        {
            net.Log($"{node} nhận \"{pkt.Msg}\" từ {pkt.Src} (TTL còn {pkt.Ttl})", node);
            if (pkt.Reply)
                net.Send(node, new Packet("uni", "UNI", node, pkt.Src, $"UNI {node}→{pkt.Src}", 100)
                { Msg = $"chào {pkt.Src}" });
        };
        net.At(0, () => net.Send("H1", new Packet("uni", "UNI", "H1", "H2", "UNI H1→H2", 100) { Msg = "xin chào H2" }));
        net.At(5 * Ms, () => net.Send("H1", new Packet("uni", "UNI", "H1", "H5", "UNI H1→H5", 100)
        { Msg = "xin chào H5", Reply = true }));
        net.At(50 * Ms, () =>
        {
            net.Log("Liên kết R1–R2 hỏng → các router tính lại đường", "R1");
            net.SetLink("R1", "R2", false);
            ShowPath(net, "H1", "H5");
            net.Send("H1", new Packet("uni", "UNI", "H1", "H5", "UNI H1→H5", 100) { Msg = "lần 2" });
        });
        net.Run();
        Console.WriteLine();
        Console.WriteLine("Tóm tắt unicast: mỗi gói chỉ tới đúng một đích; switch chỉ");
        Console.WriteLine("  chuyển ra đúng một cổng; router giảm TTL, chọn chặng kế theo");
        Console.WriteLine("  chi phí nhỏ nhất; đường đổi khi liên kết hỏng.");
    }

    // ---------------- Broadcast ----------------
    public static void Broadcast(Net net)
    {
        net.OnDeliver = (node, pkt) => net.Log($"{node} nhận quảng bá \"{pkt.Msg}\" từ {pkt.Src}", node);
        net.At(0, () => net.Send("H1", new Packet("bc", "BC", "H1", "192.168.1.255", "BC 192.168.1.255", 60)
        { Msg = "Ai có 192.168.1.13?" }));
        net.At(10 * Ms, () => net.Send("H4", new Packet("bc", "BC", "H4", "255.255.255.255", "BC 255.255.255.255", 60)
        { Msg = "DHCP DISCOVER" }));
        net.Run();
        Console.WriteLine();
        Console.WriteLine("Tóm tắt broadcast: switch gửi tràn mọi cổng trong LAN; router");
        Console.WriteLine("  chặn nên H4, H5, H6 không nhận quảng bá của H1 và LAN A không");
        Console.WriteLine("  nhận quảng bá của H4 (miền quảng bá = một LAN).");
    }

    // ---------------- Multicast ----------------
    const string G = "239.1.1.1";

    public static void Multicast(Net net)
    {
        Action Join(string h, bool yes) => () =>
        {
            if (yes) net.Member[h].Add(G); else net.Member[h].Remove(G);
            string what = yes ? "tham gia" : "rời";
            string kind = yes ? "Report" : "Leave";
            net.Log($"{h} {what} nhóm {G}: gửi IGMP {kind}", h);
            net.Send(h, new Packet("igmp", "IGMP", h, G, $"IGMP {kind} {G}", 60) { Join = yes });
        };
        Action SendMc(int n) => () =>
        {
            net.Log($"H1 gửi luồng #{n} tới nhóm {G}", "H1");
            net.Send("H1", new Packet("mc", "MC", "H1", G, $"MC {G} #{n}", 500) { Msg = $"luồng #{n}" });
        };

        net.OnDeliver = (node, pkt) => net.Log($"{node} nhận {pkt.Msg} của nhóm {G}", node);
        net.At(0, Join("H2", true));
        net.At(5 * Ms, SendMc(1));
        net.At(20 * Ms, Join("H5", true));
        net.At(40 * Ms, SendMc(2));
        net.At(70 * Ms, Join("H6", true));
        net.At(75 * Ms, Join("H2", false));
        net.At(90 * Ms, SendMc(3));
        net.Run();
        Console.WriteLine();
        Console.WriteLine("Tóm tắt multicast: chỉ thành viên nhóm nhận dữ liệu; switch");
        Console.WriteLine("  (IGMP snooping) chỉ chuyển ra cổng có thành viên; router chỉ");
        Console.WriteLine("  chuyển sang LAN có thành viên; mỗi liên kết chỉ mang một bản");
        Console.WriteLine("  sao dù phía sau có nhiều thành viên.");
    }

    // ---------------- Anycast ----------------
    const string A = "10.53.0.53";

    public static void Anycast(Net net)
    {
        net.Anycast[A] = ["H3", "H6"];
        Console.WriteLine($"Địa chỉ anycast {A} (DNS) do H3 và H6 cùng quảng bá.");
        foreach (var c in new[] { "H1", "H5" })
        {
            string costs = string.Join(", ", net.Anycast[A].Select(s => $"{s} chi phí {net.Dist[c][s]}"));
            Console.WriteLine($"  {c}: {costs} → chọn {net.Target(c, new Packet("any", "ANY", c, A, ""))}");
        }

        net.OnDeliver = (node, pkt) =>
        {
            if (pkt.Cast == "any")
            {
                net.Log($"{node} (phục vụ {A}) nhận \"{pkt.Msg}\" từ {pkt.Src}", node);
                net.Send(node, new Packet("uni", "UNI", node, pkt.Src, $"UNI {node}→{pkt.Src}", 120)
                { Msg = $"trả lời từ {node}" });
            }
            else net.Log($"{node} nhận \"{pkt.Msg}\"", node);
        };
        Action Query(string c, int n) => () =>
        {
            net.Log($"{c} gửi truy vấn #{n} tới {A}", c);
            net.Send(c, new Packet("any", "ANY", c, A, $"ANY {c}→{A}", 80) { Msg = $"truy vấn #{n}" });
        };

        net.At(0, Query("H1", 1));
        net.At(2 * Ms, Query("H5", 1));
        net.At(40 * Ms, () =>
        {
            net.Log($"H3 ngừng hoạt động → rút quảng bá {A}", "H3");
            net.SetNode("H3", false);
            var best = net.Target("H1", new Packet("any", "ANY", "H1", A, ""));
            Console.WriteLine($"  H1: còn H6 chi phí {net.Dist["H1"]["H6"]} → chọn {best}");
        });
        net.At(41 * Ms, Query("H1", 2));
        net.Run();
        Console.WriteLine();
        Console.WriteLine("Tóm tắt anycast: cùng một địa chỉ, gói tới máy phục vụ gần nhất");
        Console.WriteLine("  theo chi phí định tuyến; khi H3 hỏng, truy vấn của H1 được");
        Console.WriteLine("  chuyển sang H6 (dự phòng tự động).");
    }
}
