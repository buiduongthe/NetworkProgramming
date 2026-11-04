using System.Text.Json.Nodes;
using System.Threading.Channels;

namespace Bai5_4;

// Giao diện dòng lệnh dùng lớp ChatClient (Mã giả 5.5): chỉ đăng ký OnMessage, OnStatus và gọi
// SendMessageAsync, Disconnect. Mỗi dòng nhập là một tin; /quit để thoát; /cho <giây> tạm dừng đọc
// bàn phím (dùng trong kịch bản minh họa). Bàn phím được đọc trên luồng phụ và chuyển qua Channel,
// nên vòng nhận vẫn chạy trong lúc chờ người dùng gõ.
static class ChatCli
{
    const string Room = "chung";                                        // phòng đang chọn; server tối giản chỉ có "chung"

    static void ShowMessage(string me, JsonNode msg)                    // trình xử lý sự kiện OnMessage
    {
        switch ((string?)msg["type"])
        {
            case "msg": Console.WriteLine($"[{me}] {msg["from"]}: {msg["text"]}"); break;
            case "sys": Console.WriteLine($"[{me}] (hệ thống) {msg["text"]}"); break;
            case "welcome":
                var users = msg["users"]!.AsArray().Select(u => (string?)u);
                Console.WriteLine($"[{me}] Server chào mừng; đang trực tuyến: {string.Join(", ", users)}"); break;
            case "error": Console.WriteLine($"[{me}] Lỗi {msg["code"]}: {msg["text"]}"); break;
            default: Console.WriteLine($"[{me}] Nhận: {Khung.JsonText(msg)}"); break;
        }
    }

    public static async Task<int> RunAsync(string[] args)
    {
        if (args.Length < 1) { Console.WriteLine("Cách dùng: dotnet run -- cli <tên> [host] [port]"); return 1; }
        string me = args[0];
        var (host, port) = Net.ParseEndpoint(args.Skip(1).ToArray(), "127.0.0.1", 9200);
        var c = new ChatClient();
        c.OnMessage += msg => ShowMessage(me, msg);                     // đăng ký sự kiện
        c.OnStatus += text => Console.WriteLine($"[{me}] Trạng thái: {text}");
        try { await c.ConnectAsync(host, port, me); }
        catch (NetError e) { Console.WriteLine($"[{me}] Không kết nối được: {e.Message}"); return 1; }
        var lines = Channel.CreateUnbounded<string?>();
        new Thread(() =>
        {
            string? line;
            while ((line = Console.ReadLine()) != null) lines.Writer.TryWrite(line);
            lines.Writer.TryWrite(null);                                // hết dữ liệu vào (Ctrl+D)
        }) { IsBackground = true }.Start();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; lines.Writer.TryWrite(null); };  // Ctrl+C: thoát gọn
        try
        {
            string? line;
            while ((line = await lines.Reader.ReadAsync()) != null && line != "/quit")
            {
                if (line.StartsWith("/cho ")) await Task.Delay(TimeSpan.FromSeconds(double.Parse(line[5..], System.Globalization.CultureInfo.InvariantCulture)));
                else if (line.Length > 0) await c.SendMessageAsync(Room, line);   // AWAIT SEND_MESSAGE(c, room, text)
            }
        }
        finally
        {
            c.Disconnect();
            Console.WriteLine($"[{me}] Đã ngắt kết nối");
        }
        return 0;
    }
}
