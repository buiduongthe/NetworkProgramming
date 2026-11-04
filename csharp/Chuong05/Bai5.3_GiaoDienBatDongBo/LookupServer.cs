using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai5_3;

// Server tra cứu tối giản phục vụ ứng dụng có giao diện: dùng lại server của Bài 5.1
// (nghiệp vụ Mã giả 3.7, khung JSON Mã giả 5.1), thêm độ trễ giả lập delayMs trước mỗi phản hồi.
static class LookupServer
{
    static int delayMs;

    static readonly Dictionary<string, string> Dict = new()          // từ điển nhỏ thay cho glossary.json
    {
        ["TCP"] = "Giao thức truyền tin cậy, hướng kết nối",
        ["UDP"] = "Giao thức datagram, không kết nối",
        ["Socket"] = "Điểm cuối truyền thông của một chương trình",
        ["Bất đồng bộ"] = "Khởi động vào/ra rồi làm việc khác, quay lại khi xong",
    };

    static JsonNode ProcessRequest(JsonNode req)                     // Mã giả 3.7: độc lập với socket
    {
        string? op = (string?)req["op"];
        if (op == "lookup")
        {
            string term = (string?)req["term"] ?? "";
            if (Dict.TryGetValue(term, out var text))
                return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text };
            return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
        }
        if (op == "list") return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray(Dict.Keys.Select(k => (JsonNode)k!).ToArray()) };
        return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };
    }

    static async Task HandleLookupAsync(Socket conn)                 // HANDLE_LOOKUP, phiên bản bất đồng bộ
    {
        string addr = conn.RemoteEndPoint!.ToString()!;
        Console.WriteLine($"[server] Client kết nối: {addr}");
        using var stream = new NetworkStream(conn, ownsSocket: true);
        var ct = CancellationToken.None;
        try
        {
            while (true)
            {
                JsonNode req;
                try
                {
                    req = await Khung.RecvJsonAsync(stream, ct);         // AWAIT: chờ khung mà không chặn luồng
                }
                catch (ConnectionClosedException e) when (e.Received == 0)
                {
                    break;                                              // đóng tại ranh giới khung: bình thường
                }
                catch (JsonException)
                {
                    await Khung.SendJsonAsync(stream, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" }, ct);
                    continue;
                }
                Console.WriteLine($"[server] Nhận từ {addr}: {Khung.JsonText(req)}");
                if ((string?)req["op"] == "bye") break;
                await Task.Delay(delayMs, ct);                          // độ trễ giả lập của server
                await Khung.SendJsonAsync(stream, ProcessRequest(req), ct);
            }
        }
        catch (Exception e) when (e is ConnectionClosedException || e is ProtocolException || e is IOException)
        {
            Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");   // lỗi một client không dừng server
        }
        Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args.Take(2).ToArray(), "0.0.0.0", 9100);
        delayMs = args.Length > 2 ? int.Parse(args[2]) : 0;
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));          // BIND
        srv.Listen(16);                                                 // LISTEN, backlog 16
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}, độ trễ giả lập {delayMs} ms");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        while (true)
        {
            Socket conn = await srv.AcceptAsync();                      // AWAIT ACCEPT_ASYNC
            _ = HandleLookupAsync(conn);                                // RUN_BACKGROUND: mỗi kết nối một Task
        }
    }
}
