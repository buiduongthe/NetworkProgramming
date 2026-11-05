using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR6_4.BanCo;
using static BaiMR6_4.GiaoThuc;

namespace BaiMR6_4;

// Client Caro: người chơi thật (dòng lệnh), bot, hoặc người xem tự động.
// Kết nối: chỉ định host, port; hoặc --tim CỔNG để tìm server trong LAN bằng UDP broadcast (Bài 6.5).
// Chế độ người chơi (không có --bot, --xem): gõ lệnh
//     ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> (ví dụ h8) | lai | roi | thoat
// Chế độ bot (--bot tan_cong|phong_thu|can_bang|tap_su|im_lang, kèm --tao hoặc --vao):
//     --thu-loi: cố tình gửi nước sai (chưa đến lượt, ô đã có quân, ngoài bàn) để xem server kiểm tra.
// Chế độ người xem tự động (--xem <phòng>): in tiến trình và bàn cờ cuối mỗi ván, thoát khi phòng đóng.
record ChoiOptions(string Name, string Host, int Port, int? Tim, string? Tao, string? Vao, string? Xem,
                   string? Bot, long Seed, bool ThuLoi, int SoVan, int TreMs);

/// <summary>Bàn cờ phía client – chỉ là bản sao để hiển thị, được cập nhật từ thông điệp của server.</summary>
class View
{
    public char[][] Board = NewBoard();
    public (int, int)? Last;
    public char? Mark;
    public char? Turn = 'X';

    public void Apply(JsonObject msg, string me)
    {
        switch (msg.Str("type"))
        {
            case "start":
                (Board, Last, Turn) = (NewBoard(), null, 'X');
                Mark = msg.Str("x") == me ? 'X' : msg.Str("o") == me ? 'O' : null;
                break;
            case "board":                                  // ảnh chụp bàn cờ cho người xem đến muộn
                Board = msg["rows"]!.AsArray().Select(r => r!.GetValue<string>().ToCharArray()).ToArray();
                Turn = msg.Str("turn")?[0];
                break;
            case "moved":
                Board[msg.Int("r")][msg.Int("c")] = msg.Str("mark")![0];
                Last = (msg.Int("r"), msg.Int("c"));
                Turn = msg.Str("next")?[0];               // lượt kế do server quyết định (null: ván đã hết)
                break;
            case "over":
                Turn = null;
                break;
        }
    }
}

static class NguoiChoi
{
    static string OverText(JsonObject m)
    {
        string reason = CaroServer.ReasonText[m.Str("reason")!];
        string result = m.Str("winner") is string w ? $"{w} ({m.Str("mark")}) thắng – {reason}" : reason;
        return $"Ván {m.Int("game_no")} kết thúc sau {m.Int("moves")} nước: {result}";
    }

    static string LineText(JsonObject m) =>
        string.Join(" ", m["line"]!.AsArray().Select(p => CellName(p![0]!.GetValue<int>(), p[1]!.GetValue<int>())));

    static JsonObject Msg(string type, string? key = null, JsonNode? val = null)
    {
        var o = new JsonObject { ["type"] = type };
        if (key != null) o[key] = val;
        return o;
    }

    static Socket? Connect(ChoiOptions a)
    {
        string me = a.Name, host = a.Host;
        int port = a.Port;
        if (a.Tim is int tim)
        {
            Console.WriteLine($"[{me}] Tìm server Caro bằng broadcast tới cổng {tim}...");
            var found = Discover(tim, 800);
            if (found.Count == 0)
            {
                Console.Error.WriteLine($"[{me}] Không tìm thấy server nào");
                return null;
            }
            (host, port) = (found[0].Ip, found[0].Port);
            Console.WriteLine($"[{me}] Thấy {found[0].Name} tại {host}:{port}, kết nối");
        }
        var s = new Socket(SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };   // tắt Nagle: gửi thông điệp nhỏ ngay
        using (var cts = new CancellationTokenSource(3000))
            s.ConnectAsync(host, port, cts.Token).AsTask().GetAwaiter().GetResult();
        if (a.Tim == null) Console.WriteLine($"[{me}] Đã kết nối {host}:{port}");
        return s;
    }

    // ---------------------------------------------------------------- bot
    static void QuitRoom(Socket s, string me, int games)
    {
        Console.WriteLine($"[{me}] Đã chơi {games} ván, rời phòng");
        SendJson(s, Msg("bye"));                          // bye = rời phòng + rời sảnh trong một thông điệp
    }

    /// <summary>Bot phản ứng theo từng thông điệp của server. Sau mỗi ván, người THUA (hoặc quân O nếu hòa) lên
    /// tiếng trước: xin chơi lại hoặc rời phòng; người thắng chờ quyết định đó rồi mới trả lời – nhờ quy ước này
    /// hai bot không "nói cùng lúc", nhật ký có thứ tự cố định.</summary>
    static void RunBot(Socket s, string me, ChoiOptions a)
    {
        var view = new View();
        int games = 0;
        var tried = new HashSet<string>();
        var rng = new Lcg(a.Seed);
        bool waiting = false, inRoom = false;            // người thắng đang chờ đối thủ quyết định
        while (true)
        {
            var msg = RecvJson(s);
            if (msg == null) continue;
            string? t = msg.Str("type");
            view.Apply(msg, me);
            bool wantMore = games < a.SoVan && a.Bot != "im_lang";
            switch (t)
            {
                case "welcome":
                    Console.WriteLine($"[{me}] Vào sảnh; số phòng hiện có: {msg["rooms"]!.AsArray().Count}");
                    SendJson(s, a.Tao != null ? Msg("create", "room", a.Tao) : Msg("join", "room", a.Vao));
                    break;
                case "joined":
                    inRoom = true;
                    Console.WriteLine($"[{me}] Vào phòng {msg.Str("room")}, cầm quân {msg.Str("role")}");
                    break;
                case "error":
                    Console.WriteLine($"[{me}] Server báo lỗi {msg.Str("code")}: {msg.Str("msg")}");
                    if (!inRoom)                          // không tạo/vào được phòng: bot dừng
                    {
                        SendJson(s, Msg("bye"));
                        return;
                    }
                    break;
                case "start":
                    Console.WriteLine($"[{me}] Ván {msg.Int("game_no")} bắt đầu: X = {msg.Str("x")}, O = {msg.Str("o")} (tôi cầm {view.Mark})");
                    if (a.ThuLoi && view.Mark == 'O' && tried.Add("early"))
                    {
                        Console.WriteLine($"[{me}] (thử lỗi) đánh h8 khi chưa đến lượt");
                        SendJson(s, new JsonObject { ["type"] = "move", ["r"] = 7, ["c"] = 7 });
                    }
                    break;
                case "rematch_wait" when msg.Str("by") != me && waiting:
                    waiting = false;
                    if (!wantMore)
                    {
                        QuitRoom(s, me, games);
                        return;
                    }
                    Console.WriteLine($"[{me}] {msg.Str("by")} xin chơi lại – đồng ý");
                    SendJson(s, Msg("rematch"));
                    break;
                case "left":
                    Console.WriteLine($"[{me}] {msg.Str("player")} rời phòng");
                    if (waiting)
                    {
                        QuitRoom(s, me, games);
                        return;
                    }
                    break;
                case "over":
                    games++;
                    wantMore = games < a.SoVan && a.Bot != "im_lang";
                    Console.WriteLine($"[{me}] {OverText(msg)}");
                    bool leader = msg.Str("winner") == me || (msg.Str("winner") == null && view.Mark == 'X');
                    if (msg.Str("reason") == "left")      // đối thủ đã đi: không còn ai để chơi
                    {
                        QuitRoom(s, me, games);
                        return;
                    }
                    if (leader) waiting = true;
                    else if (wantMore)
                    {
                        Console.WriteLine($"[{me}] Xin chơi lại");
                        SendJson(s, Msg("rematch"));
                    }
                    else
                    {
                        QuitRoom(s, me, games);
                        return;
                    }
                    break;
            }
            if ((t == "start" || t == "moved") && view.Mark != null && view.Turn == view.Mark && a.Bot != "im_lang")
            {
                Thread.Sleep(a.TreMs);                    // "suy nghĩ" để người xem kịp theo dõi
                if (a.ThuLoi && view.Last is (int lr, int lc) && tried.Add("bad_cells"))
                {
                    Console.WriteLine($"[{me}] (thử lỗi) đánh vào ô đã có quân {CellName(lr, lc)} và ô ngoài bàn (15, 3)");
                    SendJson(s, new JsonObject { ["type"] = "move", ["r"] = lr, ["c"] = lc });
                    SendJson(s, new JsonObject { ["type"] = "move", ["r"] = N, ["c"] = 3 });
                }
                var (r, c) = ChooseMove(view.Board, view.Mark.Value, a.Bot!, rng);
                SendJson(s, new JsonObject { ["type"] = "move", ["r"] = r, ["c"] = c });
            }
        }
    }

    // ---------------------------------------------------------------- người xem tự động
    static void RunWatcher(Socket s, string me, ChoiOptions a)
    {
        var view = new View();
        while (true)
        {
            var msg = RecvJson(s);
            if (msg == null) continue;
            string? t = msg.Str("type");
            view.Apply(msg, me);
            switch (t)
            {
                case "welcome": SendJson(s, Msg("watch", "room", a.Xem)); break;
                case "joined":
                    var info = msg["info"]!.AsObject();
                    Console.WriteLine($"[{me}] Xem phòng {msg.Str("room")} (X = {info.Str("x") ?? "chưa có"}, O = {info.Str("o") ?? "chưa có"})");
                    break;
                case "error":
                    Console.WriteLine($"[{me}] Server báo lỗi {msg.Str("code")}: {msg.Str("msg")}");
                    return;
                case "board":
                    Console.WriteLine($"[{me}] Nhận ảnh chụp bàn cờ: ván {msg.Int("game_no")}, đã đi {msg.Int("moves")} nước");
                    break;
                case "start":
                    Console.WriteLine($"[{me}] Ván {msg.Int("game_no")}: X = {msg.Str("x")}, O = {msg.Str("o")}");
                    break;
                case "moved" when msg.Int("move_no") % 10 == 0:
                    Console.WriteLine($"[{me}] Nước {msg.Int("move_no")}: {msg.Str("by")} ({msg.Str("mark")}) {CellName(msg.Int("r"), msg.Int("c"))}");
                    break;
                case "over":
                    Console.WriteLine($"[{me}] {OverText(msg)}");
                    foreach (var line in Render(view.Board, view.Last)) Console.WriteLine(line);
                    if (msg["line"]!.AsArray().Count > 0) Console.WriteLine($"[{me}] Dãy thắng: {LineText(msg)}");
                    break;
                case "left":
                    Console.WriteLine($"[{me}] {msg.Str("player")} rời phòng");
                    break;
                case "room_closed":
                    Console.WriteLine($"[{me}] Phòng {msg.Str("room")} đã đóng");
                    SendJson(s, Msg("bye"));
                    return;
            }
        }
    }

    // ---------------------------------------------------------------- người chơi thật
    const string Help = "Lệnh: ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> ví dụ h8 | lai | roi | thoat";

    static void RunHuman(Socket s, string me, ChoiOptions a)
    {
        var view = new View();
        var gate = new object();                          // luồng nhận ghi bàn cờ, luồng chính đọc khi kiểm tra ô
        bool quitting = false;

        void ShowBoard()
        {
            foreach (var line in Render(view.Board, view.Last)) Console.WriteLine(line);
            if (view.Mark != null) Console.WriteLine($"[{me}] Bạn cầm {view.Mark}; " + (view.Turn == view.Mark ? "ĐẾN LƯỢT BẠN" : "chờ đối thủ"));
        }

        new Thread(() =>
        {
            try
            {
                while (true)
                {
                    var msg = RecvJson(s);
                    if (msg == null) continue;
                    string? t = msg.Str("type");
                    lock (gate) view.Apply(msg, me);
                    switch (t)
                    {
                        case "welcome" or "rooms":
                            var rooms = msg["rooms"]!.AsArray();
                            Console.WriteLine($"[{me}] Có {rooms.Count} phòng" + (rooms.Count > 0 ? ":" : ""));
                            foreach (var r in rooms.Select(x => x!.AsObject()))
                                Console.WriteLine($"    {r.Str("room")}: X = {r.Str("x") ?? "chưa có"}, O = {r.Str("o") ?? "chưa có"}, " +
                                                  $"{r.Int("watchers")} người xem, {r.Str("state")}");
                            break;
                        case "joined": Console.WriteLine($"[{me}] Vào phòng {msg.Str("room")} với vai trò {msg.Str("role")}"); break;
                        case "start":
                            Console.WriteLine($"[{me}] Ván {msg.Int("game_no")}: X = {msg.Str("x")}, O = {msg.Str("o")}; mỗi nước tối đa {msg.Int("timeout_ms") / 1000} s");
                            ShowBoard();
                            break;
                        case "moved" or "board": ShowBoard(); break;
                        case "over":
                            Console.WriteLine($"[{me}] {OverText(msg)}" + (msg["line"]!.AsArray().Count > 0 ? $"; dãy {LineText(msg)}" : ""));
                            Console.WriteLine($"[{me}] Gõ 'lai' để xin chơi lại hoặc 'roi' để rời phòng");
                            break;
                        case "rematch_wait": Console.WriteLine($"[{me}] {msg.Str("by")} xin chơi lại"); break;
                        case "left": Console.WriteLine($"[{me}] {msg.Str("player")} rời phòng"); break;
                        case "room_closed": Console.WriteLine($"[{me}] Phòng {msg.Str("room")} đã đóng"); break;
                        case "error": Console.WriteLine($"[{me}] Lỗi {msg.Str("code")}: {msg.Str("msg")}"); break;
                    }
                }
            }
            catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException || e is ObjectDisposedException)
            {
                if (!quitting) Console.WriteLine($"[{me}] Mất kết nối tới server");   // gõ thoat: lỗi đọc sau đó là bình thường
            }
        }) { IsBackground = true }.Start();

        Console.WriteLine($"[{me}] {Help}");
        if (a.Tao != null || a.Vao != null) SendJson(s, a.Tao != null ? Msg("create", "room", a.Tao) : Msg("join", "room", a.Vao));
        string? line;
        while ((line = Console.ReadLine()) != null)
        {
            var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length == 0) continue;
            string cmd = parts[0].ToLowerInvariant(), arg = parts.Length > 1 ? parts[1] : "";
            if (cmd == "thoat") break;
            switch (cmd)
            {
                case "ds": SendJson(s, Msg("list")); break;
                case "tao": SendJson(s, Msg("create", "room", arg)); break;
                case "vao": SendJson(s, Msg("join", "room", arg)); break;
                case "xem": SendJson(s, Msg("watch", "room", arg)); break;
                case "lai": SendJson(s, Msg("rematch")); break;
                case "roi": SendJson(s, Msg("leave")); break;
                default:
                    if (ParseCell(cmd) is (int r, int c))
                    {
                        bool taken;
                        lock (gate) taken = view.Board[r][c] != Empty;
                        if (taken)                         // kiểm tra sớm phía client; server vẫn kiểm tra lại
                        {
                            Console.WriteLine($"[{me}] Ô {cmd} đã có quân");
                            continue;
                        }
                        SendJson(s, new JsonObject { ["type"] = "move", ["r"] = r, ["c"] = c });
                    }
                    else Console.WriteLine($"[{me}] {Help}");
                    break;
            }
        }
        quitting = true;
        SendJson(s, Msg("bye"));
    }

    public static int Run(string[] args)
    {
        // <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P] [--bot KIỂU] [--seed 1] [--thu-loi]
        // [--so-van 1] [--tre-ms 300]
        var pos = new List<string>();
        var o = new Dictionary<string, string>();
        bool thuLoi = false;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--thu-loi") thuLoi = true;
            else if (args[i].StartsWith("--") && i + 1 < args.Length) o[args[i]] = args[++i];
            else pos.Add(args[i]);
        }
        if (pos.Count < 1)
        {
            Console.WriteLine("Cách dùng: dotnet run -- choi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P] [--bot KIỂU] ...");
            return 2;
        }
        var a = new ChoiOptions(pos[0], pos.Count > 1 ? pos[1] : "127.0.0.1", pos.Count > 2 ? int.Parse(pos[2]) : 9100,
            o.TryGetValue("--tim", out var tim) ? int.Parse(tim) : null, o.GetValueOrDefault("--tao"), o.GetValueOrDefault("--vao"),
            o.GetValueOrDefault("--xem"), o.GetValueOrDefault("--bot"), long.Parse(o.GetValueOrDefault("--seed", "1")), thuLoi,
            int.Parse(o.GetValueOrDefault("--so-van", "1")), int.Parse(o.GetValueOrDefault("--tre-ms", "300")));
        string me = a.Name;
        if (a.Bot != null && !(Styles.ContainsKey(a.Bot) || a.Bot == "im_lang"))
        {
            Console.Error.WriteLine("--bot phải là tan_cong, phong_thu, can_bang, tap_su hoặc im_lang");
            return 2;
        }
        if (a.Bot != null && a.Tao == null && a.Vao == null)
        {
            Console.Error.WriteLine("Bot cần --tao hoặc --vao");
            return 1;
        }
        Socket? s;
        try { s = Connect(a); }
        catch (Exception e) when (e is SocketException || e is OperationCanceledException)
        {
            Console.Error.WriteLine($"[{me}] Không kết nối được: {e.Message}");
            return 1;
        }
        if (s == null) return 1;
        using (s)
        {
            SendJson(s, new JsonObject { ["type"] = "hello", ["name"] = me });
            try
            {
                if (a.Xem != null) RunWatcher(s, me, a);
                else if (a.Bot != null) RunBot(s, me, a);
                else RunHuman(s, me, a);
            }
            catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException)
            {
                Console.WriteLine($"[{me}] Server đóng kết nối");
            }
        }
        return 0;
    }
}
