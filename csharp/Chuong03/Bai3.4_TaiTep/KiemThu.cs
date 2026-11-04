using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json.Nodes;

namespace Bai3_4;

// Client kiểm thử chức năng tải tệp (ca 8, 9 của tập ca kiểm thử, Mục 3.5.3): tên chứa "../" hoặc "..\",
// tên chỉ gồm thư mục, mã băm sai, kích thước vượt MAX_FILE, ngắt khi đang gửi nội dung. Mỗi ca một kết nối
static class KiemThu
{
    static string Host = "127.0.0.1";
    static int Port = 9100;
    static readonly byte[] Data = Enumerable.Repeat(Encoding.UTF8.GetBytes("Nội dung thử\n"), 10).SelectMany(b => b).ToArray();
    static readonly List<bool> Results = [];

    static Socket Connect()
    {
        var s = NetUtil.SafeConnect(Host, Port, 3000);
        s.ReceiveTimeout = 5000;
        return s;
    }

    static void Report(string no, string name, bool ok, string detail)
    {
        Results.Add(ok);
        Console.WriteLine($"[kiểm thử] Ca {no} – {name}: {(ok ? "ĐẠT" : "KHÔNG ĐẠT")} ({detail})");
    }

    static int Status(JsonNode? r) => r?["status"]?.GetValue<int>() ?? -1;

    // Gửi tiêu đề put; nếu server sẵn sàng (100) thì gửi nội dung và trả phản hồi cuối
    static JsonNode? PutRaw(Socket s, string name, byte[] data, string? sha256 = null, long size = -1)
    {
        Framing.SendJson(s, new JsonObject
        {
            ["op"] = "put", ["name"] = name, ["size"] = size < 0 ? data.Length : size,
            ["sha256"] = sha256 ?? Convert.ToHexStringLower(SHA256.HashData(data)),
        });
        var resp = Framing.RecvJson(s);
        if (Status(resp) != 100) return resp;
        Framing.SendAll(s, data);
        return Framing.RecvJson(s);
    }

    public static int Run(string[] args)
    {
        (Host, Port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9100);
        Console.WriteLine($"[kiểm thử] Server {Host}:{Port}");
        using (var s = Connect())
        {
            var r = PutRaw(s, "../thu.txt", Data);                      // Ca 8a
            Report("8a", "tên '../thu.txt'", Status(r) == 200 && $"{r!["saved"]}" == "thu.txt", $"status {Status(r)}, lưu '{r?["saved"]}'");
            r = PutRaw(s, "..\\..\\thu2.txt", Data);                    // Ca 8b
            Report("8b", "tên '..\\..\\thu2.txt'", Status(r) == 200 && $"{r!["saved"]}" == "thu2.txt", $"status {Status(r)}, lưu '{r?["saved"]}'");
            r = PutRaw(s, "../", Data);                                 // Ca 8c
            Report("8c", "tên '../' (không còn tên tệp)", Status(r) == 400, $"bị từ chối: status {Status(r)}, {r?["error"]}");
            r = PutRaw(s, "sai_bam.txt", Data, sha256: new string('0', 64));   // Ca 9
            Report("9", "mã băm khai báo sai", Status(r) == 422, $"status {Status(r)}, {r?["error"]}");
            r = PutRaw(s, "lon.bin", Data, size: 200L * 1048576);       // Ca bổ sung 1
            Report("bổ sung 1", "size 200 MiB > MAX_FILE", Status(r) == 400, $"status {Status(r)}, {r?["error"]}");
            Framing.SendJson(s, new JsonObject { ["op"] = "bye" });
        }
        JsonNode? r100;
        using (var s = Connect())                                       // Ca bổ sung 2: ngắt giữa chừng
        {
            Framing.SendJson(s, new JsonObject { ["op"] = "put", ["name"] = "dang_do.txt", ["size"] = 1000, ["sha256"] = new string('0', 64) });
            r100 = Framing.RecvJson(s);
            Framing.SendAll(s, Data[..100]);                            // chỉ gửi 100/1000 byte rồi đóng
        }
        using (var s = Connect())
        {
            Framing.SendJson(s, new JsonObject { ["op"] = "list" });
            var r2 = Framing.RecvJson(s);
            Report("bổ sung 2", "ngắt sau 100/1000 byte", Status(r100) == 100 && Status(r2) == 200,
                   $"server vẫn phục vụ client mới, list -> {Status(r2)}");
            Framing.SendJson(s, new JsonObject { ["op"] = "bye" });
        }
        Console.WriteLine($"[kiểm thử] Kết quả: {Results.Count(x => x)}/{Results.Count} ca đạt");
        return 0;
    }
}
