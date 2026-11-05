using System.Collections.Concurrent;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace BaiMR3_3;

// Client trò chơi đoán số: người chơi thật (bàn phím) hoặc bot.
// Chế độ người chơi: một luồng nhận thông điệp của server và in ra; luồng chính chỉ hỏi số khi đến lượt.
// Client cũng kiểm tra đầu vào (phải là số nguyên) để báo lỗi sớm cho người dùng, nhưng server vẫn
// kiểm tra lại mọi thứ – server không bao giờ tin client.
// Các bot (dùng cho demo tất định):
//   chia_doi   – tìm kiếm nhị phân: đoán điểm giữa khoảng [lo, hi] server gửi kèm lượt.
//   ngau_nhien – đoán ngẫu nhiên trong [lo, hi] (bộ sinh LCG, hạt giống --seed).
//   nghich     – cố tình gửi thông điệp sai để xem server kiểm tra: đoán khi ván chưa bắt đầu,
//                khung không phải JSON, đoán khi chưa đến lượt, giá trị là chuỗi, số ngoài khoảng.
//   im_lang    – không bao giờ trả lời: bị server tính hết giờ, rồi tự ngắt kết nối.
static class NguoiChoi
{
    /// <summary>Chuyển thông điệp của server thành dòng chữ cho người chơi; trả null nếu không cần in.</summary>
    static string? Describe(string me, JsonObject m)
    {
        string? t = m.Str("type");
        switch (t)
        {
            case "welcome":
                return $"[{me}] Vào phòng chờ: {m.Int("waiting")}/{m.Int("need")} người, " +
                       $"đoán số trong {m.Int("min")}–{m.Int("max")}";
            case "start":
                var order = m["order"]!.AsArray().Select(x => x!.GetValue<string>());
                return $"[{me}] Ván {m.Int("round")} bắt đầu, thứ tự: {string.Join(", ", order)}";
            case "turn" when m.Str("player") == me:
                return $"[{me}] Đến lượt bạn (lượt {m.Int("turn_no")}), khoảng {m.Int("lo")}–{m.Int("hi")}";
            case "result":
                string hint = m.Str("hint") ?? "";
                return $"[{me}] {m.Str("player")} đoán {m.Int("value")}: {Game.HintText.GetValueOrDefault(hint, hint)}";
            case "timeout":
                return $"[{me}] {m.Str("player")} hết giờ, mất lượt";
            case "left":
                return $"[{me}] {m.Str("player")} đã rời trò chơi";
            case "error":
                return $"[{me}] Server báo lỗi {m.Str("code")}: {m.Str("msg")}";
            case "game_over":
                string who = string.IsNullOrEmpty(m.Str("winner")) ? "không ai" : m.Str("winner")!;
                return $"[{me}] Kết thúc: {who} thắng, số bí mật là {m.Int("secret")} ({m.Int("turns")} lượt)";
            case "bye":
                return $"[{me}] Server kết thúc phiên";
            default:
                return null;
        }
    }

    static void Say(string? line)
    {
        if (line != null) Console.WriteLine(line);
    }

    static JsonObject Guess(JsonNode? v) => new() { ["type"] = "guess", ["value"] = v };

    // ---------------------------------------------------------------- bot
    static void RunBot(Socket s, string me, string kind, long seed, int thinkMs)
    {
        var rng = new Lcg(seed);
        var tricksDone = new HashSet<string>();         // các "trò nghịch" đã làm (mỗi trò một lần)
        while (true)
        {
            var msg = GiaoThuc.RecvJson(s);
            if (msg == null) continue;
            Say(Describe(me, msg));
            string? t = msg.Str("type");
            if (t == "game_over" || t == "bye") return;
            if (kind == "nghich" && t == "welcome")
            {
                Console.WriteLine($"[{me}] (nghịch) đoán khi ván chưa bắt đầu, rồi gửi khung không phải JSON");
                GiaoThuc.SendJson(s, Guess(1));
                GiaoThuc.SendFrame(s, Encoding.UTF8.GetBytes("xin chào"));
            }
            if (t == "timeout" && msg.Str("player") == me && kind == "im_lang")
            {
                Console.WriteLine($"[{me}] (im lặng) bị hết giờ, ngắt kết nối");
                return;
            }
            if (t != "turn") continue;
            if (msg.Str("player") != me)
            {
                if (kind == "nghich" && tricksDone.Add("out_of_turn"))
                {
                    Console.WriteLine($"[{me}] (nghịch) đoán khi đang là lượt của {msg.Str("player")}");
                    GiaoThuc.SendJson(s, Guess(msg.Int("lo")));
                }
                continue;
            }
            if (kind == "im_lang") continue;
            long lo = msg.Int("lo"), hi = msg.Int("hi");
            Thread.Sleep(thinkMs);                       // "suy nghĩ" để người xem kịp theo dõi
            if (kind == "nghich" && tricksDone.Add("bad_values"))
            {
                Console.WriteLine($"[{me}] (nghịch) gửi giá trị chuỗi và số ngoài khoảng");
                GiaoThuc.SendJson(s, Guess("năm mươi"));
                GiaoThuc.SendJson(s, Guess(hi + 50));
            }
            long guess = kind == "ngau_nhien" ? rng.NextInt(lo, hi) : (lo + hi) / 2;
            Console.WriteLine($"[{me}] Đoán {guess}");
            GiaoThuc.SendJson(s, Guess(guess));
        }
    }

    // ---------------------------------------------------------------- người chơi thật
    static void RunHuman(Socket s, string me)
    {
        var turns = new BlockingCollection<string>();   // luồng nhận báo cho luồng chính: "turn" hoặc "end"
        new Thread(() =>
        {
            try
            {
                while (true)
                {
                    var msg = GiaoThuc.RecvJson(s);
                    if (msg == null) continue;
                    Say(Describe(me, msg));
                    string? t = msg.Str("type");
                    if (t == "turn" && msg.Str("player") == me) turns.Add("turn");
                    if (t == "game_over" || t == "bye") break;
                }
            }
            catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException ||
                                      e is ObjectDisposedException) { }
            turns.Add("end");
        }) { IsBackground = true }.Start();

        bool echo = Console.IsInputRedirected;           // đầu vào từ tệp/ống: in lại để nhật ký dễ đọc
        while (turns.Take() == "turn")
        {
            while (true)
            {
                Console.Write($"[{me}] Nhập số: ");
                string? line = Console.ReadLine();
                if (line == null)                        // hết đầu vào (Ctrl+D / Ctrl+Z): rời trò chơi
                {
                    Console.WriteLine();
                    GiaoThuc.SendJson(s, new JsonObject { ["type"] = "bye" });
                    return;
                }
                line = line.Trim();
                if (echo) Console.WriteLine(line);
                if (!long.TryParse(line, out long value))    // kiểm tra phía client: chỉ để báo lỗi sớm
                {
                    Console.WriteLine($"[{me}] Vui lòng nhập một số nguyên");
                    continue;
                }
                GiaoThuc.SendJson(s, Guess(value));
                break;
            }
        }
    }

    public static int Run(string[] args)
    {
        // <tên> [host] [port] [--bot KIỂU] [--seed N] [--nghi-ms 300]
        var pos = new List<string>();
        string? bot = null; long seed = 1; int think = 300;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--bot": bot = args[++i]; break;
                case "--seed": seed = long.Parse(args[++i]); break;
                case "--nghi-ms": think = int.Parse(args[++i]); break;
                default: pos.Add(args[i]); break;
            }
        }
        if (pos.Count < 1 || (bot != null && !new[] { "chia_doi", "ngau_nhien", "nghich", "im_lang" }.Contains(bot)))
        {
            Console.WriteLine("Cách dùng: dotnet run -- choi <tên> [host] [port] [--bot chia_doi|ngau_nhien|nghich|im_lang] [--seed N] [--nghi-ms 300]");
            return 2;
        }
        string me = pos[0], host = pos.Count > 1 ? pos[1] : "127.0.0.1";
        int port = pos.Count > 2 ? int.Parse(pos[2]) : 9000;
        var s = new Socket(SocketType.Stream, ProtocolType.Tcp);
        try
        {
            using var cts = new CancellationTokenSource(3000);
            s.ConnectAsync(host, port, cts.Token).AsTask().GetAwaiter().GetResult();
        }
        catch (Exception e) when (e is SocketException || e is OperationCanceledException)
        {
            Console.Error.WriteLine($"[{me}] Không kết nối được {host}:{port}: {e.Message}");
            return 1;
        }
        s.NoDelay = true;                                 // tắt Nagle: gửi thông điệp nhỏ ngay
        Console.WriteLine($"[{me}] Đã kết nối {host}:{port}");
        GiaoThuc.SendJson(s, new JsonObject { ["type"] = "hello", ["name"] = me });
        try
        {
            if (bot != null) RunBot(s, me, bot, seed, think);
            else RunHuman(s, me);
        }
        catch (Exception e) when (e is ConnectionClosed || e is ProtocolError || e is SocketException)
        {
            Console.WriteLine($"[{me}] Server đóng kết nối");
        }
        finally
        {
            s.Close();
        }
        return 0;
    }
}
