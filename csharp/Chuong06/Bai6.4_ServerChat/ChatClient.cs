using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai6_4;

// Lớp ChatClient chép gọn từ Mã giả 5.5 (Mục 5.3.3): Connect gửi hello rồi chạy nền ReceiveLoop,
// Send dùng send_lock (SemaphoreSlim) để mỗi lúc chỉ gửi một khung, Disconnect hủy vòng nhận rồi đóng socket.
class ChatClient
{
    Socket? _sock;
    NetworkStream? _stream;
    CancellationTokenSource? _cts;
    readonly SemaphoreSlim _sendLock = new(1, 1);                        // ASYNC_LOCK()
    public event Action<JsonObject>? OnMessage;
    public event Action<string>? OnStatus;

    public async Task ConnectAsync(string host, int port, string name)
    {
        _sock = new Socket(SocketType.Stream, ProtocolType.Tcp);
        using (var t = new CancellationTokenSource(3000))                // SAFE_CONNECT_ASYNC, 3000 ms
            await _sock.ConnectAsync(host, port, t.Token);
        _stream = new NetworkStream(_sock, ownsSocket: true);
        _cts = new CancellationTokenSource();
        try
        {
            await AsyncFrame.SendJsonAsync(_stream, new JsonObject { ["type"] = "hello", ["name"] = name }, _cts.Token);
        }
        catch (IOException)
        {
            _sock.Close();                                               // không để rò socket
            throw;
        }
        OnStatus?.Invoke("connected");                                   // báo trước khi vòng nhận chạy nền
        _ = ReceiveLoop();                                               // RUN_BACKGROUND: chạy nền
    }

    async Task ReceiveLoop()
    {
        try
        {
            while (true) OnMessage?.Invoke(await AsyncFrame.RecvJsonAsync(_stream!, _cts!.Token));
        }
        catch (OperationCanceledException)
        {
            return;                                                      // ngắt kết nối chủ động
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is JsonException || e is ObjectDisposedException)
        {
            _sock!.Close();                                              // giải phóng socket
            string reason = e is ConnectionClosedException cc && cc.Received == 0 ? "server đã đóng kết nối" : e.Message;
            OnStatus?.Invoke("lost: " + reason);
        }
    }

    public async Task SendAsync(JsonObject m)
    {
        await _sendLock.WaitAsync();                                     // mỗi lúc gửi một khung
        try { await AsyncFrame.SendJsonAsync(_stream!, m, _cts!.Token); }
        finally { _sendLock.Release(); }
    }

    public void Disconnect()
    {
        _cts?.Cancel();                                                  // hủy trước, đóng socket sau
        _sock?.Close();
    }
}

// Client chat dòng lệnh: /join phòng, /leave phòng, /pm tên nội dung, /ping, /quit; dòng khác là tin msg
// gửi tới phòng hiện tại. Bộ hẹn giờ gửi ping mỗi 30 giây. Hết dữ liệu nhập: chờ 1 giây rồi ngắt kết nối.
static class ChatClientApp
{
    static void Show(JsonObject m)
    {
        string? t = AsyncFrame.Str(m, "type");
        string ts = AsyncFrame.Str(m, "ts") ?? "";
        string hhmmss = ts.Length >= 19 ? ts.Substring(11, 8) : "";
        string Join(string key) => string.Join(", ", m[key]!.AsArray().Select(x => (string)x!));
        Console.WriteLine(t switch
        {
            "welcome" => $"[client] Chào mừng! Trực tuyến: {Join("users")}; phòng: " +
                         (m["rooms"]!.AsArray().Count > 0 ? Join("rooms") : "(chưa có)"),
            "msg" => $"[client] {hhmmss} [{m["room"]}] {m["from"]}: {m["text"]}",
            "pm" => $"[client] {hhmmss} [riêng] {m["from"]}: {m["text"]}",
            "sys" => $"[client] * {m["text"]}",
            "error" => $"[client] Lỗi {m["code"]}: {m["text"]}",
            "pong" => "[client] pong",
            _ => $"[client] {m.ToJsonString()}",
        });
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = AsyncFrame.ParseEndpoint(args, "127.0.0.1", 9200);
        string name = args.Length > 2 ? args[2] : "user";
        var c = new ChatClient();
        c.OnMessage += Show;
        c.OnStatus += s => Console.WriteLine(s == "connected"
            ? $"[client] Đã kết nối {host}:{port}, gửi hello với tên {name}" : $"[client] Mất kết nối: {s[6..]}");
        try { await c.ConnectAsync(host, port, name); }
        catch (Exception e) when (e is SocketException || e is IOException || e is OperationCanceledException)
        {
            Console.WriteLine($"[client] Không kết nối được {host}:{port}: {e.Message}");
            return 1;
        }
        _ = Task.Run(async () =>                                         // bộ hẹn giờ nhịp tim 30 giây
        {
            using var timer = new PeriodicTimer(TimeSpan.FromSeconds(30));
            while (await timer.WaitForNextTickAsync()) await c.SendAsync(new JsonObject { ["type"] = "ping" });
        });
        string? room = null;                                             // phòng hiện tại
        string? line;
        bool quit = false;
        while ((line = await Task.Run(Console.ReadLine)) != null)        // READ_CONSOLE
        {
            line = line.Trim();
            if (line.Length == 0) continue;
            Console.WriteLine($"[client] > {line}");
            string[] p = line.Split(' ', 3);
            try
            {
                if (p[0] == "/join" && p.Length > 1) { room = line[6..]; await c.SendAsync(new JsonObject { ["type"] = "join", ["room"] = room }); }
                else if (p[0] == "/leave" && p.Length > 1) await c.SendAsync(new JsonObject { ["type"] = "leave", ["room"] = line[7..] });
                else if (p[0] == "/pm" && p.Length == 3) await c.SendAsync(new JsonObject { ["type"] = "pm", ["to"] = p[1], ["text"] = p[2] });
                else if (p[0] == "/ping") await c.SendAsync(new JsonObject { ["type"] = "ping" });
                else if (p[0] == "/quit") { quit = true; break; }
                else if (line.StartsWith('/')) Console.WriteLine("[client] Lệnh không hợp lệ");
                else if (room == null) Console.WriteLine("[client] Chưa chọn phòng: dùng /join tên_phòng");
                else await c.SendAsync(new JsonObject { ["type"] = "msg", ["room"] = room, ["text"] = line });
            }
            catch (Exception e) when (e is IOException || e is SocketException || e is ObjectDisposedException)
            {
                Console.WriteLine($"[client] Gửi lỗi: {e.Message}");
                quit = true;
                break;
            }
        }
        if (!quit) await Task.Delay(1000);                               // hết dữ liệu nhập: chờ nhận nốt
        c.Disconnect();
        Console.WriteLine("[client] Đã ngắt kết nối");
        return 0;
    }
}
