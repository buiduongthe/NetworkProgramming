using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json.Nodes;

namespace Bai3_4;

// Mã giả 3.9 – Server tra cứu và nhận tệp có kiểm tra mã băm SHA-256 (Mục 3.5.2). HANDLE_PUT được gắn vào
// HANDLE_LOOKUP (Mã giả 3.7) bằng nhánh  IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE.
// An toàn: BASENAME chặn "../"; giới hạn MAX_FILE; ghi tệp tạm .part, chỉ đổi tên khi mã băm khớp
static class PutServer
{
    const long MaxFile = 100L * 1048576;
    const string UploadDir = "uploads";
    static JsonObject Dict = new();                                     // DICT

    // Mã giả 3.7 – nghiệp vụ tra cứu, độc lập với socket
    static JsonObject ProcessRequest(JsonNode? req)
    {
        string? op = Str(req, "op");
        if (op == "lookup")
        {
            string? term = Str(req, "term");
            if (term != null && Dict.TryGetPropertyValue(term, out var text))
                return new JsonObject { ["status"] = 200, ["term"] = term, ["text"] = text!.GetValue<string>() };
            return new JsonObject { ["status"] = 404, ["error"] = "Không có thuật ngữ" };
        }
        if (op == "list")
            return new JsonObject { ["status"] = 200, ["terms"] = new JsonArray([.. Dict.Select(kv => (JsonNode?)kv.Key)]) };
        return new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" };
    }

    static string? Str(JsonNode? o, string key) =>
        o is JsonObject obj && obj[key] is JsonValue v && v.TryGetValue<string>(out var s) ? s : null;

    // BASENAME(REPLACE(name, "\\", "/")): đổi '\' thành '/' rồi bỏ mọi thành phần thư mục
    static string BaseName(string? name) => name == null ? "" : name.Replace('\\', '/').Split('/')[^1];

    // IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân
    static bool IsHex(string? v, int n) => v != null && v.Length == n && v.All(Uri.IsHexDigit);

    // Mã giả 3.9 – req = {op, name, size, sha256}; trả chuỗi kết quả để ghi nhật ký
    static string HandlePut(Socket conn, JsonObject req)
    {
        string name = BaseName(Str(req, "name"));                       // dòng 3: bỏ thư mục, chặn "../"
        long size = req["size"] is JsonValue sv && sv.TryGetValue<long>(out var z) ? z : -1;
        string? sha = Str(req, "sha256");
        if (name is "" or "." or ".." || size < 0 || size > MaxFile || !IsHex(sha, 64))   // dòng 4–6
        {
            Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "Sai tham số" });
            return "400 (Sai tham số)";
        }
        Framing.SendJson(conn, new JsonObject { ["status"] = 100 });     // sẵn sàng nhận nội dung
        string tmp = Path.Combine(UploadDir, name + ".part");
        var f = File.Create(tmp);                                       // OPEN_FILE(tmp, WRITE)
        using var h = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);   // SHA256_INIT()
        long remaining = size;
        var buf = new byte[65536];
        try
        {
            while (remaining > 0)
            {
                int n = conn.Receive(buf, 0, (int)Math.Min(65536, remaining), SocketFlags.None);
                if (n == 0)
                    throw new ConnectionClosed((int)(size - remaining), $"Tệp chưa nhận đủ ({size - remaining}/{size} byte)");
                f.Write(buf, 0, n);
                h.AppendData(buf, 0, n);                                // SHA256_UPDATE
                remaining -= n;
            }
        }
        catch (Exception)                                               // dòng 23–25 CATCH ANY: xóa tệp dở dang
        {
            f.Dispose();
            File.Delete(tmp);
            throw;
        }
        f.Dispose();                                                    // CLOSE_FILE(f)
        if (Convert.ToHexStringLower(h.GetHashAndReset()) == sha)       // HEX(SHA256_FINAL(h))
        {
            File.Move(tmp, Path.Combine(UploadDir, name), overwrite: true);   // RENAME (ghi đè nếu đã có)
            Framing.SendJson(conn, new JsonObject { ["status"] = 200, ["saved"] = name });
            return $"200, lưu {UploadDir}/{name} ({size} byte)";
        }
        File.Delete(tmp);
        Framing.SendJson(conn, new JsonObject { ["status"] = 422, ["error"] = "Mã băm không khớp" });
        return "422 (Mã băm không khớp)";
    }

    // Mã giả 3.7 – HANDLE_LOOKUP, bổ sung nhánh "put" theo Mã giả 3.9
    static void HandleLookup(Socket conn, EndPoint? addr)
    {
        while (true)
        {
            JsonNode? req;
            try { req = Framing.RecvJson(conn); }
            catch (ConnectionClosed e) when (e.Received == 0) { return; }   // đóng tại ranh giới
            catch (JsonError)
            {
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                continue;
            }
            if (req is not JsonObject)                                  // Mã giả 3.7 dòng 24–27
            {
                Framing.SendJson(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" });
                continue;
            }
            string op = Str(req, "op") ?? "?";
            if (op == "bye") return;
            if (op == "put")
            {
                Console.WriteLine($"[server] {addr}: put '{req!["name"]}' -> {HandlePut(conn, req.AsObject())}");
                continue;
            }
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
        Directory.CreateDirectory(UploadDir);
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp); // SOCKET(INET, STREAM)
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);          // SETOPT(REUSEADDR)
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));                                        // BIND
        srv.Listen(16);                                                                               // LISTEN, backlog 16
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}, thư mục tải lên: {UploadDir}/");
        while (true)
        {
            using Socket conn = srv.Accept();                           // ACCEPT
            var addr = conn.RemoteEndPoint;
            Console.WriteLine($"[server] Client kết nối: {addr}");
            try { HandleLookup(conn, addr); }
            catch (Exception e) when (e is SocketException || e is ConnectionClosed || e is ProtocolError || e is IOException)
            {
                Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");   // không dừng server
            }
            Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
        }                                                               // 'using' đóng conn (CLOSE) ở cuối mỗi vòng
    }
}
