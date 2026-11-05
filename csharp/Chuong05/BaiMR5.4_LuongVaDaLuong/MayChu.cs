// Bài mở rộng MR5.4 (b) – Server TCP đa luồng: mỗi client một luồng và nhóm luồng cố định.
//   cổng P   – máy chủ A: luồng "nhận-A" AcceptTcpClient(), mỗi kết nối một Thread MỚI "kết-nối-k";
//   cổng P+1 – máy chủ B: 2 luồng cố định "nhóm-1", "nhóm-2" lấy việc từ BlockingCollection có
//              boundedCapacity = 2; luồng "nhận-B" đọc dòng yêu cầu rồi TryAdd() – đầy → "503 BAN <mã>".
// Vai trò tai: 6 luồng "khách-k" (cách nhau 20 ms) gửi "VIEC k 300" tới từng máy chủ.
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using static BaiMR5_4.NhatKy;

namespace BaiMR5_4;

static class MayChu
{
    const int NKhach = 6, MsViec = 300, CachMs = 20, Nhom = 2, HangDoi = 2;

    record Viec(TcpClient C, StreamWriter W, string[] P);

    // Phần việc giống nhau ở hai máy chủ: chờ <ms> rồi trả lời kèm tên luồng đang chạy
    static void XuLy(StreamWriter w, string[] p)
    {
        Log($"xử lý VIEC {p[1]} {p[2]}");
        Thread.Sleep(int.Parse(p[2]));
        w.WriteLine($"XONG {p[1]} {Thread.CurrentThread.Name}");
    }

    static string[]? DocYeuCau(StreamReader r)
    {
        var p = (r.ReadLine() ?? "").Split(' ', StringSplitOptions.RemoveEmptyEntries);
        return p.Length == 3 && p[0] == "VIEC" ? p : null;
    }

    static void MayChuA(TcpListener ln)
    {
        for (int dem = 1; ; dem++)
        {
            var c = ln.AcceptTcpClient();
            Log($"kết nối mới → tạo luồng kết-nối-{dem}");
            new Thread(() =>
            {
                using (c)
                {
                    var (r, w) = DichVu.Luong(c);
                    if (DocYeuCau(r) is { } p) XuLy(w, p);
                }
            }) { Name = $"kết-nối-{dem}", IsBackground = true }.Start();
        }
    }

    static void MayChuB(TcpListener ln)
    {
        var viec = new BlockingCollection<Viec>(boundedCapacity: HangDoi);   // hàng đợi có giới hạn
        for (int i = 1; i <= Nhom; i++)
            new Thread(() =>
            {
                // luồng thợ của nhóm: lặp mãi, mỗi lần lấy một việc (chặn khi hàng đợi rỗng)
                foreach (var v in viec.GetConsumingEnumerable())
                    using (v.C) XuLy(v.W, v.P);
            }) { Name = $"nhóm-{i}", IsBackground = true }.Start();
        while (true)
        {
            var c = ln.AcceptTcpClient();
            var (r, w) = DichVu.Luong(c);
            if (DocYeuCau(r) is not { } p) { c.Dispose(); continue; }
            if (viec.TryAdd(new Viec(c, w, p)))           // không chờ: đầy thì từ chối ngay
                Log($"VIEC {p[1]} vào hàng đợi");
            else
            {
                Log($"hàng đợi đầy ({HangDoi}/{HangDoi}) → trả 503 cho VIEC {p[1]}");
                w.WriteLine($"503 BAN {p[1]}");
                c.Dispose();
            }
        }
    }

    public static void RunServer(string host, int pa)
    {
        var lnA = new TcpListener(IPAddress.Parse(host), pa);
        var lnB = new TcpListener(IPAddress.Parse(host), pa + 1);
        lnA.Start(64);
        lnB.Start(64);
        Say($"[server] cổng {pa}: máy chủ A – mỗi client một luồng");
        Say($"[server] cổng {pa + 1}: máy chủ B – nhóm {Nhom} luồng cố định, hàng đợi tối đa {HangDoi} (đầy → 503)");
        var ta = new Thread(() => MayChuA(lnA)) { Name = "nhận-A", IsBackground = true };
        var tb = new Thread(() => MayChuB(lnB)) { Name = "nhận-B" };
        ta.Start();
        tb.Start();
        tb.Join();
    }

    // ---------------- Máy tạo tải ----------------
    static void Khach(string host, int port, int k, string[] kq)
    {
        Thread.Sleep((k - 1) * CachMs);           // khách k xuất phát muộn hơn khách 1 là (k-1)×20 ms
        var sw = Stopwatch.StartNew();
        using var c = new TcpClient(host, port);
        var (r, w) = DichVu.Luong(c);
        w.WriteLine($"VIEC {k} {MsViec}");
        string line = r.ReadLine() ?? "";
        double dt = sw.Elapsed.TotalSeconds;
        kq[k - 1] = line;
        var p = line.Split(' ');
        if (p[0] == "XONG") Log($"XONG {k} bởi {p[2]} sau {dt:F2} s");
        else Log($"{line} (bị từ chối) sau {dt:F2} s");
    }

    static (int, int, double) MotDot(string host, int port)
    {
        var kq = new string[NKhach];
        var sw = Stopwatch.StartNew();
        var ths = Enumerable.Range(1, NKhach)
            .Select(k => new Thread(() => Khach(host, port, k, kq)) { Name = $"khách-{k}" }).ToList();
        ths.ForEach(t => t.Start());
        ths.ForEach(t => t.Join());
        int ok = kq.Count(x => x.StartsWith("XONG"));
        return (ok, NKhach - ok, sw.Elapsed.TotalSeconds);
    }

    public static void RunTai(string host, int pa)
    {
        Say($"(b) Máy chủ đa luồng: {NKhach} client đồng thời (cách nhau {CachMs} ms), mỗi yêu cầu xử lý {MsViec} ms");
        Say($"Máy chủ A – mỗi client một luồng (cổng {pa}):");
        var a = MotDot(host, pa);
        Say($"Máy chủ B – nhóm {Nhom} luồng, hàng đợi {HangDoi} (cổng {pa + 1}):");
        var b = MotDot(host, pa + 1);
        Say();
        Say($"  {"Máy chủ",-29}{"Thành công",11}{"Từ chối",9}{"Thời gian",10}");
        foreach (var (ten, (ok, tc, dt)) in new[] { ("A. mỗi client một luồng", a),
                                                    ($"B. nhóm {Nhom} luồng + hàng đợi {HangDoi}", b) })
            Say($"  {ten,-29}{ok,11}{tc,9}{dt,8:F2} s");
        Say("Nhận xét: A tạo luồng không giới hạn – nhanh khi ít client nhưng quá tải sẽ cạn bộ nhớ;");
        Say($"  B giữ số luồng cố định, chỉ nhận {Nhom + HangDoi} việc cùng lúc, phần vượt bị từ chối ngay (503)");
        Say("  thay vì xếp hàng vô hạn – client biết để thử lại sau.");
    }
}
