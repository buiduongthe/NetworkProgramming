using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai3_2;

// Echo Server dùng khung JSON: khung sườn ECHO_SERVER (Mã giả 3.1), thay HANDLE_ECHO bằng HandleFrames
// dùng RECV_FRAME/SEND_JSON (Mã giả 3.6). {"text": ...} -> {"status":200,"text":...}; yêu cầu sai -> 400
static class FrameServer
{
    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9000);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET(INET, STREAM)
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // SETOPT(REUSEADDR)
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(16);                                                                               // LISTEN, backlog 16
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}");
        while (true)                                                    // vòng chấp nhận kết nối
        {
            using Socket conn = srv.Accept();                           // ACCEPT
            var addr = conn.RemoteEndPoint;
            Console.WriteLine($"[server] Client kết nối: {addr}");
            try
            {
                HandleFrames(conn, addr);
            }
            catch (Exception e) when (e is SocketException || e is ConnectionClosed || e is ProtocolError)
            {
                Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");   // không dừng server
            }
            Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
        }                                                               // 'using' đóng conn (CLOSE) ở cuối mỗi vòng
    }

    // Vòng phục vụ một client: mỗi khung nhận được là một thông điệp hoàn chỉnh
    static void HandleFrames(Socket conn, EndPoint? addr)
    {
        while (true)
        {
            byte[] payload;
            try
            {
                payload = Framing.RecvFrame(conn);                      // RECV_JSON tách hai bước để ghi kích thước khung
            }
            catch (ConnectionClosed e) when (e.Received == 0)
            {
                return;                                                 // đóng tại ranh giới khung: kết thúc bình thường
            }                                                           // đóng giữa khung: lỗi được ném tiếp
            Console.WriteLine($"[server] Nhận khung {payload.Length} byte từ {addr}");
            JsonNode? req;
            try { req = Framing.JsonDecode(payload); }
            catch (JsonError)
            {
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                continue;
            }
            if (req is JsonObject o && o["text"] is JsonValue v && v.TryGetValue<string>(out var text))
                Framing.SendJson(conn, new JsonObject { ["status"] = 200, ["text"] = text });
            else
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "Thiếu trường text" });
        }
    }
}
