using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Unicode;

namespace Bai6_5;

// Mã giả 6.7, thủ tục RESPONDER – phía server (Mục 6.4.2): nhận yêu cầu discover, chờ ngẫu nhiên
// 0–200 ms (giãn phản hồi) rồi trả lời bằng unicast kèm cổng dịch vụ. Trong server thật, RESPONDER
// chạy như một tác vụ nền bên cạnh vòng chấp nhận kết nối.
static class Responder
{
    static readonly JsonSerializerOptions Opts = new() { Encoder = JavaScriptEncoder.Create(UnicodeRanges.All) };

    public static async Task<int> RunAsync(int servicePort, string name, int discPort)
    {
        using var s = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp); // SOCKET(INET, DGRAM)
        s.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);         // nhiều responder/máy
        s.Bind(new IPEndPoint(IPAddress.Any, discPort));                                          // BIND 0.0.0.0
        Console.WriteLine($"[responder] {name}, cổng dịch vụ {servicePort}, chờ discover tại cổng {discPort}");
        var buf = new byte[2048];
        while (true)
        {
            var res = await s.ReceiveFromAsync(buf, SocketFlags.None, new IPEndPoint(IPAddress.Any, 0)); // AWAIT RECVFROM_ASYNC
            JsonObject? q;
            try { q = JsonNode.Parse(Encoding.UTF8.GetString(buf, 0, res.ReceivedBytes)) as JsonObject; }
            catch (JsonException) { continue; }                         // sai định dạng: bỏ
            if (q == null || Str(q, "type") != "discover" || Str(q, "service") != "chat") continue;
            var addr = (IPEndPoint)res.RemoteEndPoint;
            int delay = Random.Shared.Next(0, 201);
            Console.WriteLine($"[responder] discover id={q["id"]} từ {addr}, trả lời sau {delay} ms");
            await Task.Delay(delay);                                     // AWAIT DELAY_ASYNC: giãn phản hồi
            var r = new JsonObject { ["id"] = q["id"]?.DeepClone(), ["port"] = servicePort, ["name"] = name };
            await s.SendToAsync(Encoding.UTF8.GetBytes(r.ToJsonString(Opts)), SocketFlags.None, addr);   // unicast
        }
    }

    static string? Str(JsonObject o, string key) => o[key] is JsonValue v && v.TryGetValue(out string? s) ? s : null;
}
