using System.Net;
using System.Net.Sockets;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Bai4_2;

// Mã giả 4.3 – Server tra cứu thuật ngữ trên UDP (Mục 4.3.1)
// Kế thừa: Mã giả 4.1 (vòng RECVFROM/SENDTO), server tra cứu TCP (PROCESS_REQUEST, DICT)
static class UdpLookupServer
{
    const int MAX_DGRAM = 1200;                                                          // dòng 1
    static readonly Dictionary<string, string> DICT =                                    // LOAD_JSON_FILE
        JsonSerializer.Deserialize<Dictionary<string, string>>(File.ReadAllText("glossary.json"))!;

    // PROCESS_REQUEST: nghiệp vụ, độc lập với socket (dùng lại nguyên vẹn từ bản TCP)
    static JsonObject ProcessRequest(JsonObject req)
    {
        switch (Json.Str(req["op"]))
        {
            case "lookup":
                string? term = Json.Str(req["term"]);
                if (term != null && DICT.TryGetValue(term, out var text))
                    return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text };
                return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
            case "list":
                return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray(DICT.Keys.Select(k => (JsonNode?)k).ToArray()) };
            default:
                return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };
        }
    }

    public static int Run(string[] args)
    {
        var (host, port) = Endpoint.Parse(args.Take(2).ToArray(), "0.0.0.0", 9100);    // dòng 3
        int drop = args.Length > 2 ? int.Parse(args[2]) : 0;                             // mô phỏng mất gói (thêm)
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp); // dòng 4
        s.Bind(new IPEndPoint(IPAddress.Parse(host), port));                             // dòng 5: BIND
        Console.WriteLine($"[server] Đang lắng nghe UDP tại {host}:{port}, từ điển {DICT.Count} thuật ngữ");
        if (drop > 0) Console.WriteLine($"[server] Mô phỏng mất gói: bỏ qua {drop} datagram đầu tiên");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        var buf = new byte[65535];
        int count = 0;
        while (true)                                                                     // dòng 6: LOOP
        {
            EndPoint addr = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = s.ReceiveFrom(buf, ref addr); }                                    // dòng 7
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset) { continue; }
            if (++count <= drop)
            {
                Console.WriteLine($"[server] Bỏ qua datagram thứ {count} từ {addr} (mô phỏng mất gói)");
                continue;
            }
            JsonObject req, resp;
            try                                                                          // dòng 8–14
            {
                req = Json.FromBytes(buf, n);
                resp = ProcessRequest(req);
            }
            catch (JsonException)                                                        // CATCH JsonError
            {
                req = new JsonObject { ["id"] = null };
                resp = new JsonObject { ["status"] = 400, ["error"] = "JSON sai" };
            }
            resp["id"] = req["id"]?.DeepClone();                                         // dòng 15: ghép với yêu cầu
            byte[] outb = Json.ToBytes(resp);                                            // dòng 16
            if (outb.Length > MAX_DGRAM)                                                 // dòng 17–20
            {
                resp = new JsonObject { ["id"] = req["id"]?.DeepClone(), ["status"] = 413, ["error"] = "Quá lớn" };
                outb = Json.ToBytes(resp);
            }
            s.SendTo(outb, addr);                                                        // dòng 21
            Console.WriteLine($"[server] Yêu cầu id={Json.Text(req["id"])} op={Json.Str(req["op"])} từ {addr}"
                              + $" -> status={resp["status"]}, {outb.Length} byte");
        }
    }
}
