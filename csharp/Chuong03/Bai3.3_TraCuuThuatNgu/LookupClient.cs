using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai3_3;

// Mã giả 3.8 – Client tra cứu thuật ngữ (Mục 3.5.1): giữ phần kết nối của ECHO_CLIENT (Mã giả 3.2),
// thay vòng gửi nhận thô bằng SEND_JSON/RECV_JSON. Mỗi dòng nhập là một thuật ngữ; hết dữ liệu nhập thì gửi bye
static class LookupClient
{
    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9100);
        using Socket s = NetUtil.SafeConnect(host, port, 3000);
        s.ReceiveTimeout = 5000;                                        // RECV_TIMEOUT 5000 ms
        Console.WriteLine($"[client] Đã kết nối {host}:{port}. Nhập thuật ngữ cần tra (Ctrl+Z/Ctrl+D để kết thúc).");
        string? term;
        while ((term = Console.ReadLine()) != null)                     // READ_CONSOLE()
        {
            Framing.SendJson(s, new JsonObject { ["op"] = "lookup", ["term"] = term });
            var resp = Framing.RecvJson(s);
            if (resp?["status"]?.GetValue<int>() == 200)
                Console.WriteLine($"[client] {resp["term"]}: {resp["text"]}");
            else
                Console.WriteLine($"[client] Lỗi {resp?["status"]}: {resp?["error"]} ('{term}')");
        }
        Framing.SendJson(s, new JsonObject { ["op"] = "bye" });
        Console.WriteLine("[client] Đã đóng kết nối");                  // 'using' đóng s (CLOSE)
        return 0;
    }
}
