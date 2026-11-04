using System.Diagnostics;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json.Nodes;

namespace Bai5_5;

// Mã giả 5.6 – PUT_FILE_ASYNC: tải tệp bất đồng bộ có báo cáo tiến trình và hủy (Mục 5.3.4).
// Kế thừa Mã giả 3.10 (PUT_CLIENT): giữ nguyên giao thức và trình tự 4 bước; thêm async/await,
// onProgress (tối đa ~10 lần/giây) và CancellationToken ct. Nếu tệp chưa có, tạo tệp thử 4 MiB.
static class PutClient
{
    public static async Task<JsonNode> PutFileAsync(NetworkStream s, string path,
        Action<long, long> onProgress, CancellationToken ct)
    {
        long size = new FileInfo(path).Length;
        string digest;
        using (var fs = File.OpenRead(path))
            digest = Convert.ToHexStringLower(await SHA256.HashDataAsync(fs, ct));   // tệp lớn: lâu
        var hdr = new JsonObject { ["op"] = "put", ["name"] = Path.GetFileName(path), ["size"] = size, ["sha256"] = digest };
        await Khung.SendJsonAsync(s, hdr, ct);
        if ((int?)(await Khung.RecvJsonAsync(s, ct))["status"] != 100)
            throw new ProtocolException("Server từ chối");
        var f = File.OpenRead(path);
        long sent = 0, last = 0;
        var buf = new byte[65536];
        try
        {
            while (true)
            {
                int n = await f.ReadAsync(buf, ct);                             // READ_ASYNC(f, 65536, ct)
                if (n == 0) break;
                await s.WriteAsync(buf.AsMemory(0, n), ct);                     // SEND_ALL_ASYNC: điểm có thể hủy
                sent += n;                                                      // đã vào bộ đệm gửi, chưa chắc tới server
                if (Environment.TickCount64 - last >= 100)                      // NOW() - last ≥ 100: tối đa ~10 lần/giây
                {
                    onProgress(sent, size); last = Environment.TickCount64;
                }
            }
        }
        finally
        {
            f.Close();                                                          // CLOSE_FILE(f)
        }
        JsonNode resp = await Khung.RecvJsonAsync(s, ct);                       // dòng 24: server đã nhận đủ
        if ((int?)resp["status"] == 200) onProgress(size, size);                // dòng 25: 100% khi server xác nhận
        return resp;
    }

    static void ShowProgress(long sent, long size) =>                           // onProgress
        Console.WriteLine($"[client] Tiến trình: {sent / 1048576.0:F2}/{size / 1048576.0:F2} MiB ({sent * 100 / size}%)");

    public static async Task<int> RunAsync(string[] args)
    {
        var (host, port) = Net.ParseEndpoint(args.Take(2).ToArray(), "127.0.0.1", 9100);
        string path = args.Length > 2 ? args[2] : "du_lieu_thu.bin";
        int cancelMs = args.Length > 3 ? int.Parse(args[3]) : 0;
        if (!File.Exists(path))
        {
            var data = new byte[4 * 1048576];
            new Random(5).NextBytes(data);
            File.WriteAllBytes(path, data);
            Console.WriteLine($"[client] Tạo tệp thử {path} (4 MiB)");
        }
        Socket sock;
        try { sock = await Net.SafeConnectAsync(host, port, 3000); }
        catch (NetError e) { Console.WriteLine($"[client] Không kết nối được: {e.Message}"); return 1; }
        sock.SendBufferSize = 65536;            // bộ đệm gửi nhỏ để số byte "đã gửi" bám sát số byte server đã nhận
        using var s = new NetworkStream(sock, ownsSocket: true);
        Console.WriteLine($"[client] Đã kết nối {host}:{port}, tải lên {path}"
            + (cancelMs > 0 ? $", sẽ bấm Hủy sau {cancelMs} ms" : ""));
        using var cts = new CancellationTokenSource();                         // nguồn tín hiệu hủy
        if (cancelMs > 0) cts.CancelAfter(cancelMs);                           // nút Hủy: CANCEL(cts)
        var t0 = Stopwatch.StartNew();
        try
        {
            JsonNode resp = await PutFileAsync(s, path, ShowProgress, cts.Token);
            Console.WriteLine($"[client] Kết quả: {Khung.JsonText(resp)} sau {t0.Elapsed.TotalSeconds:F2} s");
        }
        catch (OperationCanceledException)
        {
            Console.WriteLine($"[client] Đã hủy sau {t0.Elapsed.TotalSeconds:F2} s; đóng kết nối (trạng thái giao thức không còn tin cậy)");
        }
        catch (Exception e) when (e is ProtocolException || e is IOException)
        {
            Console.WriteLine($"[client] Lỗi: {e.Message}");
        }
        finally
        {
            s.Close();                                                         // hủy giữa chừng: đóng kết nối (Mục 5.4.1)
        }
        return 0;
    }
}
