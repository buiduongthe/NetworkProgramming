using System.Net;
using System.Net.Security;
using System.Net.Sockets;
using System.Security.Authentication;
using System.Security.Cryptography.X509Certificates;
using System.Text.Json.Nodes;
using System.Threading.Channels;

namespace Bai8_4;

class HelloError(int code, string text) : Exception(text) { public int Code { get; } = code; }

class Client(string name, Stream sock)
{
    public string Name { get; } = name;
    public Stream Sock { get; } = sock;
    public Channel<JsonObject> Outbox { get; } = Channel.CreateBounded<JsonObject>(256);   // BOUNDED_QUEUE(256)
}

// Mã giả 8.6 phía server: TLS_SESSION thay SERVE_ASYNC, VERIFY_JWT trong hello; kế thừa Mã giả 6.5–6.6
// (CHAT_SESSION, BROADCAST, SEND_TO, ENQUEUE, WRITER – bản rút gọn: hello, join, msg, ping)
static class ChatServer
{
    static readonly Dictionary<string, Client> users = new();
    static readonly Dictionary<string, HashSet<string>> rooms = new();
    static readonly object lockObj = new();
    static X509Certificate2 cert = null!;

    static void Log(string text) => Console.WriteLine("[server] " + text);
    static string Ver(SslProtocols p) => p switch { SslProtocols.Tls13 => "TLSv1.3", SslProtocols.Tls12 => "TLSv1.2", _ => p.ToString() };

    static void Enqueue(Client c, JsonObject msg)
    {
        if (!c.Outbox.Writer.TryWrite(msg)) { Log($"Ngắt client chậm {c.Name}"); c.Sock.Close(); }   // hàng đợi đầy
    }

    static void SendTo(string name, JsonObject msg, Client? sender)
    {
        Client? c;
        lock (lockObj) users.TryGetValue(name, out c);
        if (c != null) Enqueue(c, (JsonObject)msg.DeepClone());
        else if (sender != null) Enqueue(sender, new JsonObject { ["type"] = "error", ["code"] = 404, ["text"] = "Không có người nhận" });
    }

    static void Broadcast(string? room, JsonObject msg, Client? sender)
    {
        string[] targets = [];
        lock (lockObj) if (room != null && rooms.TryGetValue(room, out var set)) targets = [.. set];   // giữ khóa ngắn
        if (sender != null && !targets.Contains(sender.Name))
        {
            Enqueue(sender, new JsonObject { ["type"] = "error", ["code"] = 403, ["text"] = "Chưa tham gia phòng" });
            return;
        }
        foreach (var name in targets) SendTo(name, msg, null);
    }

    static async Task Writer(Client c)                       // tác vụ ghi duy nhất của mỗi client
    {
        try { await foreach (var msg in c.Outbox.Reader.ReadAllAsync()) await Frame.SendJsonAsync(c.Sock, msg); }
        catch (Exception e) when (e is IOException or ObjectDisposedException) { c.Sock.Close(); }
    }

    // CHAT_SESSION (Mã giả 6.5) chạy trên kết nối đã bọc TLS; hello mang token thay cho tên
    static async Task ChatSession(SslStream conn, string addr)
    {
        Client? c = null;
        Task? writer = null;
        try
        {
            JsonObject h = await Frame.RecvJsonAsync(conn, new CancellationTokenSource(10000).Token);
            if (Frame.Str(h["type"]) != "hello") throw new HelloError(400, "Thông điệp đầu phải là hello");
            JsonObject claims;
            try { claims = Jwt.Verify(Frame.Str(h["token"]), Jwt.Secret("CHAT_JWT_KEY"), Jwt.Issuer, "chat"); }
            catch (JwtError e) { throw new HelloError(401, "Token không hợp lệ: " + e.Message); }
            string sub = Frame.Str(claims["sub"])!;
            lock (lockObj) if (!users.ContainsKey(sub)) users[sub] = c = new Client(sub, conn);   // REGISTER(claims.sub)
            if (c == null) throw new HelloError(409, "Tên đã được sử dụng");
            Log($"{c.Name} đăng nhập từ {addr} (tên lấy từ claim sub)");
            writer = Writer(c);                                  // RUN_BACKGROUND(WRITER(c))
            string[] names;
            lock (lockObj) names = [.. users.Keys.Order()];
            Enqueue(c, new JsonObject { ["type"] = "welcome", ["name"] = c.Name, ["users"] = new JsonArray(names.Select(n => (JsonNode)n).ToArray()) });
            while (true)
            {
                JsonObject m = await Frame.RecvJsonAsync(conn, new CancellationTokenSource(90000).Token);
                string? room = Frame.Str(m["room"]);
                switch (Frame.Str(m["type"]))
                {
                    case "join":
                        lock (lockObj) { if (room != null) { if (!rooms.ContainsKey(room)) rooms[room] = []; rooms[room].Add(c.Name); } }
                        Broadcast(room, new JsonObject { ["type"] = "sys", ["text"] = $"{c.Name} đã vào phòng {room}" }, null);
                        break;
                    case "msg":
                        Broadcast(room, new JsonObject { ["type"] = "msg", ["from"] = c.Name, ["room"] = room,
                            ["text"] = Frame.Str(m["text"]), ["ts"] = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000.0 }, c);
                        break;
                    case "ping": Enqueue(c, new JsonObject { ["type"] = "pong" }); break;
                    default: Enqueue(c, new JsonObject { ["type"] = "error", ["code"] = 400, ["text"] = "Loại thông điệp không hỗ trợ" }); break;
                }
            }
        }
        catch (HelloError e)                                     // lời chào không hợp lệ
        {
            Log($"Từ chối {addr}: {e.Code} {e.Message}");
            await Frame.SendJsonAsync(conn, new JsonObject { ["type"] = "error", ["code"] = e.Code, ["text"] = e.Message });
        }
        catch (Exception e) when (e is IOException or EndOfStreamException or OperationCanceledException
                                  or ProtocolError or System.Text.Json.JsonException or ObjectDisposedException)
        {
            if (c == null) Log($"Kết nối {addr} đóng trước khi đăng nhập: {e.InnerException?.Message ?? e.Message}");
        }
        finally
        {
            if (c != null)                                       // UNREGISTER, đóng hộp thư đi
            {
                lock (lockObj) { users.Remove(c.Name); foreach (var set in rooms.Values) set.Remove(c.Name); }
                c.Outbox.Writer.TryComplete();
                if (writer != null) await Task.WhenAny(writer, Task.Delay(1000));
                Log($"{c.Name} rời phiên");
            }
            conn.Dispose();                                      // CLOSE(conn)
        }
    }

    // Mã giả 8.6 – TLS_SESSION(conn, addr): bắt tay TLS ở vai trò server trước phiên chat
    static async Task TlsSession(Socket conn)
    {
        string addr = conn.RemoteEndPoint!.ToString()!;
        var tls = new SslStream(new NetworkStream(conn, ownsSocket: true), leaveInnerStreamOpen: false);
        try
        {
            var opts = new SslServerAuthenticationOptions
            { ServerCertificate = cert, EnabledSslProtocols = SslProtocols.Tls12 | SslProtocols.Tls13 };   // min ← TLS1_2
            await tls.AuthenticateAsServerAsync(opts, new CancellationTokenSource(10000).Token);
        }
        catch (Exception e) when (e is AuthenticationException or IOException or OperationCanceledException)
        {
            Log($"Bắt tay TLS lỗi {addr}: {e.InnerException?.Message ?? e.Message}");
            tls.Dispose();
            return;
        }
        Log($"Bắt tay TLS thành công {addr}: {Ver(tls.SslProtocol)}, {tls.NegotiatedCipherSuite}");
        await ChatSession(tls, addr);                            // tls thay cho conn
    }

    public static async Task<int> Run(string[] args)
    {
        string host = args.Length > 0 ? args[0] : Environment.GetEnvironmentVariable("APP_HOST") ?? "0.0.0.0";   // Mã giả 2.1 (rút gọn)
        int port = args.Length > 1 ? int.Parse(args[1]) : int.Parse(Environment.GetEnvironmentVariable("APP_PORT") ?? "9200");
        // LOAD_CERT_AND_KEY(ENV("TLS_CERT"), SECRET("TLS_KEY_PASS")): nạp một lần khi khởi động
        cert = X509CertificateLoader.LoadPkcs12FromFile(Jwt.Secret("TLS_CERT"), Jwt.Secret("TLS_KEY_PASS"));
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Log($"Đang lắng nghe (TLS ≥ 1.2) tại {host}:{port}");
        while (true)
        {
            Socket conn = await srv.AcceptAsync();
            _ = TlsSession(conn);                                // mỗi kết nối một tác vụ (Mã giả 6.3)
        }
    }
}
