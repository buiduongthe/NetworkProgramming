using System.Collections.Concurrent;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace BaiMR3_3;

// Server trò chơi đoán số nhiều người chơi trên TCP.
// Server chọn một số bí mật; người chơi lần lượt đoán, server trả lời "thấp quá", "cao quá" hoặc "đúng".
// Thiết kế:
//   - Mỗi kết nối có một luồng đọc (reader) chỉ làm một việc: nhận khung JSON và đẩy sự kiện vào hàng đợi.
//   - MỘT luồng trò chơi (vòng lặp sự kiện) giữ toàn bộ trạng thái, xử lý tuần tự từng sự kiện,
//     nên không cần khóa (lock). Hết giờ mỗi lượt = thời gian chờ của TryTake().
//   - Máy trạng thái của server: CHO (chờ đủ người) -> DANG_CHOI -> (kết thúc ván) -> CHO.
//   - Server là bên quyết định: kiểm tra mọi thông điệp (đúng lượt, kiểu số, khoảng hợp lệ), không tin client.
record ServerOptions(string Host, int Port, int SoNguoi, int HetGioMs, long Seed, long Min, long Max, int SoVan);

/// <summary>Sự kiện cho vòng trò chơi: open / msg / closed / stop.</summary>
record GameEvent(string Kind, Socket? Conn, object? Data);

class Game(ServerOptions a)
{
    public const string Cho = "CHO", DangChoi = "DANG_CHOI";       // hai trạng thái của server
    public static readonly Dictionary<string, string> HintText = new()
        { ["THAP"] = "thấp quá", ["CAO"] = "cao quá", ["DUNG"] = "ĐÚNG" };
    static readonly Dictionary<string, string> Errors = new()
    {
        ["BAD_JSON"] = "Khung không phải đối tượng JSON",
        ["NEED_HELLO"] = "Phải gửi hello trước",
        ["BAD_NAME"] = "Tên phải là chuỗi 1–16 ký tự",
        ["NAME_TAKEN"] = "Tên đã có người dùng",
        ["GAME_IN_PROGRESS"] = "Ván đang diễn ra, hãy vào lại sau",
        ["GAME_NOT_STARTED"] = "Ván chưa bắt đầu",
        ["NOT_YOUR_TURN"] = "Chưa đến lượt bạn",
        ["BAD_VALUE"] = "Giá trị đoán phải là số nguyên",
        ["OUT_OF_RANGE"] = "Số đoán nằm ngoài khoảng cho phép",
        ["UNKNOWN_TYPE"] = "Loại thông điệp không hỗ trợ",
    };
    static readonly Stopwatch Clock = Stopwatch.StartNew();
    public static double Now => Clock.Elapsed.TotalSeconds;          // đồng hồ đơn điệu (giây)

    readonly Lcg rng = new(a.Seed != 0 ? a.Seed : DateTimeOffset.UtcNow.ToUnixTimeSeconds());
    public string State = Cho;
    // conn -> tên; OrderedDictionary giữ đúng thứ tự vào phòng (như dict của Python) kể cả sau khi xóa
    readonly OrderedDictionary<Socket, string> names = new();
    List<Socket> order = [];              // thứ tự lượt
    int turn;                             // chỉ số người đang có lượt trong order
    int turnNo;
    public double? Deadline;              // hạn chót của lượt hiện tại (giây theo Now)
    long secret, lo = a.Min, hi = a.Max;
    public int Round;

    public bool Has(Socket c) => names.ContainsKey(c);

    // ----- gửi -----
    void Send(Socket conn, JsonObject obj)
    {
        try { GiaoThuc.SendJson(conn, obj); }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }   // luồng đọc sẽ báo "closed"
    }

    void Broadcast(JsonObject obj)
    {
        foreach (var conn in names.Keys.ToList()) Send(conn, (JsonObject)obj.DeepClone());
    }

    void Error(Socket conn, string code, string who, string extra = "")
    {
        Console.WriteLine($"[server] Từ chối {who}: {code}{extra}");
        Send(conn, new JsonObject { ["type"] = "error", ["code"] = code, ["msg"] = Errors[code] });
    }

    /// <summary>Đóng hẳn kết nối (luồng đọc sẽ thoát).</summary>
    void Drop(Socket conn)
    {
        names.Remove(conn);
        try { conn.Shutdown(SocketShutdown.Both); } catch (Exception) { }
        conn.Close();
    }

    // ----- máy trạng thái -----
    void StartRound()
    {
        Round++;
        State = DangChoi;
        secret = rng.NextInt(a.Min, a.Max);
        lo = a.Min; hi = a.Max;
        order = names.Keys.ToList();
        turn = 0; turnNo = 0;
        var orderNames = order.Select(c => names[c]).ToList();
        Console.WriteLine($"[server] Bắt đầu ván {Round}: số bí mật = {secret} (chỉ server biết), " +
                          $"thứ tự: {string.Join(", ", orderNames)}");
        Broadcast(new JsonObject
        {
            ["type"] = "start", ["round"] = Round, ["order"] = new JsonArray(orderNames.Select(n => (JsonNode)n!).ToArray()),
            ["min"] = a.Min, ["max"] = a.Max,
        });
        NextTurn();
    }

    /// <summary>Giao lượt cho người kế tiếp và đặt hạn chót.</summary>
    void NextTurn()
    {
        turnNo++;
        string name = names[order[turn]];
        Deadline = Now + a.HetGioMs / 1000.0;
        Broadcast(new JsonObject
        {
            ["type"] = "turn", ["player"] = name, ["lo"] = lo, ["hi"] = hi,
            ["turn_no"] = turnNo, ["timeout_ms"] = a.HetGioMs,
        });
    }

    void Advance()
    {
        turn = (turn + 1) % order.Count;
        NextTurn();
    }

    void EndRound(string? winner)
    {
        if (winner != null) Console.WriteLine($"[server] Kết thúc ván {Round}: {winner} thắng sau {turnNo} lượt");
        else Console.WriteLine($"[server] Kết thúc ván {Round}: không còn người chơi");
        Broadcast(new JsonObject { ["type"] = "game_over", ["winner"] = winner, ["secret"] = secret, ["turns"] = turnNo });
        foreach (var conn in names.Keys.ToList())                  // mỗi ván một phiên: đóng mọi kết nối
        {
            Send(conn, new JsonObject { ["type"] = "bye" });
            Drop(conn);
        }
        State = Cho; order = []; Deadline = null;
    }

    public void RemovePlayer(Socket conn)
    {
        string name = names[conn];
        names.Remove(conn);
        Console.WriteLine($"[server] {name} rời trò chơi (còn {names.Count} người)");
        Broadcast(new JsonObject { ["type"] = "left", ["player"] = name });
        if (State != DangChoi) return;
        int i = order.IndexOf(conn);
        bool wasCurrent = i == turn;
        order.RemoveAt(i);
        if (order.Count == 0)
        {
            EndRound(null);
            return;
        }
        if (i < turn) turn--;                                      // giữ nguyên người đang có lượt
        if (wasCurrent)
        {
            turn %= order.Count;
            NextTurn();
        }
    }

    public void OnTimeout()
    {
        string name = names[order[turn]];
        Console.WriteLine($"[server] {name} hết giờ ở lượt {turnNo}");
        Broadcast(new JsonObject { ["type"] = "timeout", ["player"] = name });
        Advance();
    }

    // ----- xử lý thông điệp -----
    public void OnMessage(Socket conn, string addr, JsonObject? msg)
    {
        string who = names.TryGetValue(conn, out var nm) ? nm : addr;
        if (msg == null)
        {
            Error(conn, "BAD_JSON", who);
            return;
        }
        string? kind = msg.Str("type");
        if (!names.ContainsKey(conn))                             // chưa vào phòng: chỉ chấp nhận hello
        {
            if (kind != "hello") Error(conn, "NEED_HELLO", who);
            else OnHello(conn, addr, msg);
            return;
        }
        if (kind == "guess")
        {
            OnGuess(conn, who, msg["value"]);
            return;
        }
        if (kind == "bye")
        {
            RemovePlayer(conn);
            Drop(conn);
            return;
        }
        Error(conn, "UNKNOWN_TYPE", who, $" ({GiaoThuc.Repr(msg["type"])})");
    }

    void OnHello(Socket conn, string addr, JsonObject msg)
    {
        string? name = msg.Str("name")?.Trim();
        if (name == null || name.Length < 1 || name.Length > 16)
        {
            Error(conn, "BAD_NAME", addr);
            return;
        }
        if (names.Values.Contains(name))
        {
            Error(conn, "NAME_TAKEN", addr, $" ({name})");
            return;
        }
        if (State == DangChoi)
        {
            Error(conn, "GAME_IN_PROGRESS", name);
            Send(conn, new JsonObject { ["type"] = "bye" });
            Drop(conn);
            return;
        }
        names[conn] = name;
        Console.WriteLine($"[server] {name} vào phòng chờ ({names.Count}/{a.SoNguoi})");
        Send(conn, new JsonObject
        {
            ["type"] = "welcome", ["name"] = name, ["waiting"] = names.Count,
            ["need"] = a.SoNguoi, ["min"] = a.Min, ["max"] = a.Max,
        });
        if (names.Count >= a.SoNguoi) StartRound();
    }

    void OnGuess(Socket conn, string who, JsonNode? valueNode)
    {
        if (State != DangChoi)
        {
            Error(conn, "GAME_NOT_STARTED", who);
            return;
        }
        if (order[turn] != conn)
        {
            Error(conn, "NOT_YOUR_TURN", who);
            return;
        }
        if (!GiaoThuc.TryInt(valueNode, out long value))           // true/false, chuỗi, số thực đều bị từ chối
        {
            Error(conn, "BAD_VALUE", who, $" ({GiaoThuc.Repr(valueNode)})");
            return;
        }
        if (value < lo || value > hi)
        {
            Error(conn, "OUT_OF_RANGE", who, $" ({value} ngoài {lo}–{hi})");
            return;
        }
        string hint = value == secret ? "DUNG" : value < secret ? "THAP" : "CAO";
        Console.WriteLine($"[server] Lượt {turnNo}: {who} đoán {value} -> {HintText[hint]}");
        Broadcast(new JsonObject { ["type"] = "result", ["player"] = who, ["value"] = value, ["hint"] = hint });
        if (hint == "DUNG")
        {
            EndRound(who);
            return;
        }
        if (hint == "THAP") lo = value + 1;
        else hi = value - 1;
        Advance();
    }
}

static class ServerDoanSo
{
    public static int Run(string[] args)
    {
        // đọc tham số giống argparse: host port [--so-nguoi N] [--het-gio-ms N] [--seed N] [--min N] [--max N] [--so-van N]
        var pos = new List<string>();
        var opt = new Dictionary<string, long> { ["--so-nguoi"] = 2, ["--het-gio-ms"] = 30000, ["--seed"] = 0,
                                                 ["--min"] = 1, ["--max"] = 100, ["--so-van"] = 0 };
        for (int i = 0; i < args.Length; i++)
        {
            if (opt.ContainsKey(args[i]) && i + 1 < args.Length) opt[args[i]] = long.Parse(args[++i]);
            else pos.Add(args[i]);
        }
        var a = new ServerOptions(pos.Count > 0 ? pos[0] : "0.0.0.0", pos.Count > 1 ? int.Parse(pos[1]) : 9000,
            (int)opt["--so-nguoi"], (int)opt["--het-gio-ms"], opt["--seed"], opt["--min"], opt["--max"], (int)opt["--so-van"]);

        var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(a.Host), a.Port));
        srv.Listen(16);
        Console.WriteLine($"[server] Trò chơi đoán số tại {a.Host}:{a.Port}; cần {a.SoNguoi} người; " +
                          $"mỗi lượt tối đa {a.HetGioMs} ms");
        var events = new BlockingCollection<GameEvent>();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; events.Add(new GameEvent("stop", null, null)); };
        new Thread(() => Acceptor(srv, events)) { IsBackground = true }.Start();
        var game = new Game(a);
        var addrs = new Dictionary<Socket, string>();               // conn -> "ip:port"
        try
        {
            while (true)
            {
                if (game.Deadline is double d0 && Game.Now >= d0)
                    game.OnTimeout();      // kiểm tra trước: thông điệp đến liên tục không "hoãn" được hết giờ
                int timeoutMs = Timeout.Infinite;
                if (game.Deadline is double d) timeoutMs = (int)Math.Max(0, Math.Ceiling((d - Game.Now) * 1000));
                if (!events.TryTake(out var ev, timeoutMs))         // hết giờ của lượt hiện tại
                {
                    game.OnTimeout();
                    continue;
                }
                var conn = ev.Conn!;
                switch (ev.Kind)
                {
                    case "stop":
                        Console.WriteLine("[server] Dừng server");
                        return 0;
                    case "open":
                        addrs[conn] = (string)ev.Data!;
                        Console.WriteLine($"[server] {ev.Data} kết nối");
                        break;
                    case "msg":
                        game.OnMessage(conn, addrs.GetValueOrDefault(conn, "?"), ev.Data as JsonObject);
                        break;
                    case "closed":
                        addrs.Remove(conn);
                        if (game.Has(conn)) game.RemovePlayer(conn);         // mất kết nối giữa chừng
                        conn.Close();
                        break;
                }
                if (a.SoVan != 0 && game.Round >= a.SoVan && game.State == Game.Cho)
                {
                    Console.WriteLine($"[server] Đã chơi {game.Round} ván, dừng server");
                    break;
                }
            }
        }
        finally
        {
            srv.Close();
        }
        return 0;
    }

    /// <summary>Luồng chấp nhận kết nối: mỗi client mới có một luồng đọc riêng.</summary>
    static void Acceptor(Socket srv, BlockingCollection<GameEvent> events)
    {
        while (true)
        {
            Socket conn;
            try { conn = srv.Accept(); }
            catch (Exception) { return; }                             // socket nghe đã đóng
            var ep = (IPEndPoint)conn.RemoteEndPoint!;
            conn.NoDelay = true;                                      // tắt Nagle: phản hồi nhỏ đi ngay
            events.Add(new GameEvent("open", conn, $"{ep.Address}:{ep.Port}"));
            new Thread(() => Reader(conn, events)) { IsBackground = true }.Start();
        }
    }

    /// <summary>Luồng đọc của một kết nối: chuyển mỗi khung thành sự kiện ("msg", conn, obj).</summary>
    static void Reader(Socket conn, BlockingCollection<GameEvent> events)
    {
        try
        {
            while (true)
            {
                var obj = GiaoThuc.DecodeJson(GiaoThuc.RecvFrame(conn));
                events.Add(new GameEvent("msg", conn, obj));        // obj = null nghĩa là khung hỏng
            }
        }
        catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException ||
                                  e is ObjectDisposedException)
        {
            events.Add(new GameEvent("closed", conn, e.Message));
        }
    }
}
