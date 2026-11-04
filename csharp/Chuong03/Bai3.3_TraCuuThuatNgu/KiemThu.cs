using System.Buffers.Binary;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;

namespace Bai3_3;

// Client kiểm thử server tra cứu theo tập ca kiểm thử tối thiểu (Mục 3.5.3): tự tạo dãy byte gửi đi
// để thực hiện các ca 1–7, cộng một ca bổ sung cho thao tác không có trong đặc tả. Mỗi ca một kết nối riêng
static class KiemThu
{
    static string Host = "127.0.0.1";
    static int Port = 9100;
    static readonly List<bool> Results = [];

    static Socket Connect()
    {
        var s = NetUtil.SafeConnect(Host, Port, 3000);
        s.ReceiveTimeout = 5000;
        return s;
    }

    static byte[] FrameOf(JsonNode obj)                                 // khung hoàn chỉnh của một đối tượng JSON
    {
        byte[] data = Encoding.UTF8.GetBytes(Framing.JsonEncode(obj));
        var f = new byte[4 + data.Length];
        BinaryPrimitives.WriteUInt32BigEndian(f, (uint)data.Length);
        data.CopyTo(f, 4);
        return f;
    }

    static void Report(string no, string name, bool ok, string detail)
    {
        Results.Add(ok);
        Console.WriteLine($"[kiểm thử] Ca {no} – {name}: {(ok ? "ĐẠT" : "KHÔNG ĐẠT")} ({detail})");
    }

    static JsonNode? Lookup(Socket s, string term)
    {
        Framing.SendJson(s, new JsonObject { ["op"] = "lookup", ["term"] = term });
        return Framing.RecvJson(s);
    }

    static int Status(JsonNode? r) => r?["status"]?.GetValue<int>() ?? -1;
    static void Bye(Socket s) => Framing.SendJson(s, new JsonObject { ["op"] = "bye" });

    public static int Run(string[] args)
    {
        (Host, Port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9100);
        Console.WriteLine($"[kiểm thử] Server {Host}:{Port}");
        using (var s = Connect())                                       // Ca 1, 2
        {
            var r = Lookup(s, "TCP");
            Report("1", "thuật ngữ có trong từ điển", Status(r) == 200 && $"{r!["term"]}" == "TCP", $"status {Status(r)}");
            r = Lookup(s, "Bắt tay ba bước");
            Report("2", "thuật ngữ có dấu", Status(r) == 200 && $"{r!["term"]}" == "Bắt tay ba bước",
                   $"status {Status(r)}, term '{r?["term"]}'");
            Bye(s);
        }
        using (var s = Connect())                                       // Ca 3: gửi từng byte, cách nhau 100 ms
        {
            s.NoDelay = true;
            byte[] data = FrameOf(new JsonObject { ["op"] = "lookup", ["term"] = "UDP" });
            for (int i = 0; i < data.Length; i++)
            {
                Framing.SendAll(s, data[i..(i + 1)]);
                Thread.Sleep(100);
            }
            var r = Framing.RecvJson(s);
            Report("3", $"gửi {data.Length} byte từng byte một", Status(r) == 200 && $"{r!["term"]}" == "UDP",
                   $"status {Status(r)}, term '{r?["term"]}'");
            Bye(s);
        }
        using (var s = Connect())                                       // Ca 4: hai yêu cầu trong một lần SEND
        {
            Framing.SendAll(s, [.. FrameOf(new JsonObject { ["op"] = "lookup", ["term"] = "TCP" }),
                                .. FrameOf(new JsonObject { ["op"] = "lookup", ["term"] = "XYZ" })]);
            var r1 = Framing.RecvJson(s);
            var r2 = Framing.RecvJson(s);
            Report("4", "hai yêu cầu trong một lần SEND", Status(r1) == 200 && Status(r2) == 404,
                   $"status {Status(r1)} rồi {Status(r2)}");
            Bye(s);
        }
        using (var s = Connect())                                       // Ca 5: độ dài 0xFFFFFFFF > MAX_FRAME
        {
            Framing.SendAll(s, [0xFF, 0xFF, 0xFF, 0xFF]);
            bool closed;
            try { closed = s.Receive(new byte[1]) == 0; }
            catch (SocketException e) { closed = e.SocketErrorCode == SocketError.ConnectionReset; }
            Report("5", "độ dài 4294967295 > MAX_FRAME", closed, closed ? "server đóng kết nối" : "không đóng");
        }
        using (var s = Connect())                                       // Ca 6: nội dung không phải JSON
        {
            Framing.SendFrame(s, Encoding.UTF8.GetBytes("đây không phải JSON"));
            var r = Framing.RecvJson(s);
            Framing.SendJson(s, new JsonObject { ["op"] = "list" });   // kết nối vẫn dùng được
            var r2 = Framing.RecvJson(s);
            int n = (r2?["terms"] as JsonArray)?.Count ?? 0;
            Report("6", "nội dung không phải JSON", Status(r) == 400 && Status(r2) == 200,
                   $"status {Status(r)}, sau đó list -> {Status(r2)}, {n} thuật ngữ");
            Framing.SendJson(s, new JsonObject { ["op"] = "xoa", ["term"] = "TCP" });   // Ca bổ sung: thao tác sai
            r = Framing.RecvJson(s);
            Report("bổ sung", "thao tác \"xoa\"", Status(r) == 400, $"status {Status(r)}: {r?["error"]}");
            Framing.SendFrame(s, Encoding.UTF8.GetBytes("[1,2]"));          // Ca bổ sung: JSON là mảng
            r = Framing.RecvJson(s);
            Report("bổ sung", "JSON là mảng [1,2]", Status(r) == 400, $"status {Status(r)}: {r?["error"]}");
            Bye(s);
        }
        using (var s = Connect())                                       // Ca 7: đóng kết nối giữa khung
        {
            byte[] head = new byte[4];
            BinaryPrimitives.WriteUInt32BigEndian(head, 50);
            Framing.SendAll(s, [.. head, .. "0123456789"u8]);
        }
        using (var s = Connect())
        {
            var r = Lookup(s, "TCP");
            Report("7", "đóng giữa khung (14/54 byte)", Status(r) == 200, $"client mới vẫn được phục vụ, status {Status(r)}");
            Bye(s);
        }
        Console.WriteLine($"[kiểm thử] Kết quả: {Results.Count(x => x)}/{Results.Count} ca đạt");
        return 0;
    }
}
