using System.Net.WebSockets;
using System.Text;

namespace Bai7_3;

// Client WebSocket dùng lớp ClientWebSocket của .NET (thư viện tự bắt tay, che khung, ghép khung)
// Đọc từng dòng từ bàn phím, gửi khung văn bản, in khung trả lời; cuối cùng đóng với mã 1000
static class WsClient
{
    public static async Task<int> RunAsync(string[] args)
    {
        string host = args.Length >= 1 ? args[0] : "127.0.0.1";
        int port = args.Length >= 2 ? int.Parse(args[1]) : 8080;
        using var ws = new ClientWebSocket();
        ws.Options.CollectHttpResponseDetails = true;                   // để đọc mã trạng thái bắt tay
        using (var cts = new CancellationTokenSource(3000))
            await ws.ConnectAsync(new Uri($"ws://{host}:{port}/"), cts.Token);
        Console.WriteLine($"[client] Bắt tay thành công: HTTP/1.1 {(int)ws.HttpStatusCode} Switching Protocols");
        var buf = new byte[65536];
        string? line;
        while ((line = Console.ReadLine()) != null)
        {
            await ws.SendAsync(Encoding.UTF8.GetBytes(line), WebSocketMessageType.Text, true, default);
            var ms = new MemoryStream();
            WebSocketReceiveResult res;
            do                                                          // ghép đến khung có FIN = 1
            {
                res = await ws.ReceiveAsync(buf, default);
                ms.Write(buf, 0, res.Count);
            } while (!res.EndOfMessage);
            if (res.MessageType == WebSocketMessageType.Close)
            {
                Console.WriteLine($"[client] Server đóng kết nối, mã {(int?)res.CloseStatus}");
                return 0;
            }
            Console.WriteLine($"[client] Nhận: '{Encoding.UTF8.GetString(ms.ToArray())}'");
        }
        await ws.CloseAsync(WebSocketCloseStatus.NormalClosure, null, default);   // gửi đóng 1000, chờ server đáp
        Console.WriteLine($"[client] Server xác nhận đóng, mã {(int?)ws.CloseStatus}");
        Console.WriteLine("[client] Đã đóng kết nối");
        return 0;
    }
}
