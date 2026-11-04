using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai5_6;

// Server tra cứu "khó tính" để chạy thử hạn chót và thử lại (khung JSON của Mã giả 5.1):
//   yêu cầu #1: trả lời chậm 2000 ms (vượt hạn chót của client);
//   yêu cầu #2: trả lời {status: 503, retry_after_ms: 300} – quá tải tạm thời;
//   từ yêu cầu #3: trả lời bình thường; trường delay_ms (nếu có) là thời gian xử lý giả lập.
static class FlakyServer
{
    static readonly Dictionary<string, string> Dict = new()
    {
        ["TCP"] = "Giao thức truyền tin cậy, hướng kết nối",
        ["UDP"] = "Giao thức datagram, không kết nối",
    };
    static int count;                                                   // số yêu cầu đã nhận

    static JsonNode Lookup(string? term) => term != null && Dict.TryGetValue(term, out var text)
        ? new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text }
        : new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };

    static async Task<JsonNode> AnswerAsync(JsonNode req)
    {
        int n = Interlocked.Increment(ref count);
        if (n == 1)
        {
            Console.WriteLine($"[server] Yêu cầu #{n}: cố ý trả lời chậm 2000 ms");
            await Task.Delay(2000);
            return Lookup((string?)req["term"]);
        }
        if (n == 2)
        {
            Console.WriteLine($"[server] Yêu cầu #{n}: trả lời 503, đề nghị chờ 300 ms");
            return new JsonObject { ["status"] = 503, ["error"] = "Quá tải tạm thời", ["retry_after_ms"] = 300 };
        }
        int delay = (int?)req["delay_ms"] ?? 0;
        Console.WriteLine($"[server] Yêu cầu #{n}: xử lý {delay} ms rồi trả lời");
        await Task.Delay(delay);
        return Lookup((string?)req["term"]);
    }

    static async Task SessionAsync(Socket s)
    {
        using var conn = new NetworkStream(s, ownsSocket: true);
        try
        {
            while (true)
            {
                JsonNode req = await Khung.RecvJsonAsync(conn, default);
                JsonNode resp = await AnswerAsync(req);
                await Khung.SendJsonAsync(conn, resp, default);
            }
        }
        catch (ConnectionClosedException e)
        {
            if (e.Received > 0) Console.WriteLine($"[server] Kết nối đóng giữa khung: {e.Message}");
        }
        catch (Exception e) when (e is IOException || e is ProtocolException)
        {
            Console.WriteLine($"[server] Không gửi được phản hồi, client đã đóng kết nối ({e.GetType().Name})");
        }
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args, "0.0.0.0", 9100);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        while (true)
            _ = SessionAsync(await srv.AcceptAsync());
    }
}
