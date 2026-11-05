// Bài mở rộng MR5.5 – Server bất đồng bộ trên vòng lặp một luồng, phối hợp với ThreadPool và luồng nền.
// Các luồng của tiến trình server:
//   vòng-lặp  – luồng chính chạy VongLap: nhận kết nối, đọc/ghi socket bằng async I/O, nhịp tim 10 ms;
//   nhóm#k    – luồng của ThreadPool: Task.Run(...) cho hàm thư viện cũ kiểu chặn; việc băm (CPU) cũng chạy
//               trên ThreadPool nhưng qua TaskScheduler giới hạn 2 việc cùng lúc (ConcurrentExclusiveSchedulerPair);
//   cảm-biến  – một Thread riêng chạy vòng lặp chặn của nó, đẩy mẫu đo VÀO vòng lặp qua Channel<T>,
//               và hoàn tất TaskCompletionSource khi vòng lặp nhờ đọc ngay.
// Ranh giới async ↔ luồng:
//   async → luồng : await Task.Run(...) – vòng lặp rảnh trong lúc chờ, phần sau await quay về vòng lặp;
//   luồng → async : ChannelWriter.TryWrite, TaskCompletionSource.SetResult, SynchronizationContext.Post;
//   hủy, hạn chót : task.WaitAsync(hạn) ném TimeoutException → CancellationTokenSource.Cancel() → luồng
//                   băm thấy token.IsCancellationRequested ở vòng kế tiếp và tự dừng.
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Threading.Channels;
using static BaiMR5_5.Chung;

namespace BaiMR5_5;

sealed class MayChu(VongLap loop)
{
    static readonly TaskScheduler NhomTinhLich =
        new ConcurrentExclusiveSchedulerPair(TaskScheduler.Default, NhomTinh).ConcurrentScheduler;
    readonly SemaphoreSlim congVao = new(GioiHan, GioiHan);   // WaitAsync() chờ KHÔNG chặn vòng lặp
    readonly BlockingCollection<TaskCompletionSource<(int, string)>> ycCamBien = new();
    readonly Channel<(int K, double TTao)> kenhMau = Channel.CreateBounded<(int, double)>(256);

    // Thống kê – chỉ luồng vòng lặp đọc/ghi nên không cần khóa
    double treMau;
    int soMau, oNhom, oNhomMax, soYC;
    readonly Dictionary<string, SortedSet<string>> luong = new();

    void DatLai()
    {
        loop.TreNhipMax = 0;
        treMau = 0;
        soMau = oNhomMax = soYC = 0;
        foreach (var gd in new[] { "nhận/trả lời", "tra cứu cũ", "băm SHA-256" }) luong[gd] = new(StringComparer.Ordinal);
    }

    // ---------- luồng cảm biến: luồng HĐH → thế giới async ----------
    public void CamBien()
    {
        int k = 0;
        while (true)
        {
            if (ycCamBien.TryTake(out var tcs, CamBienMs))      // vòng lặp nhờ đọc ngay
            {
                k++;
                tcs.SetResult(((k * 37) % 100, TenLuong()));    // hoàn tất từ luồng khác: an toàn
            }
            else
            {
                k++;
                kenhMau.Writer.TryWrite((k, BayGioMs()));       // đẩy mẫu định kỳ vào Channel<T>
            }
        }
    }

    public async Task NhanMauAsync()
    {
        // await foreach bắt context của vòng lặp → thân vòng lặp này chạy trên luồng vòng-lặp
        await foreach (var m in kenhMau.Reader.ReadAllAsync())
        {
            soMau++;
            treMau = Math.Max(treMau, BayGioMs() - m.TTao);
        }
    }

    // ---------- B, C: hàm chặn và việc băm đều đẩy sang ThreadPool ----------
    async Task<(string Hex, string Ten)> TachAsync(string ma, byte[] data, int vong, CancellationToken ct)
    {
        luong["tra cứu cũ"].Add(await Task.Run(TraCuuCu));   // phần sau await quay về vòng lặp
        ct.ThrowIfCancellationRequested();                    // đã quá hạn khi đang tra cứu → không băm nữa
        oNhom++;
        oNhomMax = Math.Max(oNhomMax, oNhom);
        // Việc CPU: chạy trên ThreadPool nhưng qua bộ lập lịch giới hạn NhomTinh việc cùng lúc (= số CPU),
        // để vài chục yêu cầu không chiếm hết CPU của luồng vòng lặp (Task.Run thì không giới hạn).
        var viec = Task.Factory.StartNew(() => Bam(data, vong, ct), CancellationToken.None,
                                         TaskCreationOptions.None, NhomTinhLich);
        // khi luồng băm xong THẬT (kể cả sau khi bị hủy) → chạy RaNhom trên vòng lặp
        _ = viec.ContinueWith(t => RaNhom(ma, t.Result), TaskScheduler.FromCurrentSynchronizationContext());
        var (hex, ten, _) = await viec;
        return (hex!, ten);
    }

    void RaNhom(string ma, (string? Hex, string Ten, int DungO) kq)
    {
        oNhom--;
        if (kq.Hex is null) Say($"[server] BAM {ma}: luồng tính thấy cờ hủy → dừng ở vòng {kq.DungO}");
    }

    async Task<string> XuLyBamAsync(string ma, string cheDo, byte[] data, int vong, int han, bool ghi)
    {
        string hex, ten;
        if (cheDo == "chan")                                  // A. SAI: chặn vòng lặp sự kiện
        {
            luong["tra cứu cũ"].Add(TraCuuCu());
            var kq = Bam(data, vong, CancellationToken.None);
            (hex, ten) = (kq.Hex ?? "", kq.Ten);
        }
        else
        {
            if (cheDo == "gioihan") await congVao.WaitAsync();  // C. quá GioiHan yêu cầu thì CHỜ bất đồng bộ
            try
            {
                var cts = new CancellationTokenSource();        // token chia sẻ với luồng băm
                try { (hex, ten) = await TachAsync(ma, data, vong, cts.Token).WaitAsync(TimeSpan.FromMilliseconds(han)); }
                catch (TimeoutException)
                {
                    cts.Cancel();                               // hủy đi xuyên ranh giới: báo luồng băm dừng
                    Log($"BAM {ma}: quá hạn {han} ms → trả HETGIO, bật cờ hủy cho luồng tính");
                    return $"HETGIO {ma}";
                }
            }
            finally { if (cheDo == "gioihan") congVao.Release(); }
        }
        luong["băm SHA-256"].Add(ten);
        if (ghi) Log($"BAM {ma}: băm xong trên {ten}, trả lời OK");
        return $"OK {ma} {hex}";
    }

    // Đọc dòng lệnh và (với BAM) đúng <số_byte> byte dữ liệu theo sau
    static async Task<(string[] P, byte[] Data)> DocYeuCauAsync(NetworkStream ns)
    {
        var buf = new byte[8192];
        var dau = new MemoryStream();
        byte[] du;
        while (true)
        {
            int n = await ns.ReadAsync(buf);
            if (n == 0) return ([], []);
            int j = Array.IndexOf(buf, (byte)'\n', 0, n);
            if (j < 0) { dau.Write(buf, 0, n); continue; }
            dau.Write(buf, 0, j);
            du = buf[(j + 1)..n];
            break;
        }
        var p = Utf8.GetString(dau.ToArray()).Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (p.Length != 6 || p[0] != "BAM") return (p, []);
        var data = new byte[int.Parse(p[3])];
        du.CopyTo(data, 0);
        for (int got = du.Length; got < data.Length;)
        {
            int n = await ns.ReadAsync(data.AsMemory(got));
            if (n == 0) break;
            got += n;
        }
        return (p, data);
    }

    public async Task PhucVuAsync(TcpClient c)
    {
        // Lưu ý .NET: thao tác async có dữ liệu sẵn (accept, đọc) hoàn tất ĐỒNG BỘ – phần sau await chạy luôn,
        // không qua vòng lặp. Task.Yield() đưa phần còn lại thành một việc mới trên vòng lặp, để mỗi kết nối
        // được xử lý như một sự kiện riêng (giống asyncio luôn lên lịch callback qua vòng lặp).
        await Task.Yield();
        using (c)
        {
            luong["nhận/trả lời"].Add(TenLuong());
            var ns = c.GetStream();
            var (p, data) = await DocYeuCauAsync(ns);
            if (p.Length == 0) return;
            string traLoi;
            switch (p[0])
            {
                case "BAM":
                    string ma = p[1], cheDo = p[2];
                    int vong = int.Parse(p[4]), han = int.Parse(p[5]);
                    bool ghi = ma is "1" or "101" or "102";     // chỉ ghi dòng thời gian cho vài yêu cầu
                    if (ghi) Log($"BAM {ma} ({cheDo}): nhận {data.Length} byte, {vong} vòng, hạn {han} ms");
                    soYC++;
                    traLoi = await XuLyBamAsync(ma, cheDo, data, vong, han, ghi);
                    break;
                case "CAMBIEN":
                    var tcs = new TaskCompletionSource<(int, string)>(TaskCreationOptions.RunContinuationsAsynchronously);
                    ycCamBien.Add(tcs);                        // nhờ luồng cảm biến đọc ngay
                    var (giaTri, ten) = await tcs.Task.WaitAsync(TimeSpan.FromSeconds(1));
                    traLoi = $"CAMBIEN {p[1]} {giaTri} {ten}";
                    Log($"CAMBIEN {p[1]}: future được hoàn tất từ luồng {ten}, giá trị {giaTri}");
                    break;
                case "DATLAI":
                    DatLai();
                    traLoi = "OK";
                    break;
                default:                                       // THONGKE <nhãn>
                    string nhan = string.Join(' ', p.Skip(1));
                    Say($"[server] đợt \"{nhan}\": {soYC} yêu cầu");
                    Say($"  {"giai đoạn",-15}luồng đã chạy");
                    foreach (var (gd, ds) in luong) Say($"  {gd,-15}{TomTat(ds)}");
                    Say($"  trễ nhịp tối đa {loop.TreNhipMax:F1} ms; nhóm tính tối đa {oNhomMax} việc");
                    Say($"  cảm biến {soMau} mẫu, trễ giao tối đa {treMau:F1} ms");
                    traLoi = $"TK {loop.TreNhipMax:F1} {soMau} {treMau:F1} {oNhomMax}";
                    break;
            }
            await ns.WriteAsync(Utf8.GetBytes(traLoi + "\n"));
        }
    }

    // Danh sách tên luồng; quá 8 tên thì chỉ in 3 tên đầu và tổng số
    static string TomTat(SortedSet<string> ds) =>
        ds.Count == 0 ? "–" : ds.Count <= 8 ? string.Join(", ", ds) : string.Join(", ", ds.Take(3)) + $", … ({ds.Count} luồng)";

    // Đặc thù C#: .Wait()/.Result trên vòng lặp cho tác vụ cần quay về chính vòng lặp → bế tắc
    static async Task<int> LayAsync(bool configureAwaitFalse)
    {
        if (configureAwaitFalse) await Task.Delay(20).ConfigureAwait(false);  // phần sau chạy trên ThreadPool
        else await Task.Delay(20);                                            // phần sau phải về vòng lặp
        return 1;
    }

    static void DemoBeTacResult()
    {
        Say("Đặc thù C#: gọi .Wait() ngay trên vòng lặp cho một tác vụ cần quay về vòng lặp:");
        bool ok1 = LayAsync(false).Wait(300);  // vòng lặp bị chặn ở đây nên phần tiếp theo không bao giờ chạy
        Say($"  await Task.Delay(20)                        → {(ok1 ? "xong" : "BẾ TẮC: .Wait() không xong, bỏ sau 300 ms")}");
        bool ok2 = LayAsync(true).Wait(300);
        Say($"  await Task.Delay(20).ConfigureAwait(false)  → {(ok2 ? "xong" : "bế tắc")}");
        Say("  → trên vòng lặp/luồng giao diện luôn dùng await, không .Result/.Wait(); mã thư viện nên ConfigureAwait(false).");
    }

    public static void Run(string host, int port)
    {
        Thread.CurrentThread.Name = "vòng-lặp";
        ThreadPool.SetMinThreads(8, 8);    // đủ luồng sẵn cho hàm chặn, tránh "đói" ThreadPool (starvation)
        var loop = new VongLap();
        var mc = new MayChu(loop);
        mc.DatLai();
        loop.Run(async () =>
        {
            var ln = new TcpListener(IPAddress.Parse(host), port);
            ln.Start(128);
            new Thread(mc.CamBien) { Name = "cảm-biến", IsBackground = true }.Start();
            _ = mc.NhanMauAsync();
            Say($"[server] lắng nghe {host}:{port}; vòng lặp sự kiện trên luồng \"vòng-lặp\", nhóm tính {NhomTinh} việc trên ThreadPool,");
            Say($"  nhịp tim {NhipMs} ms, luồng cảm biến mỗi {CamBienMs} ms, chế độ C giới hạn {GioiHan} yêu cầu");
            DemoBeTacResult();
            mc.DatLai();
            while (true)
            {
                var c = await ln.AcceptTcpClientAsync();       // phần sau await quay về vòng lặp
                _ = mc.PhucVuAsync(c);
            }
        });
    }
}
