using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai5_4;

// Server chat tối giản để chạy thử lớp ChatClient: chỉ phần giao thức chat (Mục 6.3.1) mà Mã giả 5.5 dùng:
// hello → welcome (409 nếu trùng tên), msg → chuyển tiếp tới mọi người trong một phòng chung, ping → pong,
// sys khi có người vào, ra. Bản đầy đủ ở Chương 6. Mỗi client có một khóa gửi để các khung không xen kẽ.
static class ChatServer
{
    record Peer(NetworkStream Stream, SemaphoreSlim Lock);
    static readonly ConcurrentDictionary<string, Peer> Users = new();  // tên → client

    static async Task SendToAsync(string name, JsonNode obj)
    {
        if (!Users.TryGetValue(name, out var p)) return;
        await p.Lock.WaitAsync();                                       // mỗi socket mỗi lúc chỉ ghi một khung
        try { await Khung.SendJsonAsync(p.Stream, obj, CancellationToken.None); }
        finally { p.Lock.Release(); }
    }

    static async Task BroadcastAsync(Func<JsonNode> make)
    {
        foreach (string name in Users.Keys.ToList())                    // sao chép danh sách rồi mới gửi
        {
            try { await SendToAsync(name, make()); }
            catch (Exception e) when (e is IOException || e is ObjectDisposedException) { }  // client vừa rời
        }
    }

    static async Task ChatSessionAsync(Socket conn)
    {
        string? name = null;
        using var stream = new NetworkStream(conn, ownsSocket: true);
        try
        {
            using (var t10 = new CancellationTokenSource(10000))        // hello trong 10 giây
            {
                JsonNode h = await Khung.RecvJsonAsync(stream, t10.Token);
                string? n = (string?)h["name"];
                if ((string?)h["type"] != "hello" || string.IsNullOrEmpty(n))
                {
                    await Khung.SendJsonAsync(stream, new JsonObject { ["type"] = "error", ["code"] = 400, ["text"] = "Cần hello" }, default);
                    return;
                }
                if (!Users.TryAdd(n, new Peer(stream, new SemaphoreSlim(1, 1))))
                {
                    await Khung.SendJsonAsync(stream, new JsonObject { ["type"] = "error", ["code"] = 409, ["text"] = "Tên đã được dùng" }, default);
                    return;
                }
                name = n;
            }
            Console.WriteLine($"[server] {name} đã kết nối");
            await SendToAsync(name, new JsonObject { ["type"] = "welcome", ["users"] = new JsonArray(Users.Keys.Select(k => (JsonNode)k!).ToArray()) });
            await BroadcastAsync(() => new JsonObject { ["type"] = "sys", ["text"] = $"{name} đã vào phòng" });
            while (true)
            {
                using var t90 = new CancellationTokenSource(90000);
                JsonNode m = await Khung.RecvJsonAsync(stream, t90.Token);
                string? type = (string?)m["type"];
                if (type == "msg")
                {
                    string? text = (string?)m["text"];
                    string room = m["room"] is JsonValue rv && rv.TryGetValue(out string? r) ? r : "chung";
                    Console.WriteLine($"[server] {name}: {text}");
                    string ts = DateTime.Now.ToString("HH:mm:ss");
                    await BroadcastAsync(() => new JsonObject { ["type"] = "msg", ["from"] = name, ["room"] = room, ["text"] = text, ["ts"] = ts });
                }
                else if (type == "ping") await SendToAsync(name, new JsonObject { ["type"] = "pong" });
                else await SendToAsync(name, new JsonObject { ["type"] = "error", ["code"] = 400, ["text"] = "Sai loại thông điệp" });
            }
        }
        catch (Exception) { }                                           // kết thúc phiên bình thường hoặc do lỗi
        finally
        {
            if (name != null && Users.TryRemove(name, out _))
            {
                Console.WriteLine($"[server] {name} đã rời phòng");
                await BroadcastAsync(() => new JsonObject { ["type"] = "sys", ["text"] = $"{name} đã rời phòng" });
            }
        }
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args, "0.0.0.0", 9200);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        while (true)
            _ = ChatSessionAsync(await srv.AcceptAsync());              // mỗi kết nối một Task
    }
}
