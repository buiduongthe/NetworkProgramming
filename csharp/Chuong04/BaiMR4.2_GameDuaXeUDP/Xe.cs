using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR4_2.DuaXe;

namespace BaiMR4_2;

// Client đua xe UDP: người lái (bàn phím) hoặc bot.
// Mỗi khi nhận ảnh chụp tick k (chỉ nhận ảnh MỚI HƠN ảnh đang có), client:
//   1. đối chiếu: so vị trí xe mình mà client đã dự đoán với vị trí server gửi (lệch khi đầu vào bị mất);
//   2. dự đoán phía client: từ ảnh chụp k, chạy lại vật lý với các đầu vào của chính mình để biết xe
//      sẽ ở đâu ở tick k+1 (xe khác thì ngoại suy theo vận tốc cuối);
//   3. chọn đầu vào cho tick k+2 và gửi kèm 2 đầu vào trước đó (dư thừa).
// Khi một ảnh chụp bị mất, khung hình của tick đó được vẽ bằng dự đoán – người chơi không thấy xe "đứng hình".

/// <summary>Ảnh chụp trạng thái server gửi (đã giải mã từ JSON).</summary>
record Snap(long Race, long Tick, string Phase, List<string> Names, List<long[]> Cars, List<(string Name, long Done)> Rank)
{
    public static Snap From(JsonObject p) => new(p.Long("race"), p.Long("tick"), p.Str("phase") ?? "",
        p["names"]!.AsArray().Select(n => n!.GetValue<string>()).ToList(),
        p["cars"]!.AsArray().Select(c => c!.AsArray().Select(v => v!.GetValue<long>()).ToArray()).ToList(),
        p["rank"] is JsonArray r ? r.Select(x => (x![0]!.GetValue<string>(), x[1]!.GetValue<long>())).ToList() : []);

    /// <summary>[id, x, v, done] của xe cid.</summary>
    public long[] Car(int cid) => Cars.First(c => c[0] == cid);
}

/// <summary>Những gì client biết về cuộc đua: ảnh chụp mới nhất, đầu vào của mình, thống kê.</summary>
class RaceView(string me)
{
    public int? MyId;
    public long LastTick = -1;
    public Snap? S;
    readonly Dictionary<long, int> myInputs = [];   // tick -> đầu vào mình đã chọn
    long? maxInput;                                 // tick lớn nhất đã chọn đầu vào
    public int Got, Stale, Missed, MaxGap;
    public int ErrN; public long ErrMax;            // dự đoán xe mình sai (server đã dùng đầu vào khác)
    public int OerrN; public long OerrMax;          // ngoại suy xe khác sai sau một quãng hụt ảnh chụp

    /// <summary>Đầu vào mình dùng ở tick t (nếu chưa chọn thì giữ đầu vào gần nhất – giống server).</summary>
    int InputAt(long t)
    {
        for (; t >= 0; t--)
            if (myInputs.TryGetValue(t, out int v)) return v;
        return 0;
    }

    /// <summary>Dự đoán phía client: chạy lại vật lý từ ảnh chụp với chính các đầu vào của mình.</summary>
    public (long X, long V, long Done) PredictOwn(Snap s, long T)
    {
        var c = s.Car(MyId!.Value);
        long x = c[1], v = c[2], done = c[3];
        for (long t = s.Tick + 1; t <= T; t++)
        {
            if (t < Start || done >= 0) continue;
            (x, v, _) = Step(x, v, InputAt(t));
            if (x >= Track)
            {
                x = Track; v = 0; done = t - Start + 1;
            }
        }
        return (x, v, done);
    }

    /// <summary>Xe khác: ngoại suy tuyến tính (dead reckoning) theo vận tốc trong ảnh chụp.</summary>
    public static (long X, long V, long Done) PredictOther(Snap s, int cid, long T)
    {
        var c = s.Car(cid);
        long x = c[1], v = c[2], done = c[3];
        long ticks = T - Math.Max(s.Tick, Start - 1);
        if (done >= 0 || ticks <= 0) return (x, v, done);
        return (Math.Min(Track, x + v * ticks), v, done);
    }

    /// <summary>Trạng thái mọi xe ở tick T theo hiểu biết hiện tại (ảnh chụp thật nếu có, không thì dự đoán).</summary>
    List<(string Name, long X, long V, long Done)> StateAt(long T)
    {
        var s = S!;
        var outList = new List<(string, long, long, long)>();
        for (int cid = 0; cid < s.Names.Count; cid++)
        {
            long x, v, done;
            if (s.Tick == T)
            {
                var c = s.Car(cid);
                (x, v, done) = (c[1], c[2], c[3]);
            }
            else if (cid == MyId) (x, v, done) = PredictOwn(s, T);
            else (x, v, done) = PredictOther(s, cid, T);
            outList.Add((s.Names[cid], x, v, done));
        }
        return outList;
    }

    public List<string> Frame(long T, bool predicted)
    {
        var lines = new List<string> { $"[{me}] Khung t={T - Start}" + (predicted ? " (dự đoán: thiếu ảnh chụp)" : "") };
        foreach (var (name, x, v, done) in StateAt(T))
        {
            string tail = done >= 0 ? $" về đích ({done} tick)" : $" x={Fmt2(x)} v={Fmt2(v)}";
            char letter = char.ToUpperInvariant(name[0]);
            lines.Add($"  {letter}|{TrackLine(letter, x)}{tail}");
        }
        return lines;
    }

    /// <summary>Nhận một ảnh chụp; trả false nếu là ảnh cũ (đến muộn) – bị bỏ, ảnh mới nhất thắng.</summary>
    public bool Accept(Snap s)
    {
        long k = s.Tick;
        if (k <= LastTick)
        {
            Stale++;
            return false;
        }
        MyId ??= s.Names.IndexOf(me);
        if (LastTick >= 0 && k - LastTick > 1)
        {
            int gap = (int)(k - LastTick - 1);
            Missed += gap;
            MaxGap = Math.Max(MaxGap, gap);
            if (k >= Start)                         // xe khác: ngoại suy trong quãng hụt lệch bao nhiêu?
            {
                for (int cid = 0; cid < s.Names.Count; cid++)
                {
                    if (cid == MyId) continue;
                    long err = Math.Abs(PredictOther(S!, cid, k).X - s.Car(cid)[1]);
                    if (err != 0)
                    {
                        OerrN++;
                        OerrMax = Math.Max(OerrMax, err);
                    }
                }
            }
        }
        if (S != null && k >= Start)                // đối chiếu dự đoán với sự thật của server
        {
            long err = Math.Abs(PredictOwn(S, k).X - s.Car(MyId!.Value)[1]);
            if (err != 0)
            {
                ErrN++;
                ErrMax = Math.Max(ErrMax, err);
            }
        }
        Got++;
        return true;
    }

    /// <summary>Chọn đầu vào cho tick k+Lead, trả gói đầu vào cần gửi (mang kèm Window-1 đầu vào trước đó).</summary>
    public JsonObject Decide(Func<long, long, long, int> choose)
    {
        long k = S!.Tick;
        long T = k + Lead;
        var (x, v, done) = PredictOwn(S, T - 1);
        int inp = choose(x, v, done);
        long start = maxInput is long m ? m + 1 : T;
        for (long t = start; t < T; t++)            // tick bị bỏ qua (do mất ảnh chụp): giữ đầu vào cũ
            myInputs[t] = InputAt(t);
        myInputs[T] = inp;
        maxInput = maxInput is long mm ? Math.Max(mm, T) : T;
        var arr = new JsonArray();
        for (int j = 0; j < Window; j++) arr.Add(InputAt(T - j));
        return new JsonObject { ["t"] = "in", ["seq"] = T, ["in"] = arr };
    }
}

static class Xe
{
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    static double Now => Clock.Elapsed.TotalSeconds;

    /// <summary>Trạng thái người lái: đầu vào hiện tại và thời điểm nhận ảnh chụp gần nhất.</summary>
    class Human { public volatile int Inp; public double TLast = Now; }

    static void Send(Socket sock, EndPoint addr, JsonObject obj) => sock.SendTo(Encode(obj), addr);

    static JsonObject? Recv(Socket sock, byte[] buf)
    {
        EndPoint from = new IPEndPoint(IPAddress.Any, 0);
        int n = sock.ReceiveFrom(buf, ref from);
        return Decode(buf.AsSpan(0, n));
    }

    static bool IsTimeout(SocketException e) => e.SocketErrorCode == SocketError.TimedOut;

    /// <summary>Gửi join (lặp lại mỗi 250 ms vì UDP có thể mất) tới khi nhận welcome.</summary>
    static JsonObject Join(Socket sock, EndPoint addr, string me, byte[] buf)
    {
        sock.ReceiveTimeout = 250;
        while (true)
        {
            Send(sock, addr, new JsonObject { ["t"] = "join", ["name"] = me });
            JsonObject? p;
            try { p = Recv(sock, buf); }
            catch (SocketException e) when (IsTimeout(e) || e.SocketErrorCode == SocketError.ConnectionReset) { continue; }
            if (p?.Str("t") == "welcome") return p;
        }
    }

    static void RaceOnce(Socket sock, EndPoint addr, string me, string? bot, int khung, Human? human)
    {
        var buf = new byte[4096];
        var w = Join(sock, addr, me, buf);
        Console.WriteLine($"[{me}] Vào cuộc đua {w.Long("race")} (cần {w.Long("need")} xe; mất gói giả lập {w.Long("mat")} %, " +
                          $"đảo thứ tự {w.Long("dao")} %)");
        var view = new RaceView(me);
        long nextFrame = Start;
        sock.ReceiveTimeout = human != null ? 50 : 3000;
        double lastDraw = 0;
        while (true)
        {
            JsonObject? p;
            try
            {
                p = Recv(sock, buf);
            }
            catch (SocketException e) when (IsTimeout(e))
            {
                if (human == null)
                {
                    Console.WriteLine($"[{me}] Mất liên lạc với server");
                    return;
                }
                p = null;
            }
            if (p != null && p.Str("t") == "snap" && p.Long("race") == w.Long("race"))
            {
                var s = Snap.From(p);
                if (view.Accept(s))
                {
                    long k = s.Tick;
                    if (human != null) human.TLast = Now;
                    if (human == null && khung > 0 && s.Phase == "DUA" && view.S != null)
                    {
                        while (nextFrame <= k)               // khung cho mọi tick cần vẽ đến hết tick k
                        {
                            List<string> output;
                            if (nextFrame == k)
                            {
                                view.S = s;
                                output = view.Frame(k, false);
                            }
                            else output = view.Frame(nextFrame, true);   // dùng ảnh chụp cũ + dự đoán
                            Console.WriteLine(string.Join("\n", output));
                            nextFrame += khung;
                        }
                    }
                    view.S = s; view.LastTick = k;
                    if (s.Phase == "XONG")
                    {
                        string rank = string.Join(", ", s.Rank.Select((r, i) => $"{i + 1}. {r.Name} {(r.Done >= 0 ? r.Done.ToString() : "-")} tick"));
                        Console.WriteLine($"[{me}] Kết quả: {rank}");
                        Console.WriteLine($"[{me}] Ảnh chụp: nhận {view.Got}, hụt {view.Missed} tick (dài nhất {view.MaxGap}), " +
                                          $"bỏ {view.Stale} ảnh cũ");
                        Console.WriteLine($"[{me}] Dự đoán xe mình: lệch {view.ErrN} lần (lớn nhất {Fmt2(view.ErrMax)} ô); " +
                                          $"ngoại suy xe khác: lệch {view.OerrN} lần (lớn nhất {Fmt2(view.OerrMax)} ô)");
                        return;
                    }
                    Func<long, long, long, int> choose = human != null
                        ? (_, _, _) => human.Inp
                        : (x, v, done) => done < 0 ? BotInput(x, v, bot!) : 0;
                    Send(sock, addr, view.Decide(choose));
                }
            }
            if (human != null && view.S != null && Now - lastDraw >= 0.1)    // vẽ lại 10 lần/giây
            {
                lastDraw = Now;
                long est = view.LastTick + (long)((Now - human.TLast) * 1000 / TickMs);
                if (est >= Start)
                {
                    Console.WriteLine("\u001b[H\u001b[2J" + string.Join("\n", view.Frame(est, est != view.LastTick)));
                    Console.WriteLine($"Đầu vào: {InputText[human.Inp]}  (g = ga, n = nhả, p = phanh, Enter)");
                }
                else
                    Console.WriteLine($"\u001b[H\u001b[2JXuất phát sau {((Start - est) * TickMs / 1000.0).ToString("F1", CultureInfo.InvariantCulture)} s");
            }
        }
    }

    /// <summary>Luồng đọc bàn phím: g/n/p + Enter đổi đầu vào hiện tại.</summary>
    static void Keyboard(Human state)
    {
        string? line;
        while ((line = Console.ReadLine()) != null)
        {
            string c = line.Trim().ToLowerInvariant();
            if (c.StartsWith('g')) state.Inp = 1;
            else if (c.StartsWith('n')) state.Inp = 0;
            else if (c.StartsWith('p')) state.Inp = -1;
        }
    }

    public static int Run(string[] args)
    {
        // <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]
        var pos = new List<string>();
        string? bot = null; int khung = 10, soVan = 1;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--bot": bot = args[++i]; break;
                case "--khung": khung = int.Parse(args[++i]); break;
                case "--so-van": soVan = int.Parse(args[++i]); break;
                default: pos.Add(args[i]); break;
            }
        }
        if (pos.Count < 1 || (bot != null && !Styles.ContainsKey(bot)))
        {
            Console.WriteLine("Cách dùng: dotnet run -- xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]");
            return 2;
        }
        string me = pos[0];
        var addr = new IPEndPoint(IPAddress.Parse(pos.Count > 1 ? pos[1] : "127.0.0.1"), pos.Count > 2 ? int.Parse(pos[2]) : 9200);
        using var sock = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        Human? human = null;
        if (bot == null)
        {
            human = new Human();
            new Thread(() => Keyboard(human)) { IsBackground = true }.Start();
        }
        for (int i = 0; i < soVan; i++) RaceOnce(sock, addr, me, bot, khung, human);
        return 0;
    }
}
