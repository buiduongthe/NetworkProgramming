using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai5_1;

// Client tra cứu bất đồng bộ dùng khung JSON (minh họa Mã giả 5.1); kế thừa Mã giả 3.8 (LOOKUP_CLIENT).
// In 4 byte tiêu đề của mỗi khung và minh họa hủy một lời gọi nhận đang chờ (Mục 5.4.1).
static class FrameClient
{
    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args, "127.0.0.1", 9100);
        Socket s;
        try { s = await Net.SafeConnectAsync(host, port, 3000); }
        catch (NetError e) { Console.WriteLine($"[client] Không kết nối được: {e.Message}"); return 1; }
        using var stream = new NetworkStream(s, ownsSocket: true);
        Console.WriteLine($"[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).");
        try
        {
            string? term;
            while ((term = await Console.In.ReadLineAsync()) != null)  // READ_CONSOLE bất đồng bộ
            {
                var req = new JsonObject { ["op"] = "lookup", ["term"] = term };
                byte[] frame = Khung.MakeFrame(req);
                Console.WriteLine($"[client] Gửi khung {frame.Length} byte, tiêu đề "
                    + $"{BitConverter.ToString(frame, 0, 4).Replace('-', ' ')}: {Khung.JsonText(req)}");
                using var timeout = new CancellationTokenSource(5000);  // RECV_TIMEOUT 5000 ms
                await Khung.SendJsonAsync(stream, req, timeout.Token);
                JsonNode resp = await Khung.RecvJsonAsync(stream, timeout.Token);
                if ((int?)resp["status"] == 200)
                    Console.WriteLine($"[client] {resp["term"]}: {resp["text"]}");
                else
                    Console.WriteLine($"[client] Lỗi {resp["status"]}: {resp["error"]}");
            }
            // Minh họa hủy: chờ một khung mà server không gửi, tín hiệu hủy kích hoạt sau 500 ms
            using var cts = new CancellationTokenSource(500);           // CANCEL(ct) sau 500 ms
            try
            {
                await Khung.RecvJsonAsync(stream, cts.Token);
            }
            catch (OperationCanceledException)
            {
                Console.WriteLine("[client] RECV_JSON_ASYNC bị hủy sau 500 ms (Cancelled); "
                    + "trạng thái khung không xác định nên đóng kết nối");
            }
        }
        finally
        {
            stream.Close();                                             // CLOSE: không dùng tiếp kết nối đã hủy
            Console.WriteLine("[client] Đã đóng kết nối");
        }
        return 0;
    }
}
