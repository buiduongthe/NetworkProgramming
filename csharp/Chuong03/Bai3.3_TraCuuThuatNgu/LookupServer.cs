using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace Bai3_3;

// Mã giả 3.7 – Server tra cứu thuật ngữ dùng khung JSON (Mục 3.5.1), cổng mặc định 9100.
// Kế thừa: khung sườn ECHO_SERVER (Mã giả 3.1) với HANDLE_ECHO thay bằng HandleLookup; SEND_JSON/RECV_JSON (Mã giả 3.6)
static class LookupServer
{
    static JsonObject Dict = new();                                     // DICT

    // Nghiệp vụ, độc lập với socket: nhận đối tượng yêu cầu, trả đối tượng phản hồi
    public static JsonObject ProcessRequest(JsonNode? req)
    {
        string? op = OpOf(req);
        if (op == "lookup")
        {
            string? term = (req!["term"] as JsonValue)?.TryGetValue<string>(out var t) == true ? t : null;
            if (term != null && Dict.TryGetPropertyValue(term, out var text))
                return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text!.GetValue<string>() };
            return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
        }
        if (op == "list")
            return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray([.. Dict.Select(kv => (JsonNode?)kv.Key)]) };
        return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };   // OTHERWISE
    }

    static string? OpOf(JsonNode? req) =>
        req is JsonObject o && o["op"] is JsonValue v && v.TryGetValue<string>(out var op) ? op : null;

    static void HandleLookup(Socket conn, EndPoint? addr)
    {
        while (true)
        {
            JsonNode? req;
            try
            {
                req = Framing.RecvJson(conn);
            }
            catch (ConnectionClosed e) when (e.Received == 0)
            {
                return;                                                 // đóng tại ranh giới
            }                                                           // đóng giữa khung: lỗi được ném tiếp
            catch (JsonError)
            {
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                Console.WriteLine($"[server] {addr}: (JSON sai) -> 400");
                continue;
            }
            if (req is not JsonObject)                                  // dòng 24–27: vd. [1, 2] hoặc "abc"
            {
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                Console.WriteLine($"[server] {addr}: (không phải đối tượng) -> 400");
                continue;
            }
            string op = OpOf(req) ?? "?";
            if (op == "bye") return;
            var resp = ProcessRequest(req);
            Framing.SendJson(conn, resp);
            string term = op == "lookup" ? $" '{req!["term"]}'" : "";
            Console.WriteLine($"[server] {addr}: {op}{term} -> {resp["status"]}");
        }
    }

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "0.0.0.0", 9100);
        Dict = JsonNode.Parse(File.ReadAllText("glossary.json"))!.AsObject();       // LOAD_JSON_FILE("glossary.json")
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET(INET, STREAM)
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // SETOPT(REUSEADDR)
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(16);                                                                               // LISTEN, backlog 16
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port} ({Dict.Count} thuật ngữ)");
        while (true)                                                    // vòng chấp nhận kết nối
        {
            using Socket conn = srv.Accept();                           // ACCEPT
            var addr = conn.RemoteEndPoint;
            Console.WriteLine($"[server] Client kết nối: {addr}");
            try
            {
                HandleLookup(conn, addr);
            }
            catch (Exception e) when (e is SocketException || e is ConnectionClosed || e is ProtocolError)
            {
                Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");   // không dừng server
            }
            Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
        }                                                               // 'using' đóng conn (CLOSE) ở cuối mỗi vòng
    }
}
