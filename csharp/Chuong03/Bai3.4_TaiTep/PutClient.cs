using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json.Nodes;

namespace Bai3_4;

// Mã giả 3.10 – PUT_FILE (Mục 3.5.2) trên nền client tra cứu của Mã giả 3.8: một kết nối cho cả tra cứu và tải tệp.
// Mỗi dòng nhập là một lệnh:  put <đường dẫn tệp>  – tải tệp lên;  <thuật ngữ>  – tra cứu
static class PutClient
{
    static void PutFile(Socket s, string path)
    {
        string digest;
        using (var fs = File.OpenRead(path)) digest = Convert.ToHexStringLower(SHA256.HashData(fs));  // SHA256_OF_FILE
        string name = Path.GetFileName(path);
        long size = new FileInfo(path).Length;
        Console.WriteLine($"[client] Tải lên {name}: {size} byte, SHA-256 {digest[..16]}…");
        Framing.SendJson(s, new JsonObject { ["op"] = "put", ["name"] = name, ["size"] = size, ["sha256"] = digest });
        var resp = Framing.RecvJson(s);
        if (resp?["status"]?.GetValue<int>() != 100) throw new ProtocolError($"{resp?["error"]}");
        using (var f = File.OpenRead(path))                             // OPEN_FILE(path, READ)
        {
            var buf = new byte[65536];
            int n;
            while ((n = f.Read(buf, 0, buf.Length)) > 0)                // READ(f, 65536)
                Framing.SendAll(s, buf, n);
        }
        resp = Framing.RecvJson(s);
        Console.WriteLine($"[client] Kết quả: {resp?["status"]} {Framing.JsonEncode(resp!)}");
    }

    public static int Run(string[] args)
    {
        var (host, port) = NetUtil.ParseEndpoint(args, "127.0.0.1", 9100);
        using Socket s = NetUtil.SafeConnect(host, port, 3000);
        s.ReceiveTimeout = 5000;                                        // RECV_TIMEOUT 5000 ms
        Console.WriteLine($"[client] Đã kết nối {host}:{port}. Nhập 'put <tệp>' hoặc thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
        string? line;
        while ((line = Console.ReadLine()) != null)                     // READ_CONSOLE()
        {
            if (line.StartsWith("put "))
            {
                string path = line[4..].Trim();
                try { PutFile(s, path); }
                catch (FileNotFoundException) { Console.WriteLine($"[client] Lỗi: không tìm thấy tệp '{path}'"); }
                catch (ProtocolError e) { Console.WriteLine($"[client] Lỗi: {e.Message}"); }   // server không trả 100
                continue;
            }
            Framing.SendJson(s, new JsonObject { ["op"] = "lookup", ["term"] = line });
            var resp = Framing.RecvJson(s);
            if (resp?["status"]?.GetValue<int>() == 200)
                Console.WriteLine($"[client] {resp["term"]}: {resp["text"]}");
            else
                Console.WriteLine($"[client] Lỗi {resp?["status"]}: {resp?["error"]} ('{line}')");
        }
        Framing.SendJson(s, new JsonObject { ["op"] = "bye" });
        Console.WriteLine("[client] Đã đóng kết nối");                  // 'using' đóng s (CLOSE)
        return 0;
    }
}
