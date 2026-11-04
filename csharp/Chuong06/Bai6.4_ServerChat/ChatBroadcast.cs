using System.Globalization;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using System.Threading.Channels;

namespace Bai6_4;

/// <summary>STRUCT Client { name, sock, outbox ← BOUNDED_QUEUE(256) } (Mã giả 6.5, dòng 1).</summary>
class ChatUser(string name, Socket sock, NetworkStream stream)
{
    public string Name { get; } = name;
    public Socket Sock { get; } = sock;
    public NetworkStream Stream { get; } = stream;
    public Channel<JsonObject> Outbox { get; } = Channel.CreateBounded<JsonObject>(256);
    public long WinStart;                                                // đếm tin trong cửa sổ 1 giây (lỗi 429)
    public int WinCount;
}

// Mã giả 6.6 – Phát tới phòng, gửi riêng và tác vụ ghi (Mục 6.3.3), cùng phần cấu trúc dữ liệu
// của Mã giả 6.5 và các thủ tục mã giả chỉ nêu tên: REGISTER, UNREGISTER, JOIN_ROOM, LEAVE_ROOM.
static class Chat
{
    static readonly Dictionary<int, string> Errors = new()
    {
        [400] = "Sai cú pháp", [403] = "Chưa tham gia phòng", [404] = "Không có người nhận",
        [409] = "Tên đã được sử dụng", [429] = "Gửi quá nhanh",
    };
    public static readonly Dictionary<string, ChatUser> Users = new();             // tên → client
    public static readonly Dictionary<string, HashSet<string>> Rooms = new();      // phòng → tập tên
    public static readonly object Gate = new();                                    // LOCK()

    public static JsonObject Error(int code) => new() { ["type"] = "error", ["code"] = code, ["text"] = Errors[code] };

    public static string Now() =>                                        // NOW(): ISO 8601 UTC
        DateTime.UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'", CultureInfo.InvariantCulture);

    public static ChatUser? Register(string name, Socket sock, NetworkStream stream)
    {
        lock (Gate)
        {
            if (Users.ContainsKey(name)) return null;                    // trùng tên
            return Users[name] = new ChatUser(name, sock, stream);
        }
    }

    public static void Unregister(ChatUser c)
    {
        List<string> left;
        lock (Gate)
        {
            Users.Remove(c.Name);
            left = Rooms.Where(r => r.Value.Contains(c.Name)).Select(r => r.Key).ToList();
            foreach (var r in left)
                if (Rooms[r].Remove(c.Name) && Rooms[r].Count == 0) Rooms.Remove(r);
        }
        foreach (var r in left)                                          // phát ngoài khóa
            Broadcast(r, new JsonObject { ["type"] = "sys", ["text"] = $"{c.Name} đã rời phòng {r}" }, null);
    }

    public static void JoinRoom(ChatUser c, string room)
    {
        lock (Gate)
        {
            if (!Rooms.TryGetValue(room, out var members)) Rooms[room] = members = new HashSet<string>();
            members.Add(c.Name);
        }
        Console.WriteLine($"[server] {c.Name} vào phòng {room}");
        Broadcast(room, new JsonObject { ["type"] = "sys", ["text"] = $"{c.Name} đã vào phòng {room}" }, null);
    }

    public static void LeaveRoom(ChatUser c, string room)
    {
        bool member;
        lock (Gate)
        {
            member = Rooms.TryGetValue(room, out var m) && m.Remove(c.Name);
            if (member && Rooms[room].Count == 0) Rooms.Remove(room);
        }
        if (!member) { Enqueue(c, Error(403)); return; }
        Console.WriteLine($"[server] {c.Name} rời phòng {room}");
        Broadcast(room, new JsonObject { ["type"] = "sys", ["text"] = $"{c.Name} đã rời phòng {room}" }, null);
    }

    // BROADCAST (Mã giả 6.6, dòng 1–10); sender null: tin hệ thống
    public static void Broadcast(string room, JsonObject msg, ChatUser? sender)
    {
        var targets = new HashSet<string>();                             // phòng chưa tồn tại
        lock (Gate)                                                      // giữ khóa ngắn
        {
            if (Rooms.TryGetValue(room, out var m)) targets = new HashSet<string>(m);   // COPY
        }
        if (sender != null && !targets.Contains(sender.Name))
        {
            Enqueue(sender, Error(403));
            return;
        }
        foreach (var name in targets) SendTo(name, msg, null);
    }

    // SEND_TO (Mã giả 6.6, dòng 12–18)
    public static void SendTo(string name, JsonObject msg, ChatUser? sender)
    {
        ChatUser? c;
        lock (Gate) Users.TryGetValue(name, out c);
        if (c != null) Enqueue(c, msg);
        else if (sender != null) Enqueue(sender, Error(404));
    }

    // ENQUEUE (Mã giả 6.6, dòng 20–24): TryWrite không bao giờ chờ
    public static void Enqueue(ChatUser c, JsonObject msg)
    {
        // mỗi hàng đợi giữ bản sao riêng vì JsonObject không an toàn khi nhiều luồng ghi cùng đọc
        if (!c.Outbox.Writer.TryWrite((JsonObject)msg.DeepClone()))      // hàng đợi đầy (hoặc đã đóng)
        {
            Console.WriteLine($"[server] Ngắt client chậm {c.Name}");
            c.Sock.Close();                                              // CLOSE(c.sock)
        }
    }

    // WRITER (Mã giả 6.6, dòng 26–34): tác vụ ghi duy nhất của mỗi socket
    public static async Task Writer(ChatUser c)
    {
        try
        {
            await foreach (var msg in c.Outbox.Reader.ReadAllAsync())    // kết thúc khi outbox đóng
                await AsyncFrame.SendJsonAsync(c.Stream, msg, CancellationToken.None);
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is ObjectDisposedException)
        {
            c.Sock.Close();                                              // phiên sẽ kết thúc theo
        }
    }
}
