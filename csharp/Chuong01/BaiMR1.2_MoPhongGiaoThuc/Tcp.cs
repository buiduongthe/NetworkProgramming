// Bài mở rộng MR1.2 – Kịch bản TCP: bắt tay ba bước, cửa sổ trượt, điều khiển tắc nghẽn, đóng FIN.
//
// TCP được rút gọn để dễ theo dõi (dựa trên TCP Reno, RFC 9293 và RFC 5681):
// - bắt tay SYN → SYN+ACK → ACK; đóng FIN → ACK, FIN → ACK;
// - bên nhận xác nhận tích lũy (cumulative ACK) từng đoạn, giữ lại đoạn đến sớm (ngoài thứ tự);
// - bên gửi chỉ gửi khi số byte đang bay < cwnd (cửa sổ tắc nghẽn);
// - khởi đầu chậm (slow start): cwnd < ssthresh thì mỗi ACK mới tăng cwnd thêm 1 MSS (gấp đôi mỗi RTT);
// - tránh tắc nghẽn (congestion avoidance): mỗi ACK mới tăng MSS·MSS/cwnd (≈ +1 MSS mỗi RTT);
// - 3 ACK trùng → gửi lại nhanh (fast retransmit): ssthresh = max(đang bay/2, 2 MSS), cwnd = ssthresh;
// - hết giờ (RTO) → ssthresh = max(đang bay/2, 2 MSS), cwnd = 1 MSS, gửi lại từ đoạn chưa xác nhận,
//   RTO tăng gấp đôi (back-off).
// Đơn giản hóa: RTO cố định 100 ms ban đầu (RFC 6298 tính từ RTT đo được), không thổi phồng cwnd
// trong khôi phục nhanh, không có ACK trễ, cửa sổ bên nhận đủ lớn.
namespace BaiMR1_2;

class TcpScenario
{
    public const int Mss = 1000;
    const int Hdr = 40;            // byte tiêu đề TCP/IP
    const int W = 34;              // độ rộng phần mũi tên trong sơ đồ

    readonly Net net;
    readonly string c, s;
    readonly long total;
    readonly List<int> drops;                 // các đoạn bị ép mất (lặp số đoạn = mất nhiều lần)
    long cwnd = Mss, ssthresh = 8 * Mss;
    long una = 1, nxt = 1;                    // byte đầu tiên chưa được xác nhận / sẽ gửi tiếp
    int dup; bool recovering;
    long rto = 100_000; long gen; bool timerOn;
    readonly Dictionary<long, int> sent = []; // đoạn -> số lần đã gửi
    int tx, retx, fast, timeouts;
    long rnext = 1; readonly HashSet<long> rbuf = [];   // bên nhận: byte mong đợi tiếp, đoạn đến sớm
    readonly List<(long T, int I, string Text)> rows = [];   // dòng sơ đồ
    public readonly List<long[]> CwndLog = [[0, Mss]];
    readonly List<(long T, char M)> events = [];         // 'F' hoặc 'T' cho biểu đồ
    string state = "CLOSED";
    long tData0, tDone;

    public TcpScenario(Net net, int nseg, IEnumerable<int> drops, string client = "H1", string server = "H5")
    {
        this.net = net; c = client; s = server;
        total = (long)nseg * Mss;
        this.drops = drops.ToList();
        net.HopTrace = false;
        net.OnDeliver = Deliver;
        net.OnLoss = Loss;
    }

    /// <summary>Mũi tên giữa hai trục thời gian: ─ nhãn ───► hoặc ◄─── nhãn ─, mất gói thì dừng giữa đường bằng ×.</summary>
    static string Arrow(string label, bool right, bool lost)
    {
        static string Line(int n) => new('─', Math.Max(0, n));
        if (right)
        {
            string x = "─ " + label + " ";
            return !lost ? x + Line(W - 1 - x.Length) + "►" : (x + Line(4) + "×").PadRight(W);
        }
        else
        {
            string x = " " + label + " ─";
            return !lost ? "◄" + Line(W - 1 - x.Length) + x : ("×" + Line(4) + x).PadLeft(W);
        }
    }

    static string Cw(long v) => $"{v / Mss}.{v % Mss * 10 / Mss}";

    // ---------- ghi sơ đồ ----------
    void Row(long t, string text) => rows.Add((t, rows.Count, text));

    void PktRow(Packet pkt, long tRecv, string note, bool lost = false)
    {
        bool right = pkt.Src == c;
        string recv = lost ? "×" : Fmt.T(tRecv);
        string body = Arrow(pkt.Label, right, lost);
        Row(pkt.T, $"{Fmt.T(pkt.T),6} {recv,6}  |{body}|  {note}");
        string to = right ? s : c;                          // ghi chú cho hoạt hình HTML
        string what = lost ? $"× {pkt.Label} {note}" : $"{to} nhận {pkt.Label} {note}";
        net.Notes.Add([net.Now, to, what.TrimEnd()]);
    }

    void Event(string text)
    {
        Row(net.Now, $"{Fmt.T(net.Now),6} {"",6}  => {text}");
        net.Notes.Add([net.Now, c, text]);
    }

    void SetCwnd(long v)
    {
        cwnd = v;
        CwndLog.Add([net.Now, v]);
    }

    // ---------- gửi ----------
    void Out(string frm, string to, string label, string flags, long seq = 0, long ack = 0, int length = 0,
             bool drop = false)
    {
        var p = new Packet("uni", "TCP", frm, to, label, Hdr + length)
        {
            Flags = flags, Seq = seq, Ack = ack, Len = length, T = net.Now,
        };
        if (drop) p.DropOn = frm == c ? ("R2", "R3") : ("R2", "R1");
        net.Send(frm, p);
    }

    public void Start()
    {
        state = "SYN_SENT";
        Out(c, s, "SYN seq=0", "SYN");
        Arm();                                // SYN cũng có thể mất: hết giờ thì gửi lại (RFC 9293 §3.8.1)
    }

    void SendData()
    {
        while (nxt <= total && nxt - una + Mss <= cwnd)
        {
            long k = (nxt - 1) / Mss + 1;
            int n = sent.GetValueOrDefault(k) + 1;
            sent[k] = n;
            tx++;
            bool again = n > 1;
            if (again) retx++;
            string label = $"#{k} seq={nxt}" + (again ? " gửi lại" : "");
            Out(c, s, label, "DATA", seq: nxt, length: Mss, drop: drops.Count(d => d == k) >= n);
            nxt += Mss;
            if (!timerOn) Arm();
        }
    }

    void Arm()
    {
        gen++;
        timerOn = true;
        long g = gen;
        net.At(net.Now + rto, () => Timeout(g));
    }

    void StopTimer()
    {
        gen++;
        timerOn = false;
    }

    void Timeout(long g)
    {
        if (g != gen) return;                 // bộ hẹn giờ này đã bị hủy/khởi động lại
        if (state == "SYN_SENT")              // chưa nhận SYN+ACK: gửi lại SYN, RTO gấp đôi
        {
            Event($"HẾT GIỜ RTO={rto / 1000} ms: chưa có SYN+ACK, gửi lại SYN");
            rto = Math.Min(rto * 2, 800_000);
            Out(c, s, "SYN seq=0 gửi lại", "SYN");
            Arm();
            return;
        }
        timeouts++;
        long flight = nxt - una;
        ssthresh = Math.Max(flight / 2, 2 * Mss);
        SetCwnd(Mss);
        events.Add((net.Now, 'T'));
        Event($"HẾT GIỜ RTO={rto / 1000} ms: ssthresh={Cw(ssthresh)}, cwnd=1");
        rto = Math.Min(rto * 2, 800_000);
        nxt = una; dup = 0; recovering = false;
        timerOn = false;
        SendData();
    }

    // ---------- nhận ----------
    void Deliver(string node, Packet pkt)
    {
        var f = pkt.Flags;
        if (node == s)                                        // ---- phía máy chủ H5
        {
            if (f == "SYN")
            {
                PktRow(pkt, net.Now, "");
                Out(s, c, "SYN+ACK seq=0 ack=1", "SYNACK", ack: 1);
            }
            else if (f == "ACK" && state == "SYN_SENT2")
            {
                PktRow(pkt, net.Now, "ESTABLISHED");
                state = "ESTABLISHED";
            }
            else if (f == "DATA")
            {
                long seq = pkt.Seq;
                if (seq == rnext)
                {
                    rnext += Mss;
                    while (rbuf.Remove(rnext)) rnext += Mss;   // đoạn đến sớm nay đã liền mạch
                }
                else if (seq > rnext) rbuf.Add(seq);
                PktRow(pkt, net.Now, "");
                Out(s, c, $"ACK {rnext}", "ACK", ack: rnext);
            }
            else if (f == "FIN")
            {
                PktRow(pkt, net.Now, "CLOSE_WAIT");
                Out(s, c, $"ACK {pkt.Seq + 1}", "FINACK", ack: pkt.Seq + 1);
                net.At(net.Now + 1000, () => Out(s, c, "FIN seq=1", "FIN2", seq: 1));
            }
            else if (f == "LASTACK") PktRow(pkt, net.Now, "CLOSED");
            return;
        }
        // ---- phía khách H1
        if (f == "SYNACK")
        {
            if (state != "SYN_SENT")          // SYN+ACK trùng (do SYN được gửi lại)
            {
                PktRow(pkt, net.Now, "trùng, bỏ qua");
                return;
            }
            PktRow(pkt, net.Now, "");
            StopTimer();
            rto = 100_000;
            state = "SYN_SENT2";
            Out(c, s, "ACK ack=1", "ACK", ack: 1);
            tData0 = net.Now;
            SendData();
        }
        else if (f == "ACK") OnAck(pkt);
        else if (f == "FINACK") PktRow(pkt, net.Now, "FIN_WAIT_2");
        else if (f == "FIN2")
        {
            PktRow(pkt, net.Now, "TIME_WAIT");
            Out(c, s, "ACK ack=2", "LASTACK", ack: 2);
        }
    }

    void OnAck(Packet pkt)
    {
        long ack = pkt.Ack;
        if (ack > una)                                        // ACK mới
        {
            una = ack;
            nxt = Math.Max(nxt, ack);
            dup = 0;
            if (recovering)
            {
                recovering = false;
                SetCwnd(ssthresh);
            }
            else if (cwnd < ssthresh) SetCwnd(cwnd + Mss);              // khởi đầu chậm
            else SetCwnd(cwnd + (long)Mss * Mss / cwnd);              // tránh tắc nghẽn
            rto = 100_000;
            PktRow(pkt, net.Now, $"cwnd={Cw(cwnd)}");
            if (una > total)
            {
                StopTimer();
                if (state == "ESTABLISHED" || state == "SYN_SENT2")
                {
                    state = "FIN_WAIT_1";
                    tDone = net.Now;
                    Out(c, s, $"FIN seq={total + 1}", "FIN", seq: total + 1);
                }
                return;
            }
            Arm();
            SendData();
        }
        else if (ack == una)                                  // ACK trùng
        {
            dup++;
            PktRow(pkt, net.Now, $"trùng {dup}");
            if (dup == 3)
            {
                fast++;
                ssthresh = Math.Max((nxt - una) / 2, 2 * Mss);
                SetCwnd(ssthresh);
                recovering = true;
                events.Add((net.Now, 'F'));
                Event($"3 ACK TRÙNG: gửi lại nhanh, ssthresh=cwnd={Cw(cwnd)}");
                long k = (una - 1) / Mss + 1;
                sent[k]++;
                tx++;
                retx++;
                Out(c, s, $"#{k} seq={una} gửi lại", "DATA", seq: una, length: Mss,
                    drop: drops.Count(d => d == k) >= sent[k]);
            }
        }
    }

    void Loss(Packet pkt, string a, string b) => PktRow(pkt, 0, $"MẤT {a}→{b}", lost: true);

    // ---------- kết quả ----------
    public void Report()
    {
        Console.WriteLine($"Sơ đồ thời gian – trình tự ({c} khách, {s} máy chủ; thời gian ms):");
        Console.WriteLine($"{"gửi",6} {"nhận",6}  {c.PadRight(W + 1)}{s}");
        foreach (var r in rows.OrderBy(r => r.T).ThenBy(r => r.I))
            Console.WriteLine(r.Text.TrimEnd());
        Console.WriteLine();
        Chart();
        long dur = tDone - tData0;
        Console.WriteLine();
        Console.WriteLine($"Tóm tắt TCP: {total / Mss} đoạn ({total} byte), truyền {tx} lần, gửi lại {retx}");
        if (tDone == 0)                       // chưa truyền xong: không có thời gian, thông lượng để tính
        {
            Console.WriteLine($"  ({fast} lần gửi lại nhanh, {timeouts} lần hết giờ); chưa truyền xong dữ liệu,");
            Console.WriteLine("  nên không tính thời gian và thông lượng.");
            return;
        }
        Console.WriteLine($"  ({fast} lần gửi lại nhanh, {timeouts} lần hết giờ); xong sau {Fmt.T(dur)} ms,");
        Console.WriteLine($"  thông lượng ≈ {total * 8 * 1000 / dur} kbit/s (liên kết chậm nhất 10 Mbit/s,");
        Console.WriteLine("  RTT ≈ 26 ms): cửa sổ nhỏ và mất gói làm thông lượng thấp hơn");
        Console.WriteLine("  nhiều so với băng thông.");
    }

    void Chart()
    {
        const int cols = 52;
        long tEnd = CwndLog[^1][0];
        var vals = new long[cols];
        for (int col = 0; col < cols; col++)
        {
            long t = tEnd * (col + 1) / cols;
            long v = Mss;
            foreach (var e in CwndLog)
                if (e[0] <= t) v = e[1];
            vals[col] = v / Mss;
        }
        long top = vals.Max();
        Console.WriteLine($"Biểu đồ cwnd (đơn vị MSS = {Mss} byte) theo thời gian:");
        for (long y = top; y > 0; y--)
        {
            string line = string.Concat(vals.Select(v => v >= y ? "█" : " "));
            Console.WriteLine($"{y,4} │{line}");
        }
        Console.WriteLine("     └" + new string('─', cols));
        var marks = Enumerable.Repeat(' ', cols).ToArray();
        foreach (var (t, m) in events)
            marks[Math.Min(cols - 1, t * cols / Math.Max(tEnd, 1))] = m;
        Console.WriteLine("      " + new string(marks).TrimEnd());
        string right = $"{Fmt.T(tEnd)} ms";
        Console.WriteLine("      0" + right.PadLeft(cols - 1));
        Console.WriteLine("      F = gửi lại nhanh (3 ACK trùng), T = hết giờ RTO");
    }
}
