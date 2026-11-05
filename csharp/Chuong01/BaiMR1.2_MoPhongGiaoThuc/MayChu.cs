// Bài mở rộng MR1.2 – Bên trong máy chủ: đồng bộ, đa luồng, nhóm luồng, bất đồng bộ và phối hợp.
//
// Sáu yêu cầu từ các client (H1, H2, H3 ở LAN A; H4, H6 ở LAN B) đi qua mạng mô phỏng tới máy chủ H5.
// Mỗi yêu cầu cần đọc CSDL 30 ms (chờ I/O: luồng chờ nhưng không tốn CPU) rồi xử lý 10 ms CPU;
// máy chủ có 2 lõi CPU. Cùng khối việc đó, máy chủ được tổ chức theo năm mô hình:
//
//   dong_bo      một luồng, xử lý lần lượt: yêu cầu sau phải chờ yêu cầu trước xong hẳn;
//   da_luong     mỗi yêu cầu một luồng (tạo luồng 1 ms); luồng nhiều hơn lõi thì phải chờ lõi;
//                cập nhật bộ đếm chung trong đoạn găng có khóa (3 ms) – luồng khác phải chờ khóa;
//   nhom_luong   nhóm 2 luồng thợ + hàng đợi tối đa 2 yêu cầu; hàng đợi đầy thì từ chối ngay (503);
//   bat_dong_bo  một luồng chạy vòng lặp sự kiện: I/O không chặn (nhiều I/O chờ cùng lúc), nhưng phần
//                CPU chạy ngay trên vòng lặp nên vòng lặp bị chặn – nhịp tim 10 ms chạy muộn;
//   phoi_hop     vòng lặp sự kiện lo I/O, đẩy phần CPU sang nhóm 2 luồng (như Task.Run); luồng thợ
//                trả kết quả về vòng lặp (như SynchronizationContext.Post) – vòng lặp luôn rảnh,
//                nhịp tim đúng giờ.
//
// Mô hình giảng dạy đã đơn giản hóa: lõi CPU cấp theo thứ tự đến, không chia lát thời gian (không
// preemption); đoạn găng chỉ tính thời gian giữ khóa; thời gian chuyển ngữ cảnh bằng 0.
// Không có luồng thật nào được tạo: luồng, lõi CPU, khóa, vòng lặp sự kiện đều là sự kiện của Net.
namespace BaiMR1_2;

/// <summary>Một luồng của máy chủ và các đoạn hoạt động của nó: [t0, t1, trạng thái, mã yêu cầu].</summary>
class Lane(string name)
{
    public readonly string Name = name;
    public readonly List<object[]> Segs = [];
    public readonly List<long> Marks = [];   // thời điểm vòng lặp chạy một việc rất ngắn (nhận, chuyển giao, nhịp tim)

    public void Seg(long t0, long t1, string kind, string label)
    {
        if (t1 > t0) Segs.Add([t0, t1, kind, label]);
    }
}

/// <summary>Chỉ số so sánh của một mô hình máy chủ.</summary>
record ChiSoMayChu(long HoanTat, long Tb, long Lon, int Luong, int TuChoi, long Nhip);

class MayChu
{
    const long Ms = 1000;
    const long IoUs = 30 * Ms;        // đọc CSDL: chờ I/O
    const long CpuUs = 10 * Ms;       // xử lý: dùng một lõi CPU
    const long KhoaUs = 3 * Ms;       // đoạn găng (mô hình đa luồng)
    const long TaoUs = 1 * Ms;        // tạo một luồng mới (mô hình đa luồng)
    const long NhipUs = 10 * Ms;      // chu kỳ nhịp tim của vòng lặp sự kiện
    const int SoLoi = 2;              // số lõi CPU của máy chủ
    const int SoTho = 2;              // số luồng của nhóm luồng
    const int HangToiDa = 2;          // sức chứa hàng đợi của nhóm luồng
    const string Server = "H5";
    // (mã yêu cầu, client, thời điểm gửi µs)
    static readonly (string Label, string Client, long T)[] YeuCau =
    [
        ("A", "H4", 0), ("B", "H6", 0), ("C", "H4", 2 * Ms),
        ("D", "H1", 0), ("E", "H2", 0), ("F", "H3", 0),
    ];

    public static readonly Dictionary<string, string> Titles = new()
    {
        ["dong_bo"] = "Đồng bộ, một luồng – xử lý lần lượt, yêu cầu sau phải chờ",
        ["da_luong"] = "Đa luồng – mỗi yêu cầu một luồng, chờ lõi CPU, khóa",
        ["nhom_luong"] = "Nhóm luồng – 2 luồng thợ + hàng đợi 2, đầy thì từ chối (503)",
        ["bat_dong_bo"] = "Bất đồng bộ – một vòng lặp sự kiện, việc CPU chặn vòng lặp",
        ["phoi_hop"] = "Phối hợp – vòng lặp sự kiện lo I/O, nhóm luồng lo việc CPU",
    };
    static readonly Dictionary<string, string> TenNgan = new()
    {
        ["dong_bo"] = "đồng bộ", ["da_luong"] = "đa luồng", ["nhom_luong"] = "nhóm luồng",
        ["bat_dong_bo"] = "bất đồng bộ", ["phoi_hop"] = "phối hợp",
    };
    static readonly Dictionary<string, string[]> NhanXet = new()
    {
        ["dong_bo"] = ["Nhận xét: một luồng làm lần lượt; khi chờ CSDL luồng bị chặn,",
                       "  CPU rảnh mà không phục vụ được ai; yêu cầu sau chờ cộng dồn."],
        ["da_luong"] = ["Nhận xét: mỗi yêu cầu một luồng nên các lần chờ I/O chồng lên",
                        "  nhau; nhưng 6 luồng chỉ có 2 lõi (-: chờ lõi), đoạn găng chỉ",
                        "  cho một luồng vào mỗi lúc (~: chờ khóa); mỗi luồng tốn bộ",
                        "  nhớ và thời gian tạo (+)."],
        ["nhom_luong"] = ["Nhận xét: số luồng cố định nên tài nguyên được giới hạn; khi",
                          "  2 thợ đều bận và hàng đợi đầy, yêu cầu bị từ chối ngay (503)",
                          "  thay vì làm máy chủ quá tải."],
        ["bat_dong_bo"] = ["Nhận xét: một luồng vẫn phục vụ nhiều yêu cầu vì I/O không",
                           "  chặn (hàng I/O đang chờ có tới 6 lệnh cùng lúc); nhưng việc",
                           "  CPU chạy trên vòng lặp làm mọi việc khác, kể cả nhịp tim,",
                           "  phải chờ."],
        ["phoi_hop"] = ["Nhận xét: vòng lặp chỉ điều phối (nhận, gửi lệnh I/O, trả lời);",
                        "  việc CPU chạy trên nhóm luồng, song song trên 2 lõi, rồi báo",
                        "  về vòng lặp: vòng lặp luôn rảnh, nhịp tim đúng giờ."],
    };
    static readonly Dictionary<string, string> KyHieu = new()
    {
        ["khoa"] = "#", ["cho_khoa"] = "~", ["cho_cpu"] = "-", ["tao"] = "+",
    };

    class Req
    {
        public string Client = "";
        public long Gui, Den, Nhan;
        public int Ma;
    }

    readonly Net net;
    readonly string mode;
    readonly List<Lane> lanes = [];
    int freeCore = SoLoi;                                                  // lõi CPU rảnh
    readonly Queue<(Lane L, string Label, Action Done, long Tw)> cpuWait = new();   // hàng chờ lõi (FIFO)
    bool lockBusy;
    readonly Queue<(Lane L, string Label, Action Done, long Tw)> lockWait = new();
    int dem;
    readonly List<object[]> io = [];          // I/O bất đồng bộ đang chờ: [t0, t1, mã]
    readonly List<long[]> beats = [];         // nhịp tim [dự kiến, thực tế]
    readonly Dictionary<string, Req> req = [];
    int choXuLy = YeuCau.Length;              // số yêu cầu máy chủ chưa trả lời
    int soLuong, maxLuong;
    int tuChoi;
    // trạng thái riêng của từng mô hình
    Lane chinh = null!;
    readonly Queue<string> choKetNoi = new();
    bool ban;
    Lane[] tho = [];
    bool[] ranh = [];
    readonly Queue<(string Label, long Tw)> hangNhom = new();
    readonly Queue<string> hangPhoi = new();
    Lane vl = null!;
    readonly Queue<(Action Fn, long Dur, string Label)> ready = new();
    bool vlBan;

    MayChu(Net net, string mode)
    {
        this.net = net;
        this.mode = mode;
    }

    void Log(string text) => net.Log(text, Server);

    Lane NewLane(string name)
    {
        var l = new Lane(name);
        lanes.Add(l);
        return l;
    }

    // ---------- tài nguyên dùng chung: lõi CPU và khóa ----------
    /// <summary>Chạy 10 ms CPU trên lane; hết lõi thì chờ theo thứ tự đến.</summary>
    void Cpu(Lane lane, string label, Action done)
    {
        if (freeCore > 0)
        {
            freeCore--;
            CpuStart(lane, label, done);
        }
        else
        {
            Log($"{lane.Name}: {label} chờ lõi CPU (cả {SoLoi} lõi đang bận)");
            cpuWait.Enqueue((lane, label, done, net.Now));
        }
    }

    void CpuStart(Lane lane, string label, Action done)
    {
        long t = net.Now;
        lane.Seg(t, t + CpuUs, "cpu", label);
        net.At(t + CpuUs, () => CpuEnd(done));
    }

    void CpuEnd(Action done)
    {
        if (cpuWait.Count > 0)                 // trao lõi vừa rảnh cho luồng chờ lâu nhất
        {
            var (lane, label, d2, tw) = cpuWait.Dequeue();
            lane.Seg(tw, net.Now, "cho_cpu", label);
            CpuStart(lane, label, d2);
        }
        else freeCore++;
        done();
    }

    /// <summary>Vào đoạn găng: giữ khóa 3 ms để cập nhật bộ đếm chung; khóa đang bị giữ thì chờ.</summary>
    void Khoa(Lane lane, string label, Action done)
    {
        if (!lockBusy)
        {
            lockBusy = true;
            KhoaGiu(lane, label, done);
        }
        else
        {
            Log($"{lane.Name}: {label} chờ khóa (luồng khác đang trong đoạn găng)");
            lockWait.Enqueue((lane, label, done, net.Now));
        }
    }

    void KhoaGiu(Lane lane, string label, Action done)
    {
        long t = net.Now;
        lane.Seg(t, t + KhoaUs, "khoa", label);
        net.At(t + KhoaUs, () =>
        {
            dem++;
            Log($"{lane.Name}: {label} vào đoạn găng, bộ đếm chung = {dem}, nhả khóa");
            if (lockWait.Count > 0)
            {
                var (l2, lb2, d2, tw) = lockWait.Dequeue();
                l2.Seg(tw, net.Now, "cho_khoa", lb2);
                KhoaGiu(l2, lb2, d2);
            }
            else lockBusy = false;
            done();
        });
    }

    // ---------- mạng: gửi yêu cầu, nhận yêu cầu, trả lời ----------
    void BatDau()
    {
        net.OnDeliver = Nhan;
        foreach (var (label, client, t) in YeuCau)
        {
            req[label] = new Req { Client = client, Gui = t };
            net.At(t, () =>
            {
                net.Log($"{client} gửi yêu cầu {label} tới máy chủ {Server}", client);
                net.Send(client, new Packet("uni", "YC", client, Server, $"YC {label} {client}→{Server}", 200)
                    { Msg = label });
            });
        }
        switch (mode)
        {
            case "dong_bo": KhoiTaoDongBo(); break;
            case "da_luong": break;
            case "nhom_luong": KhoiTaoNhomLuong(); break;
            case "bat_dong_bo": KhoiTaoBatDongBo(); break;
            case "phoi_hop": KhoiTaoPhoiHop(); break;
        }
    }

    // Gói YC/TL mang mã yêu cầu trong Msg và mã trả lời (200/503) trong N.
    void Nhan(string node, Packet pkt)
    {
        string label = pkt.Msg;
        var r = req[label];
        if (node == Server)
        {
            r.Den = net.Now;
            Log($"yêu cầu {label} từ {r.Client} tới {Server}");
            switch (mode)
            {
                case "dong_bo": DenDongBo(label); break;
                case "da_luong": DenDaLuong(label); break;
                case "nhom_luong": DenNhomLuong(label); break;
                case "bat_dong_bo": VlNhan(label, BatIoXong); break;
                case "phoi_hop": VlNhan(label, PhoiIoXong); break;
            }
        }
        else
        {
            r.Nhan = net.Now;
            r.Ma = pkt.N;
            net.Log($"{node} nhận trả lời {label} ({pkt.N}) sau {Fmt.T(net.Now - r.Gui)} ms", node);
        }
    }

    void TraLoi(string label, int code = 200)
    {
        string c = req[label].Client;
        choXuLy--;
        string text = code == 200 ? $"TL {label} {Server}→{c}" : $"503 {label} {Server}→{c}";
        net.Send(Server, new Packet("uni", "TL", Server, c, text, code == 200 ? 1000 : 100)
            { Msg = label, N = code });
    }

    // ---------- 1. đồng bộ, một luồng ----------
    void KhoiTaoDongBo()
    {
        chinh = NewLane("luồng chính");
        soLuong = maxLuong = 1;
    }

    void DenDongBo(string label)
    {
        if (ban)
        {
            choKetNoi.Enqueue(label);
            Log($"luồng chính bận → {label} chờ trong hàng đợi kết nối ({choKetNoi.Count})");
        }
        else DongBoXuLy(label);
    }

    void DongBoXuLy(string label)
    {
        ban = true;
        long t = net.Now;
        Log($"luồng chính xử lý {label}: đọc CSDL, luồng bị chặn {IoUs / Ms} ms");
        chinh.Seg(t, t + IoUs, "io", label);
        net.At(t + IoUs, () => Cpu(chinh, label, () => DongBoXong(label)));
    }

    void DongBoXong(string label)
    {
        Log($"luồng chính xong {label} (CPU {CpuUs / Ms} ms), gửi trả lời");
        TraLoi(label);
        if (choKetNoi.Count > 0) DongBoXuLy(choKetNoi.Dequeue());
        else ban = false;
    }

    // ---------- 2. đa luồng: mỗi yêu cầu một luồng ----------
    void DenDaLuong(string label)
    {
        var l = NewLane($"T{lanes.Count + 1}");
        soLuong++;
        maxLuong = Math.Max(maxLuong, soLuong);
        Log($"tạo luồng {l.Name} cho {label} ({soLuong} luồng đang chạy)");
        long t = net.Now;
        l.Seg(t, t + TaoUs, "tao", label);

        void Xong()
        {
            soLuong--;
            Log($"{l.Name}: gửi trả lời {label}, luồng kết thúc ({soLuong} luồng còn chạy)");
            TraLoi(label);
        }

        net.At(t + TaoUs, () =>
        {
            long u = net.Now;
            Log($"{l.Name}: {label} đọc CSDL ({l.Name} bị chặn, các luồng khác vẫn chạy)");
            l.Seg(u, u + IoUs, "io", label);
            net.At(u + IoUs, () => Cpu(l, label, () => Khoa(l, label, Xong)));
        });
    }

    // ---------- 3. nhóm luồng + hàng đợi giới hạn ----------
    void KhoiTaoNhomLuong()
    {
        tho = Enumerable.Range(1, SoTho).Select(i => NewLane($"W{i}")).ToArray();
        ranh = Enumerable.Repeat(true, SoTho).ToArray();
        soLuong = maxLuong = SoTho;
    }

    void DenNhomLuong(string label)
    {
        for (int i = 0; i < SoTho; i++)
        {
            if (ranh[i])
            {
                NhomGiao(i, label);
                return;
            }
        }
        if (hangNhom.Count < HangToiDa)
        {
            hangNhom.Enqueue((label, net.Now));
            Log($"W1, W2 đều bận → {label} vào hàng đợi ({hangNhom.Count}/{HangToiDa})");
        }
        else
        {
            tuChoi++;
            Log($"hàng đợi đầy → từ chối {label} ngay (503)");
            TraLoi(label, 503);
        }
    }

    void NhomGiao(int i, string label)
    {
        ranh[i] = false;
        var l = tho[i];
        long t = net.Now;
        Log($"{l.Name} nhận {label}: đọc CSDL ({l.Name} bị chặn {IoUs / Ms} ms)");
        l.Seg(t, t + IoUs, "io", label);
        net.At(t + IoUs, () => Cpu(l, label, () => NhomXong(i, label)));
    }

    void NhomXong(int i, string label)
    {
        Log($"W{i + 1} xong {label}, gửi trả lời");
        TraLoi(label);
        if (hangNhom.Count > 0)
        {
            var (lb, tw) = hangNhom.Dequeue();
            Log($"W{i + 1} lấy {lb} từ hàng đợi (đã chờ {Fmt.T(net.Now - tw)} ms)");
            NhomGiao(i, lb);
        }
        else ranh[i] = true;
    }

    // ---------- vòng lặp sự kiện (dùng cho 4 và 5) ----------
    void KhoiTaoVongLap()
    {
        vl = NewLane("vòng lặp");
        net.At(0, () => Nhip(0));
    }

    /// <summary>Đưa một việc vào hàng đợi của vòng lặp (dur > 0: việc CPU chạy ngay trên vòng lặp).</summary>
    void Post(Action fn, long dur = 0, string label = "")
    {
        ready.Enqueue((fn, dur, label));
        if (!vlBan)
        {
            vlBan = true;
            net.At(net.Now, Chay);
        }
    }

    /// <summary>Vòng lặp lấy lần lượt từng việc; việc CPU giữ vòng lặp tới khi xong.</summary>
    void Chay()
    {
        while (ready.Count > 0)
        {
            var (fn, dur, label) = ready.Dequeue();
            if (dur != 0)
            {
                Cpu(vl, label, () => { fn(); Chay(); });
                return;
            }
            vl.Marks.Add(net.Now);
            fn();
        }
        vlBan = false;
    }

    /// <summary>Nhịp tim mỗi 10 ms: một việc rất ngắn trên vòng lặp; đo nó chạy muộn bao nhiêu.</summary>
    void Nhip(long p)
    {
        if (choXuLy == 0) return;
        Post(() =>
        {
            beats.Add([p, net.Now]);
            if (net.Now - p >= Ms)
                Log($"nhịp tim {Fmt.T(p)} ms chạy muộn {Fmt.T(net.Now - p)} ms: vòng lặp bận");
        });
        net.At(p + NhipUs, () => Nhip(p + NhipUs));
    }

    void VlNhan(string label, Action<string> xongIo)
    {
        Post(() =>
        {
            long t = net.Now;
            Log($"vòng lặp nhận {label}, gửi lệnh đọc CSDL không chặn");
            io.Add([t, t + IoUs, label]);
            net.At(t + IoUs, () => xongIo(label));
        });
    }

    // ---------- 4. bất đồng bộ: một vòng lặp, CPU trên vòng lặp ----------
    void KhoiTaoBatDongBo()
    {
        soLuong = maxLuong = 1;
        KhoiTaoVongLap();
    }

    void BatIoXong(string label)
    {
        Log($"CSDL xong {label} → việc CPU {CpuUs / Ms} ms vào hàng đợi vòng lặp");
        Post(() =>
        {
            Log($"vòng lặp tính xong {label} (CPU trên vòng lặp), trả lời");
            TraLoi(label);
        }, CpuUs, label);
    }

    // ---------- 5. phối hợp: vòng lặp + nhóm luồng cho việc CPU ----------
    void KhoiTaoPhoiHop()
    {
        soLuong = maxLuong = 1 + SoTho;
        KhoiTaoVongLap();
        tho = Enumerable.Range(1, SoTho).Select(i => NewLane($"P{i}")).ToArray();
        ranh = Enumerable.Repeat(true, SoTho).ToArray();
    }

    void PhoiIoXong(string label)
    {
        Post(() =>
        {
            Log($"vòng lặp giao việc CPU của {label} cho nhóm luồng");
            for (int i = 0; i < SoTho; i++)
            {
                if (ranh[i])
                {
                    PhoiChay(i, label);
                    return;
                }
            }
            hangPhoi.Enqueue(label);
            Log($"P1, P2 đều bận → việc CPU của {label} chờ trong hàng đợi");
        });
    }

    void PhoiChay(int i, string label)
    {
        ranh[i] = false;
        Cpu(tho[i], label, () => PhoiXong(i, label));
    }

    void PhoiXong(int i, string label)
    {
        Log($"P{i + 1} tính xong {label} → gửi kết quả về vòng lặp");
        Post(() =>
        {
            Log($"vòng lặp nhận kết quả {label} từ nhóm luồng, trả lời");
            TraLoi(label);
        });
        if (hangPhoi.Count > 0) PhoiChay(i, hangPhoi.Dequeue());
        else ranh[i] = true;
    }

    // ---------- báo cáo ----------
    void Gantt()
    {
        long end = 0;
        foreach (var l in lanes)
            foreach (var s in l.Segs)
                end = Math.Max(end, (long)s[1]);
        foreach (var s in io)
            end = Math.Max(end, (long)s[1]);
        long step = Math.Max(Ms, (end + 46 * Ms - 1) / (46 * Ms) * Ms);    // tối đa 46 ô
        long width = (end + step - 1) / step;
        Console.WriteLine($"Bên trong máy chủ {Server}, 1 ô = {step / Ms} ms:");
        Console.WriteLine("  CHỮ HOA dùng CPU, chữ thường bị chặn chờ I/O, + tạo luồng,");
        Console.WriteLine("  - chờ lõi CPU, # giữ khóa, ~ chờ khóa, · việc rất ngắn");
        foreach (var l in lanes)
        {
            var row = new System.Text.StringBuilder();
            for (long i = 0; i < width; i++)
            {
                long mid = i * step + step / 2;
                string ch = " ";
                foreach (var s in l.Segs)
                {
                    long t0 = (long)s[0], t1 = (long)s[1];
                    string kind = (string)s[2], label = (string)s[3];
                    if (t0 <= mid && mid < t1)
                    {
                        ch = kind == "cpu" ? label : kind == "io" ? label.ToLowerInvariant() : KyHieu[kind];
                        break;
                    }
                }
                if (ch == " " && l.Marks.Any(m => i * step <= m && m < (i + 1) * step)) ch = "·";
                row.Append(ch);
            }
            Console.WriteLine($"  {l.Name,-12}|{row}|");
        }
        if (io.Count > 0)
        {
            var row = new System.Text.StringBuilder();
            for (long i = 0; i < width; i++)
            {
                long mid = i * step + step / 2;
                int n = io.Count(s => (long)s[0] <= mid && mid < (long)s[1]);
                row.Append(n != 0 ? Math.Min(n, 9).ToString() : " ");
            }
            Console.WriteLine($"  {"I/O đang chờ",-12}|{row}|");
        }
        string right = $"{width * step / Ms} ms";
        Console.WriteLine($"  {"",-12} 0{right.PadLeft((int)width - 1)}");
    }

    /// <summary>Chỉ số so sánh: hoàn tất, phản hồi trung bình/lớn nhất, số luồng, từ chối, nhịp tim trễ.</summary>
    ChiSoMayChu TinhChiSo()
    {
        long tong = 0, lon = 0, hoanTat = 0;
        foreach (var (label, _, _) in YeuCau)
        {
            var r = req[label];
            long ph = r.Nhan - r.Gui;
            tong += ph;
            lon = Math.Max(lon, ph);
            hoanTat = Math.Max(hoanTat, r.Nhan);
        }
        long nhip = beats.Count > 0 ? beats.Max(b => b[1] - b[0]) : -1;
        return new ChiSoMayChu(hoanTat, tong / YeuCau.Length, lon, maxLuong, tuChoi, nhip);
    }

    void Report()
    {
        Console.WriteLine();
        Gantt();
        Console.WriteLine();
        Console.WriteLine($"  {"Mã",-3} {"Client",-6} {"Gửi",6} {"Tới H5",7} {"Nhận TL",8} {"Phản hồi",9}  Kết quả");
        foreach (var (label, client, _) in YeuCau)
        {
            var r = req[label];
            string kq = r.Ma == 200 ? "200 OK" : "503 từ chối";
            Console.WriteLine($"  {label,-3} {client,-6} {Fmt.T(r.Gui),6} {Fmt.T(r.Den),7} " +
                              $"{Fmt.T(r.Nhan),8} {Fmt.T(r.Nhan - r.Gui),9}  {kq}");
        }
        var s = TinhChiSo();
        Console.WriteLine($"  Hoàn tất lúc {Fmt.T(s.HoanTat)} ms; phản hồi TB {Fmt.T(s.Tb)} ms, " +
                          $"lớn nhất {Fmt.T(s.Lon)} ms");
        Console.WriteLine($"  Luồng tối đa {s.Luong}; từ chối {s.TuChoi}");
        if (beats.Count > 0)
        {
            var tre = beats.Select(b => b[1] - b[0]).ToList();
            Console.WriteLine($"  Nhịp tim {NhipUs / Ms} ms: {tre.Count} nhịp, trễ lớn nhất {Fmt.T(s.Nhip)} ms, " +
                              $"TB {Fmt.T(tre.Sum() / tre.Count)} ms");
        }
        Console.WriteLine();
        foreach (var line in NhanXet[mode]) Console.WriteLine(line);
    }

    /// <summary>Dữ liệu cho hoạt hình HTML: các luồng, I/O đang chờ, nhịp tim.</summary>
    Dictionary<string, object> Data()
    {
        var d = new Dictionary<string, object>
        {
            ["lanes"] = lanes.Select(l => new object[] { l.Name, l.Segs, l.Marks }).ToList(),
        };
        if (io.Count > 0) d["io"] = io;
        if (beats.Count > 0) d["beats"] = beats;
        return d;
    }

    /// <summary>Chạy một mô hình máy chủ trên net, in nhật ký và báo cáo; trả về dữ liệu hoạt hình.</summary>
    public static Dictionary<string, object> Chay(Net net, string mode)
    {
        net.HopTrace = false;
        Console.WriteLine($"6 yêu cầu tới {Server}, mỗi yêu cầu: đọc CSDL {IoUs / Ms} ms (chờ I/O)");
        Console.WriteLine($"  + xử lý {CpuUs / Ms} ms CPU; máy chủ có {SoLoi} lõi CPU");
        var m = new MayChu(net, mode);
        m.BatDau();
        net.Run();
        m.Report();
        return m.Data();
    }

    /// <summary>Chạy một mô hình máy chủ không in gì (net.Quiet); trả về chỉ số so sánh.</summary>
    public static ChiSoMayChu ChiSo(Net net, string mode)
    {
        net.HopTrace = false;
        net.Quiet = true;
        var m = new MayChu(net, mode);
        m.BatDau();
        net.Run();
        return m.TinhChiSo();
    }

    /// <summary>In bảng so sánh năm mô hình từ chỉ số đã đo: danh sách (mô hình, chỉ số).</summary>
    public static void SoSanh(List<(string Mode, ChiSoMayChu S)> cacKetQua)
    {
        Console.WriteLine("So sánh năm mô hình máy chủ: cùng 6 yêu cầu, mỗi yêu cầu");
        Console.WriteLine($"  {IoUs / Ms} ms I/O + {CpuUs / Ms} ms CPU, máy chủ {SoLoi} lõi. TB, Lớn nhất: thời gian");
        Console.WriteLine("  phản hồi; 503: số yêu cầu bị từ chối; Nhịp tim: trễ lớn nhất.");
        Console.WriteLine($"  {"Mô hình",-11} {"Luồng",5} {"Hoàn tất",9} {"TB",9} {"Lớn nhất",9} {"503",3} {"Nhịp tim",9}");
        foreach (var (mode, s) in cacKetQua)
        {
            string nhip = s.Nhip >= 0 ? Fmt.T(s.Nhip) + " ms" : "–";
            Console.WriteLine($"  {TenNgan[mode],-11} {s.Luong,5} {Fmt.T(s.HoanTat) + " ms",9} " +
                              $"{Fmt.T(s.Tb) + " ms",9} {Fmt.T(s.Lon) + " ms",9} {s.TuChoi,3} {nhip,9}");
        }
        Console.WriteLine();
        Console.WriteLine("Ghi nhớ: luồng giúp chồng thời gian chờ I/O nhưng tốn tài nguyên");
        Console.WriteLine("  và cần khóa khi dùng chung dữ liệu; bất đồng bộ phục vụ nhiều");
        Console.WriteLine("  I/O bằng một luồng nhưng không được làm việc CPU trên vòng");
        Console.WriteLine("  lặp; phối hợp vòng lặp + nhóm luồng có được cả hai ưu điểm.");
    }
}
