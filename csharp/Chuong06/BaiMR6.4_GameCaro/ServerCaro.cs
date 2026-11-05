using System.Collections.Concurrent;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;
using static BaiMR6_4.BanCo;
using static BaiMR6_4.GiaoThuc;

namespace BaiMR6_4;

// Server Caro nhiều phòng trên TCP, khám phá trong LAN bằng UDP broadcast.
// Thiết kế (mở rộng từ MR3.3):
//   - Luồng chấp nhận + một luồng đọc cho mỗi kết nối: chỉ nhận khung JSON, đẩy sự kiện vào hàng đợi.
//   - Luồng khám phá (RESPONDER của Bài 6.5): nhận yêu cầu discover trên UDP, đẩy sự kiện vào hàng đợi.
//   - MỘT luồng trò chơi giữ toàn bộ trạng thái (sảnh, các phòng, bàn cờ), xử lý tuần tự từng sự kiện.
//     Hạn chót của nước đi trong mọi phòng được gom lại: chờ hàng đợi đến hạn chót sớm nhất.
//   - Server là trọng tài: kiểm tra lượt, tọa độ, ô trống, phát hiện thắng; client chỉ hiển thị.
//   - Mỗi nước đi hợp lệ được phát (broadcast) tới cả hai người chơi và mọi người xem trong phòng.
// Trạng thái của một phòng: CHO (thiếu người) -> DANG_CHOI -> KET_THUC -(cả hai xin chơi lại)-> DANG_CHOI.
record CaroOptions(string Host, int Port, int KhamPha, string Ten, int HetGioMs);
record CaroEvent(string Kind, object Key, object? Data);

class Client(Socket conn, string addr)
{
    public readonly Socket Conn = conn;
    public readonly string Addr = addr;
    public string? Name;
    public string? Room;              // tên phòng đang ở
    public string? Role;              // "X", "O" hoặc "xem"
}

class Room(string name)
{
    public const string Cho = "CHO", DangChoi = "DANG_CHOI", KetThuc = "KET_THUC";
    public readonly string Name = name;
    public readonly Dictionary<string, Client?> Seats = new() { ["X"] = null, ["O"] = null };   // hai người chơi
    public readonly List<Client> Watchers = [];
    public char[][] Board = NewBoard();
    public string State = Cho;
    public string Turn = "X";
    public int Moves, GameNo;
    public double? Deadline;
    public HashSet<string> Rematch = [];
    public (int, int)? Last;

    public List<Client> Members() => new[] { Seats["X"], Seats["O"] }.OfType<Client>().Concat(Watchers).ToList();

    public JsonObject Summary() => new()
    {
        ["room"] = Name, ["x"] = Seats["X"]?.Name, ["o"] = Seats["O"]?.Name, ["watchers"] = Watchers.Count, ["state"] = State,
    };
}

class CaroServer(CaroOptions a, Socket udp)
{
    public static readonly Dictionary<string, string> ReasonText = new()
    {
        ["five"] = "5 quân liên tiếp", ["timeout"] = "đối thủ hết giờ", ["left"] = "đối thủ rời phòng", ["draw"] = "hòa (kín bàn)",
    };
    static readonly Dictionary<string, string> Errors = new()
    {
        ["BAD_JSON"] = "Khung không phải đối tượng JSON",
        ["NEED_HELLO"] = "Phải gửi hello trước",
        ["BAD_NAME"] = "Tên phải là chuỗi 1–16 ký tự",
        ["NAME_TAKEN"] = "Tên đã có người dùng",
        ["UNKNOWN_TYPE"] = "Loại thông điệp không hỗ trợ",
        ["ALREADY_IN_ROOM"] = "Bạn đang ở trong một phòng",
        ["BAD_ROOM_NAME"] = "Tên phòng phải là chuỗi 1–16 ký tự",
        ["ROOM_EXISTS"] = "Phòng đã tồn tại",
        ["NO_SUCH_ROOM"] = "Không có phòng này",
        ["ROOM_FULL"] = "Phòng đã đủ hai người chơi",
        ["NOT_IN_ROOM"] = "Bạn chưa vào phòng nào",
        ["NOT_PLAYER"] = "Người xem không được đánh",
        ["GAME_NOT_RUNNING"] = "Ván chưa bắt đầu hoặc đã kết thúc",
        ["NOT_YOUR_TURN"] = "Chưa đến lượt bạn",
        ["BAD_MOVE"] = "Nước đi phải có r, c là số nguyên",
        ["OUT_OF_BOARD"] = "Ô nằm ngoài bàn cờ",
        ["CELL_TAKEN"] = "Ô đã có quân",
    };
    static readonly Stopwatch clock = Stopwatch.StartNew();
    public static double Now => clock.Elapsed.TotalSeconds;

    public readonly Dictionary<Socket, Client> Clients = [];
    readonly OrderedDictionary<string, Room> rooms = new();   // tên -> phòng (giữ thứ tự tạo)

    static string Other(string m) => m == "X" ? "O" : "X";

    // ----- gửi -----
    void Send(Client cl, JsonObject obj)
    {
        try { SendJson(cl.Conn, obj); }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
    }

    void ToRoom(Room room, JsonObject obj)
    {
        foreach (var cl in room.Members()) Send(cl, (JsonObject)obj.DeepClone());
    }

    void Error(Client cl, string code, string extra = "")
    {
        Console.WriteLine($"[server] Từ chối {cl.Name ?? cl.Addr}: {code}{extra}");
        Send(cl, new JsonObject { ["type"] = "error", ["code"] = code, ["msg"] = Errors[code] });
    }

    JsonArray RoomList() => new(rooms.Values.Select(r => (JsonNode)r.Summary()).ToArray());

    // ----- ván cờ -----
    void StartGame(Room room)
    {
        room.GameNo++;
        (room.Board, room.Turn, room.Moves, room.Last) = (NewBoard(), "X", 0, null);
        (room.State, room.Rematch) = (Room.DangChoi, []);
        room.Deadline = Now + a.HetGioMs / 1000.0;
        string x = room.Seats["X"]!.Name!, o = room.Seats["O"]!.Name!;
        Console.WriteLine($"[server] Phòng {room.Name}, ván {room.GameNo}: X = {x}, O = {o}");
        ToRoom(room, new JsonObject
        {
            ["type"] = "start", ["room"] = room.Name, ["game_no"] = room.GameNo, ["x"] = x, ["o"] = o,
            ["size"] = N, ["timeout_ms"] = a.HetGioMs,
        });
    }

    void Finish(Room room, string? winnerMark, string reason, List<(int R, int C)>? line = null)
    {
        (room.State, room.Deadline) = (Room.KetThuc, null);
        string? winner = winnerMark != null ? room.Seats[winnerMark]?.Name : null;
        string result = winner != null ? $"{winner} ({winnerMark}) thắng" : "hòa";
        Console.WriteLine($"[server] Phòng {room.Name}, ván {room.GameNo} kết thúc sau {room.Moves} nước: {result} – {ReasonText[reason]}");
        ToRoom(room, new JsonObject
        {
            ["type"] = "over", ["room"] = room.Name, ["game_no"] = room.GameNo, ["winner"] = winner,
            ["mark"] = winnerMark, ["reason"] = reason, ["moves"] = room.Moves,
            ["line"] = new JsonArray((line ?? []).Select(p => (JsonNode)new JsonArray(p.R, p.C)).ToArray()),
        });
    }

    void OnMove(Client cl, JsonObject msg)
    {
        if (cl.Room == null || !rooms.TryGetValue(cl.Room, out var room)) { Error(cl, "NOT_IN_ROOM"); return; }
        if (cl.Role == "xem") { Error(cl, "NOT_PLAYER"); return; }
        if (room.State != Room.DangChoi) { Error(cl, "GAME_NOT_RUNNING"); return; }
        if (cl.Role != room.Turn) { Error(cl, "NOT_YOUR_TURN"); return; }
        if (!TryInt(msg["r"], out int r) || !TryInt(msg["c"], out int c)) { Error(cl, "BAD_MOVE"); return; }
        if (!Inside(r, c)) { Error(cl, "OUT_OF_BOARD", $" ({r}, {c})"); return; }
        if (room.Board[r][c] != Empty) { Error(cl, "CELL_TAKEN", $" ({CellName(r, c)})"); return; }
        room.Board[r][c] = cl.Role[0];                     // nước đi hợp lệ: ghi vào bàn cờ của server
        room.Moves++;
        room.Last = (r, c);
        var line = FiveLine(room.Board, r, c);              // phán xử TRƯỚC khi báo nước đi
        bool ended = line != null || room.Moves == N * N;
        string? nxt = ended ? null : Other(cl.Role);
        ToRoom(room, new JsonObject
        {
            ["type"] = "moved", ["room"] = room.Name, ["r"] = r, ["c"] = c, ["mark"] = cl.Role,
            ["by"] = cl.Name, ["move_no"] = room.Moves, ["next"] = nxt,     // next = null: ván đã hết
        });
        if (line != null) { Finish(room, cl.Role, "five", line); return; }
        if (ended) { Finish(room, null, "draw"); return; }
        room.Turn = nxt!;
        room.Deadline = Now + a.HetGioMs / 1000.0;          // mỗi nước có hạn chót riêng
    }

    public void OnTimeout(Room room)
    {
        var loser = room.Seats[room.Turn]!;
        Console.WriteLine($"[server] Phòng {room.Name}: {loser.Name} ({room.Turn}) hết giờ");
        Finish(room, Other(room.Turn), "timeout");
    }

    // ----- phòng -----
    void Enter(Client cl, Room room, string role)
    {
        (cl.Room, cl.Role) = (room.Name, role);
        if (role == "xem") room.Watchers.Add(cl);
        else room.Seats[role] = cl;
        Console.WriteLine($"[server] {cl.Name} vào phòng {room.Name} ({(role == "xem" ? "người xem" : "quân " + role)})");
        Send(cl, new JsonObject { ["type"] = "joined", ["room"] = room.Name, ["role"] = role, ["info"] = room.Summary() });
        if (role == "xem" && room.State != Room.Cho)        // người xem đến muộn: gửi ảnh chụp bàn cờ
            Send(cl, new JsonObject
            {
                ["type"] = "board", ["room"] = room.Name, ["game_no"] = room.GameNo,
                ["rows"] = new JsonArray(room.Board.Select(row => (JsonNode)new string(row)).ToArray()), ["moves"] = room.Moves,
                ["turn"] = room.State == Room.DangChoi ? room.Turn : null, ["state"] = room.State,
            });
    }

    void LeaveRoom(Client cl)
    {
        if (cl.Room == null || !rooms.TryGetValue(cl.Room, out var room)) return;
        string role = cl.Role!;
        (cl.Room, cl.Role) = (null, null);
        if (role == "xem")
        {
            room.Watchers.Remove(cl);
            return;
        }
        if (room.State == Room.DangChoi) Finish(room, Other(role), "left");   // bỏ dở ván: đối thủ thắng
        room.Seats[role] = null;
        Console.WriteLine($"[server] {cl.Name} rời phòng {room.Name}");
        ToRoom(room, new JsonObject { ["type"] = "left", ["room"] = room.Name, ["player"] = cl.Name, ["role"] = role });
        (room.State, room.Deadline, room.Rematch) = (Room.Cho, null, []);
        if (room.Seats["X"] == null && room.Seats["O"] == null)
        {
            Console.WriteLine($"[server] Phòng {room.Name} đóng (không còn người chơi)");
            foreach (var w in room.Watchers)
            {
                (w.Room, w.Role) = (null, null);
                Send(w, new JsonObject { ["type"] = "room_closed", ["room"] = room.Name });
            }
            rooms.Remove(room.Name);
        }
    }

    // ----- xử lý thông điệp -----
    public void OnMessage(Client cl, JsonObject? msg)
    {
        if (msg == null) { Error(cl, "BAD_JSON"); return; }
        string? kind = msg.Str("type");
        if (cl.Name == null)
        {
            if (kind != "hello") { Error(cl, "NEED_HELLO"); return; }
            string? name = msg.Str("name")?.Trim();
            if (name == null || name.Length < 1 || name.Length > 16) { Error(cl, "BAD_NAME"); return; }
            if (Clients.Values.Any(o => o.Name == name)) { Error(cl, "NAME_TAKEN", $" ({name})"); return; }
            cl.Name = name;
            Console.WriteLine($"[server] {cl.Name} vào sảnh ({cl.Addr})");
            Send(cl, new JsonObject { ["type"] = "welcome", ["name"] = cl.Name, ["rooms"] = RoomList() });
            return;
        }
        switch (kind)
        {
            case "list": Send(cl, new JsonObject { ["type"] = "rooms", ["rooms"] = RoomList() }); return;
            case "create" or "join" or "watch": OnRoomRequest(cl, kind, msg.Str("room")); return;
            case "move": OnMove(cl, msg); return;
            case "rematch": OnRematch(cl); return;
            case "leave":
                if (cl.Room == null) Error(cl, "NOT_IN_ROOM");
                else LeaveRoom(cl);
                return;
            case "bye":
                LeaveRoom(cl);
                Console.WriteLine($"[server] {cl.Name} tạm biệt");
                cl.Name = null;                             // không còn trong sảnh; "closed" đến sau sẽ im lặng
                try { cl.Conn.Shutdown(SocketShutdown.Both); } catch (SocketException) { }   // luồng đọc sẽ báo "closed"
                return;
        }
        Error(cl, "UNKNOWN_TYPE", $" ({Repr(msg["type"])})");
    }

    void OnRoomRequest(Client cl, string kind, string? name)
    {
        if (cl.Room != null) { Error(cl, "ALREADY_IN_ROOM"); return; }
        name = name?.Trim();
        if (name == null || name.Length < 1 || name.Length > 16) { Error(cl, "BAD_ROOM_NAME"); return; }
        rooms.TryGetValue(name, out var room);
        if (kind == "create")
        {
            if (room != null) { Error(cl, "ROOM_EXISTS", $" ({name})"); return; }
            room = rooms[name] = new Room(name);
            Console.WriteLine($"[server] {cl.Name} tạo phòng {name}");
            Enter(cl, room, "X");
            return;
        }
        if (room == null) { Error(cl, "NO_SUCH_ROOM", $" ({name})"); return; }
        if (kind == "watch") { Enter(cl, room, "xem"); return; }
        var free = new[] { "X", "O" }.Where(m => room.Seats[m] == null).ToList();
        if (free.Count == 0) { Error(cl, "ROOM_FULL", $" ({name})"); return; }
        Enter(cl, room, free[0]);
        if (room.Seats["X"] != null && room.Seats["O"] != null) StartGame(room);
    }

    void OnRematch(Client cl)
    {
        if (cl.Room == null || !rooms.TryGetValue(cl.Room, out var room) || (cl.Role != "X" && cl.Role != "O"))
        {
            Error(cl, "NOT_PLAYER");
            return;
        }
        if (room.State != Room.KetThuc) { Error(cl, "GAME_NOT_RUNNING"); return; }
        room.Rematch.Add(cl.Role);
        if (room.Rematch.Count < 2)
        {
            ToRoom(room, new JsonObject { ["type"] = "rematch_wait", ["room"] = room.Name, ["by"] = cl.Name });
            return;
        }
        (room.Seats["X"], room.Seats["O"]) = (room.Seats["O"], room.Seats["X"]);   // đổi màu quân: người kia đi trước
        foreach (var m in new[] { "X", "O" }) room.Seats[m]!.Role = m;
        StartGame(room);
    }

    public void OnDiscover(IPEndPoint addr, JsonObject q)
    {
        int players = Clients.Values.Count(c => c.Name != null);
        var reply = new JsonObject { ["id"] = q["id"]?.DeepClone(), ["name"] = a.Ten, ["port"] = a.Port, ["rooms"] = rooms.Count, ["players"] = players };
        Console.WriteLine($"[server] Trả lời discover từ {addr.Address}:{addr.Port}");
        udp.SendTo(Encoding.UTF8.GetBytes(reply.ToJsonString(Opt)), addr);
    }

    public void OnClosed(Socket conn)
    {
        if (!Clients.Remove(conn, out var cl)) return;
        if (cl.Name != null)
        {
            LeaveRoom(cl);
            Console.WriteLine($"[server] {cl.Name} ngắt kết nối");
        }
        conn.Close();
    }

    /// <summary>Phòng có hạn chót sớm nhất (hoặc null) và số giây còn lại.</summary>
    public (Room? Room, double Left) ExpiredRoom()
    {
        var r = rooms.Values.Where(x => x.Deadline != null).MinBy(x => x.Deadline!.Value);
        return r == null ? (null, 0) : (r, r.Deadline!.Value - Now);
    }
}

static class ServerCaro
{
    public static int Run(string[] args)
    {
        var pos = new List<string>();
        var o = new Dictionary<string, string> { ["--kham-pha"] = "9999", ["--ten"] = "Caro", ["--het-gio-ms"] = "30000" };
        for (int i = 0; i < args.Length; i++)
        {
            if (o.ContainsKey(args[i]) && i + 1 < args.Length) o[args[i]] = args[++i];
            else pos.Add(args[i]);
        }
        var a = new CaroOptions(pos.Count > 0 ? pos[0] : "0.0.0.0", pos.Count > 1 ? int.Parse(pos[1]) : 9100,
                                int.Parse(o["--kham-pha"]), o["--ten"], int.Parse(o["--het-gio-ms"]));
        var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(a.Host), a.Port));
        srv.Listen(16);
        var udp = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        udp.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        udp.Bind(new IPEndPoint(IPAddress.Any, a.KhamPha));            // nhận broadcast: phải bind 0.0.0.0
        Console.WriteLine($"[server] {a.Ten}: Caro {N}×{N} tại {a.Host}:{a.Port}, khám phá UDP cổng {a.KhamPha}, " +
                          $"mỗi nước tối đa {a.HetGioMs} ms");
        var events = new BlockingCollection<CaroEvent>();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; events.Add(new CaroEvent("stop", "", null)); };
        void Bg(Action f) => new Thread(() => f()) { IsBackground = true }.Start();
        Bg(() =>                                                       // luồng chấp nhận
        {
            while (true)
            {
                Socket conn;
                try { conn = srv.Accept(); } catch (Exception) { return; }
                var ep = (IPEndPoint)conn.RemoteEndPoint!;
                conn.NoDelay = true;                                   // tắt Nagle: nước đi đến ngay
                events.Add(new CaroEvent("open", conn, $"{ep.Address}:{ep.Port}"));
                Bg(() =>                                               // luồng đọc của kết nối
                {
                    try
                    {
                        while (true) events.Add(new CaroEvent("msg", conn, DecodeJson(RecvFrame(conn))));
                    }
                    catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException || e is ObjectDisposedException)
                    {
                        events.Add(new CaroEvent("closed", conn, null));
                    }
                });
            }
        });
        Bg(() =>                                                       // RESPONDER (Mã giả 6.7)
        {
            var buf = new byte[2048];
            while (true)
            {
                EndPoint from = new IPEndPoint(IPAddress.Any, 0);
                int n;
                try { n = udp.ReceiveFrom(buf, ref from); }
                catch (SocketException) { continue; }
                catch (ObjectDisposedException) { return; }
                var q = DecodeJson(buf.AsSpan(0, n));
                if (q != null && q.Str("type") == "discover" && q.Str("service") == DiscService)
                    events.Add(new CaroEvent("discover", from, q));   // để luồng trò chơi trả lời (nó mới biết số phòng)
            }
        });
        var s = new CaroServer(a, udp);
        try
        {
            while (true)
            {
                var (room, left) = s.ExpiredRoom();
                if (room != null && left <= 0)                         // có phòng hết giờ: xử lý trước
                {
                    s.OnTimeout(room);
                    continue;
                }
                int timeout = room == null ? Timeout.Infinite : (int)Math.Ceiling(left * 1000);
                if (!events.TryTake(out var ev, timeout)) continue;   // vòng sau sẽ thấy phòng hết giờ
                switch (ev.Kind)
                {
                    case "stop":
                        Console.WriteLine("[server] Dừng server");
                        return 0;
                    case "open": s.Clients[(Socket)ev.Key] = new Client((Socket)ev.Key, (string)ev.Data!); break;
                    case "msg":
                        if (s.Clients.TryGetValue((Socket)ev.Key, out var cl)) s.OnMessage(cl, ev.Data as JsonObject);
                        break;
                    case "closed": s.OnClosed((Socket)ev.Key); break;
                    case "discover": s.OnDiscover((IPEndPoint)ev.Key, (JsonObject)ev.Data!); break;
                }
            }
        }
        finally
        {
            srv.Close();
            udp.Close();
        }
    }
}
