using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR4_2.DuaXe;

namespace BaiMR4_2;

// Server đua xe thời gian thực trên UDP (server có thẩm quyền, vòng tick 20 Hz).
// Mô hình:
//   - Client gửi ĐẦU VÀO (ga/nhả/phanh) kèm số thứ tự = tick mà đầu vào áp dụng; mỗi gói mang kèm 3 đầu vào
//     gần nhất nên mất một gói vẫn còn bản sao ở gói sau.
//   - Server chạy vòng lặp tick cố định 50 ms: lấy đầu vào của tick hiện tại cho từng xe; nếu không có
//     (mọi bản sao đều mất) thì GIỮ đầu vào cũ; mô phỏng vật lý; gửi ẢNH CHỤP trạng thái tới mọi client.
//   - Ảnh chụp mang số tick: client chỉ giữ ảnh mới nhất, bỏ ảnh đến muộn (cũ hơn).
//   - --mat, --dao giả lập mất gói và đảo thứ tự (theo hàm băm có hạt giống nên tái lập được).
//     Danh sách "0,20" nghĩa là cuộc đua 1 mất 0 %, cuộc đua 2 mất 20 %.
class Car(string name, IPEndPoint addr)
{
    public string Name = name;
    public IPEndPoint Addr = addr;
    public int Id = -1;
    public long X, V;
    public long Done = -1;                         // số tick từ lúc xuất phát đến khi về đích (-1: chưa)
    public Dictionary<long, int> Inputs = [];      // tick -> đầu vào đã nhận (bộ đệm đầu vào)
    public int LastIn;                             // đầu vào áp dụng gần nhất (dùng lại khi thiếu)
    public int Recv, Dropped, Held, Crashes;
    public int SnapSent, SnapDropped, SnapReordered;
    public byte[]? Delayed;                        // ảnh chụp đang bị giữ lại để gửi muộn (giả lập đảo thứ tự)
}

record RaceOptions(string Host, int Port, int SoXe, string Mat, string Dao, long Seed, int SoVan);

class RaceServer
{
    const int DirIn = 0, DirSnap = 1, DirReorder = 2;
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    public static double Now => Clock.Elapsed.TotalSeconds;

    readonly RaceOptions a;
    readonly Socket sock;
    readonly int[] mat, dao;
    public int Race;
    Dictionary<IPEndPoint, Car> cars = [];
    public string State = "CHO";                   // CHO -> DUA (gồm đếm ngược) -> XONG
    long tick = -1;
    long? endTick;
    public double? NextTick;

    public RaceServer(RaceOptions a, Socket sock)
    {
        this.a = a; this.sock = sock;
        mat = a.Mat.Split(',').Select(int.Parse).ToArray();
        dao = a.Dao.Split(',').Select(int.Parse).ToArray();
        Reset();
    }

    void Reset()
    {
        cars = [];
        State = "CHO";
        tick = -1;
        endTick = null;
        NextTick = null;
    }

    void Send(IPEndPoint addr, JsonObject obj) => sock.SendTo(Encode(obj), addr);

    // ----- nhận gói -----
    public void OnPacket(ReadOnlySpan<byte> data, IPEndPoint addr)
    {
        var p = Decode(data);
        if (p == null) return;                              // datagram hỏng: bỏ qua
        if (p.Str("t") == "join") OnJoin(p, addr);
        else if (p.Str("t") == "in") OnInput(p, addr);
    }

    void OnJoin(JsonObject p, IPEndPoint addr)
    {
        string? name = p.Str("name");
        if (name == null || name.Length < 1 || name.Length > 12) return;
        cars.TryGetValue(addr, out var car);
        if (car == null || State == "XONG")
        {
            if (State != "CHO" || cars.Values.Any(c => c.Name == name))
            {
                Send(addr, new JsonObject { ["t"] = "busy" });      // đang đua hoặc trùng tên: client thử lại sau
                return;
            }
            car = cars[addr] = new Car(name, addr);
            Console.WriteLine($"[server] Thêm một xe từ {addr.Address} ({cars.Count}/{a.SoXe})");
        }
        // trả lời cả khi client gửi lại join (welcome trước đó có thể đã mất)
        Send(addr, new JsonObject
        {
            ["t"] = "welcome", ["name"] = car.Name, ["need"] = a.SoXe, ["race"] = Race + 1,
            ["mat"] = Loss(), ["dao"] = Reorder(),
        });
        if (State == "CHO" && cars.Count == a.SoXe) StartRace();
    }

    void OnInput(JsonObject p, IPEndPoint addr)
    {
        cars.TryGetValue(addr, out var car);
        if (car == null || State != "DUA" || !TryLong(p["seq"], out long seq) || p["in"] is not JsonArray inputs)
            return;
        if (Lossy(a.Seed, DirIn, car.Id, seq, Loss()))
        {
            car.Dropped++;                                  // giả lập: gói này "mất trên đường"
            return;
        }
        car.Recv++;
        for (int j = 0; j < Math.Min(3, inputs.Count); j++)  // inputs[j] là đầu vào của tick seq - j
        {
            long t = seq - j;
            if (t > tick && TryLong(inputs[j], out long inp) && inp >= -1 && inp <= 1)   // chỉ nhận tick chưa mô phỏng
                car.Inputs.TryAdd(t, (int)inp);
        }
    }

    // ----- cuộc đua -----
    int Loss() => mat[Math.Min(Race, mat.Length - 1)];
    int Reorder() => dao[Math.Min(Race, dao.Length - 1)];
    List<Car> Ordered() => cars.Values.OrderBy(c => c.Id).ToList();

    void StartRace()
    {
        int i = 0;
        foreach (var car in cars.Values.OrderBy(c => c.Name, StringComparer.Ordinal))
            car.Id = i++;                                   // mã xe theo thứ tự tên: không phụ thuộc ai vào trước
        State = "DUA"; tick = -1;
        NextTick = Now + TickMs / 1000.0;
        string names = string.Join(", ", Ordered().Select(c => c.Name));
        Console.WriteLine($"[server] Cuộc đua {Race + 1}: {names}; mất gói {Loss()} %, " +
                          $"đảo thứ tự {Reorder()} %; xuất phát sau {Start} tick");
    }

    public void DoTick()
    {
        tick++;
        long k = tick;
        bool racing = State == "DUA" && k >= Start;
        foreach (var car in Ordered())
        {
            bool has = car.Inputs.Remove(k, out int inp);
            foreach (var t in car.Inputs.Keys.Where(t => t < k).ToList()) car.Inputs.Remove(t);   // dọn đầu vào quá hạn
            if (!has)
            {
                inp = car.LastIn;                           // thiếu đầu vào: giữ đầu vào cũ
                if (racing && car.Done < 0) car.Held++;
            }
            car.LastIn = inp;
            if (!racing || car.Done >= 0) continue;
            (car.X, car.V, bool crashed) = Step(car.X, car.V, inp);
            if (crashed)
            {
                car.Crashes++;
                Console.WriteLine($"[server] t={k - Start}: {car.Name} văng ở khúc cua (quá {Fmt2(VCua)} ô/tick)");
            }
            if (car.X >= Track)
            {
                car.X = Track; car.V = 0; car.Done = k - Start + 1;
            }
        }
        if (State == "DUA" && (cars.Values.All(c => c.Done >= 0) || k >= Start + MaxRace))
        {
            State = "XONG"; endTick = k;
            string rank = string.Join("; ", Ranking().Select((c, i) => $"{i + 1}. {c.Name} {(c.Done >= 0 ? c.Done.ToString() : "-")} tick"));
            Console.WriteLine($"[server] Cuộc đua {Race + 1} kết thúc: {rank}");
        }
        Broadcast(k);
        if (State == "XONG" && k >= endTick + EndTicks) FinishRace();
    }

    List<Car> Ranking() =>
        cars.Values.OrderBy(c => c.Done < 0).ThenBy(c => c.Done).ThenBy(c => -c.X).ThenBy(c => c.Id).ToList();

    void Broadcast(long k)
    {
        var ordered = Ordered();
        var snap = new JsonObject
        {
            ["t"] = "snap", ["race"] = Race + 1, ["tick"] = k, ["phase"] = State,
            ["names"] = new JsonArray(ordered.Select(c => (JsonNode)c.Name!).ToArray()),
            ["cars"] = new JsonArray(ordered.Select(c => (JsonNode)new JsonArray(c.Id, c.X, c.V, c.Done)).ToArray()),
        };
        if (State == "XONG")
            snap["rank"] = new JsonArray(Ranking().Select(c => (JsonNode)new JsonArray(c.Name, c.Done)).ToArray());
        byte[] data = Encode(snap);
        foreach (var car in ordered)
        {
            var prev = car.Delayed;
            car.Delayed = null;
            if (Lossy(a.Seed, DirSnap, car.Id, k, Loss())) car.SnapDropped++;
            else if (Lossy(a.Seed, DirReorder, car.Id, k, Reorder()))
            {
                car.Delayed = data;                          // giữ lại, gửi SAU ảnh chụp tick kế tiếp
                car.SnapReordered++;
            }
            else
            {
                sock.SendTo(data, car.Addr);
                car.SnapSent++;
            }
            if (prev != null)
            {
                sock.SendTo(prev, car.Addr);                 // ảnh chụp cũ đến sau ảnh mới: client phải bỏ
                car.SnapSent++;
            }
        }
    }

    void FinishRace()
    {
        foreach (var c in Ordered())
        {
            int total = c.Recv + c.Dropped;
            Console.WriteLine($"[server] {c.Name}: nhận {c.Recv}/{total} gói đầu vào (giả lập mất {c.Dropped}), " +
                              $"{c.Held} tick phải giữ đầu vào cũ, văng {c.Crashes} lần");
        }
        Race++;
        Reset();
    }
}

static class ServerDuaXe
{
    public static int Run(string[] args)
    {
        var pos = new List<string>();
        var o = new Dictionary<string, string> { ["--so-xe"] = "2", ["--mat"] = "0", ["--dao"] = "0", ["--seed"] = "1", ["--so-van"] = "0" };
        for (int i = 0; i < args.Length; i++)
        {
            if (o.ContainsKey(args[i]) && i + 1 < args.Length) o[args[i]] = args[++i];
            else pos.Add(args[i]);
        }
        var a = new RaceOptions(pos.Count > 0 ? pos[0] : "0.0.0.0", pos.Count > 1 ? int.Parse(pos[1]) : 9200,
            int.Parse(o["--so-xe"]), o["--mat"], o["--dao"], long.Parse(o["--seed"]), int.Parse(o["--so-van"]));
        var sock = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        sock.Bind(new IPEndPoint(IPAddress.Parse(a.Host), a.Port));
        Console.WriteLine($"[server] Đua xe UDP tại {a.Host}:{a.Port}, {1000 / TickMs} tick/s, cần {a.SoXe} xe, " +
                          $"đường dài {Track / 100} ô");
        bool stop = false;
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop = true; };
        var srv = new RaceServer(a, sock);
        var buf = new byte[2048];
        try
        {
            while (!stop)
            {
                // chờ gói tới hoặc tới tick kế tiếp (giống select() có thời gian chờ); tối đa 200 ms để kịp thấy Ctrl+C
                double wait = srv.NextTick is double nt ? Math.Max(0, nt - RaceServer.Now) : 0.2;
                if (sock.Poll((int)(Math.Min(wait, 0.2) * 1_000_000), SelectMode.SelectRead))
                {
                    EndPoint from = new IPEndPoint(IPAddress.Any, 0);
                    try
                    {
                        int n = sock.ReceiveFrom(buf, ref from);
                        srv.OnPacket(buf.AsSpan(0, n), (IPEndPoint)from);
                    }
                    catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset) { }
                    // Windows: ICMP "port unreachable" từ client đã thoát
                }
                if (srv.NextTick is double t && RaceServer.Now >= t)
                {
                    srv.NextTick = t + TickMs / 1000.0;      // lịch cố định: không trôi theo thời gian xử lý
                    srv.DoTick();
                    if (srv.State == "CHO" && a.SoVan != 0 && srv.Race >= a.SoVan)
                    {
                        Console.WriteLine($"[server] Đã đua {srv.Race} cuộc, dừng server");
                        return 0;
                    }
                }
            }
            Console.WriteLine("[server] Dừng server");
        }
        finally
        {
            sock.Close();
        }
        return 0;
    }
}
