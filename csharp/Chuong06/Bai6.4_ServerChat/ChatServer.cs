using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.RegularExpressions;

namespace Bai6_4;

class HelloError(int code) : Exception($"Lỗi {code}")
{
    public int Code { get; } = code;
}

// Mã giả 6.5 – Server chat: vòng chấp nhận kết nối (kế thừa Mã giả 6.3) và tác vụ phiên CHAT_SESSION.
// Giao thức: Bảng đặc tả giao thức chat (Mục 6.3.1), cổng mặc định 9200.
static class ChatServer
{
    static readonly Regex NameRe = new(@"^[\w-]{1,32}$");                // tập ký tự cho phép của tên, phòng
    const int RateLimit = 10;                                            // tối đa 10 tin msg/pm mỗi giây

    static bool Valid(string? s) => s != null && NameRe.IsMatch(s);

    static bool RateOk(ChatUser c)
    {
        long now = Environment.TickCount64;
        if (now - c.WinStart >= 1000) { c.WinStart = now; c.WinCount = 0; }
        return ++c.WinCount <= RateLimit;
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = AsyncFrame.ParseEndpoint(args, "0.0.0.0", 9200);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // như Mã giả 6.3
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(128);
        Console.WriteLine($"[server] Server chat đang lắng nghe tại {host}:{port}");
        while (true)
        {
            Socket conn = await srv.AcceptAsync();                       // AWAIT ACCEPT_ASYNC
            _ = ChatSession(conn, conn.RemoteEndPoint!.ToString()!);     // RUN_BACKGROUND
        }
    }

    // CHAT_SESSION (Mã giả 6.5, dòng 4–33)
    static async Task ChatSession(Socket conn, string addr)
    {
        ChatUser? c = null;                                              // chưa đăng ký tên
        var stream = new NetworkStream(conn, ownsSocket: false);
        try
        {
            JsonObject h;
            using (var cts = new CancellationTokenSource(10000))         // 10 giây cho hello
                h = await AsyncFrame.RecvJsonAsync(stream, cts.Token);
            string? name = AsyncFrame.Str(h, "name");
            if (AsyncFrame.Str(h, "type") != "hello" || !Valid(name)) throw new HelloError(400);
            c = Chat.Register(name!, conn, stream);                      // null nếu trùng tên
            if (c == null) throw new HelloError(409);
            Console.WriteLine($"[server] {c.Name} đăng ký từ {addr}");
            _ = Chat.Writer(c);                                          // RUN_BACKGROUND(WRITER(c))
            JsonArray users, rooms;
            lock (Chat.Gate)
            {
                users = new JsonArray(Chat.Users.Keys.Order(StringComparer.Ordinal).Select(k => (JsonNode)k!).ToArray());
                rooms = new JsonArray(Chat.Rooms.Keys.Order(StringComparer.Ordinal).Select(k => (JsonNode)k!).ToArray());
            }
            Chat.Enqueue(c, new JsonObject { ["type"] = "welcome", ["users"] = users, ["rooms"] = rooms });
            while (true)
            {
                JsonObject m;
                using (var cts = new CancellationTokenSource(90000))     // 3 chu kỳ nhịp tim
                    m = await AsyncFrame.RecvJsonAsync(stream, cts.Token);
                string? t = AsyncFrame.Str(m, "type"), room = AsyncFrame.Str(m, "room"),
                        text = AsyncFrame.Str(m, "text"), to = AsyncFrame.Str(m, "to");
                if ((t == "msg" || t == "pm") && !RateOk(c))
                    Chat.Enqueue(c, Chat.Error(429));
                else if (t == "join" && Valid(room))
                    Chat.JoinRoom(c, room!);
                else if (t == "leave" && room != null)
                    Chat.LeaveRoom(c, room);
                else if (t == "msg" && room != null && text != null)
                    Chat.Broadcast(room, new JsonObject { ["type"] = "msg", ["from"] = c.Name, ["room"] = room,
                                                          ["text"] = text, ["ts"] = Chat.Now() }, c);
                else if (t == "pm" && to != null && text != null)
                    Chat.SendTo(to, new JsonObject { ["type"] = "pm", ["from"] = c.Name, ["text"] = text,
                                                     ["ts"] = Chat.Now() }, c);
                else if (t == "ping")
                    Chat.Enqueue(c, new JsonObject { ["type"] = "pong" });
                else                                                     // OTHERWISE
                    Chat.Enqueue(c, Chat.Error(400));
            }
        }
        catch (HelloError e)                                             // lời chào không hợp lệ
        {
            Console.WriteLine($"[server] Từ chối {addr}: lỗi {e.Code}");
            try { await AsyncFrame.SendJsonAsync(stream, Chat.Error(e.Code), CancellationToken.None); }
            catch (Exception x) when (x is IOException || x is SocketException) { }
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is JsonException ||
                                  e is OperationCanceledException || e is ObjectDisposedException)
        {
            // kết thúc phiên bình thường hoặc do lỗi
        }
        finally
        {
            if (c != null)
            {
                Chat.Unregister(c);
                c.Outbox.Writer.TryComplete();                           // outbox.close()
                Console.WriteLine($"[server] {c.Name} rời server");
            }
            conn.Close();                                                // CLOSE(conn)
        }
    }
}
