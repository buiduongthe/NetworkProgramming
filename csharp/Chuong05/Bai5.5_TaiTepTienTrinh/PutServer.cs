using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json.Nodes;

namespace Bai5_5;

// Server nhận tệp có kiểm tra mã băm – Mã giả 3.9 (HANDLE_PUT, Mục 3.5.2) trên nền bất đồng bộ của Mã giả 5.1:
// ghi vào tệp tạm .part, chỉ đổi tên khi SHA-256 khớp; mọi lỗi khi nhận (CATCH ANY) đều xóa tệp .part.
// rateKiB giới hạn tốc độ đọc (KiB/s) để mô phỏng đường truyền chậm; 0 = không giới hạn.
static class PutServer
{
    const long MaxFile = 100L * 1048576;
    const string UploadDir = "uploads";
    static int rateKiB;
    static readonly CancellationToken None = CancellationToken.None;

    // IS_HEX(v, n): v là chuỗi đúng n ký tự thập lục phân
    static bool IsHex(string? v, int n) => v != null && v.Length == n && v.All(Uri.IsHexDigit);

    static async Task HandlePutAsync(NetworkStream conn, JsonObject req)
    {
        string name = (Str(req["name"]) ?? "").Replace('\\', '/').Split('/')[^1];   // dòng 3: BASENAME(REPLACE(…))
        long size = req["size"] is JsonValue sv && sv.TryGetValue<long>(out var z) ? z : -1;   // phải là số nguyên
        string? sha = Str(req["sha256"]);
        if (name is "" or "." or ".." || size < 0 || size > MaxFile || !IsHex(sha, 64))   // dòng 4–6
        {
            await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 400, ["error"] = "Sai tham số" }, None);
            return;
        }
        Console.WriteLine($"[server] Nhận yêu cầu put: {name}, {size} byte");
        await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 100 }, None);   // sẵn sàng nhận nội dung
        Directory.CreateDirectory(UploadDir);
        string tmp = Path.Combine(UploadDir, name + ".part");
        using var h = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        long remaining = size;
        var buf = new byte[65536];
        var f = File.Create(tmp);
        try                                                             // dòng 14–26
        {
            while (remaining > 0)
            {
                int n = await conn.ReadAsync(buf.AsMemory(0, (int)Math.Min(65536, remaining)));
                if (n == 0) throw new ConnectionClosedException((int)(size - remaining));
                await f.WriteAsync(buf.AsMemory(0, n));
                h.AppendData(buf, 0, n);
                remaining -= n;
                if (rateKiB > 0) await Task.Delay(TimeSpan.FromSeconds(n / (rateKiB * 1024.0)));  // đường truyền chậm
            }
        }
        catch (Exception)                                               // CATCH ANY: xóa tệp dở dang rồi báo tiếp
        {
            f.Close(); File.Delete(tmp);
            Console.WriteLine($"[server] Lỗi khi mới nhận {size - remaining}/{size} byte: xóa {UploadDir}/{name}.part");
            throw;
        }
        f.Close();
        if (Convert.ToHexStringLower(h.GetHashAndReset()) == sha)
        {
            File.Move(tmp, Path.Combine(UploadDir, name), overwrite: true);
            Console.WriteLine($"[server] Đã lưu {UploadDir}/{name}, mã băm khớp (200)");
            await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 200, ["saved"] = name }, None);
        }
        else
        {
            File.Delete(tmp);
            Console.WriteLine("[server] Mã băm không khớp (422)");
            await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 422, ["error"] = "Mã băm không khớp" }, None);
        }
    }

    static string? Str(JsonNode? n) => n is JsonValue v && v.TryGetValue(out string? s) ? s : null;

    static async Task SessionAsync(Socket s)
    {
        string addr = s.RemoteEndPoint!.ToString()!;
        Console.WriteLine($"[server] Client kết nối: {addr}");
        using var conn = new NetworkStream(s, ownsSocket: true);
        try
        {
            while (true)
            {
                JsonNode node;
                try { node = await Khung.RecvJsonAsync(conn, None); }
                catch (ConnectionClosedException e) when (e.Received == 0) { break; }
                catch (System.Text.Json.JsonException)                  // Mã giả 3.7: JsonError → 400, tiếp tục
                {
                    await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" }, None);
                    continue;
                }
                if (node is not JsonObject req)                         // Mã giả 3.7 dòng 24–27
                {
                    await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 400, ["error"] = "JSON sai" }, None);
                    continue;
                }
                string? op = Str(req["op"]);
                if (op == "put") await HandlePutAsync(conn, req);
                else if (op == "bye") break;
                else await Khung.SendJsonAsync(conn, new JsonObject { ["status"] = 400, ["error"] = "Sai thao tác" }, None);
            }
        }
        catch (Exception e) when (e is ConnectionClosedException || e is ProtocolException || e is IOException)
        {
            Console.WriteLine($"[server] Lỗi với {addr}: {e.Message}");
        }
        Console.WriteLine($"[server] Client {addr} đã đóng kết nối");
    }

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args.Take(2).ToArray(), "0.0.0.0", 9100);
        rateKiB = args.Length > 2 ? int.Parse(args[2]) : 0;
        using var srv = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        srv.SetSocketOption(SocketOptionLevel.Socket, SocketOptionName.ReuseAddress, true);
        if (rateKiB > 0) srv.ReceiveBufferSize = 65536;                 // bộ đệm nhận nhỏ: mô phỏng đường truyền chậm
        srv.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        srv.Listen(16);
        Console.WriteLine($"[server] Đang lắng nghe tại {host}:{port}, giới hạn đọc {rateKiB} KiB/s");
        Console.CancelKeyPress += (_, _) => Console.WriteLine("[server] Dừng server");
        while (true)
            _ = SessionAsync(await srv.AcceptAsync());
    }
}
