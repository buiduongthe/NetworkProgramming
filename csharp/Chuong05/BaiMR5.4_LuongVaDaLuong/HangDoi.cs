// Bài mở rộng MR5.4 (d) – Nhà sản xuất – người tiêu dùng trên một kết nối TCP.
//   luồng "nhận"    : gửi "PHAT 9", đọc từng dòng "GOI k dữ-liệu", Add() vào BlockingCollection có
//                     boundedCapacity = 4 – đầy thì Add() CHẶN: áp lực ngược, không đọc thêm từ socket;
//   3 luồng "xử-lý-k": Take() một gói, xử lý 80 ms + tính tổng byte, Add() kết quả vào hàng đợi kết quả;
//   luồng "gửi"     : ghi "KQ k tổng" vào CÙNG NetworkStream (một luồng đọc, một luồng ghi: được phép).
// Kết thúc êm bằng "viên thuốc độc": null. (Cách khác: CancellationToken truyền vào Take(token) khi cần
// dừng NGAY; hoặc CompleteAdding() – mọi luồng đang GetConsumingEnumerable() thoát khi hàng đợi rỗng.)
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Net.Sockets;
using static BaiMR5_4.NhatKy;

namespace BaiMR5_4;

static class HangDoi
{
    const int NGoi = 9, NTho = 3, SucChua = 4, MsXuLy = 80;

    record Goi(int K, string Text);
    record KetQua(int K, int V);

    public static void Run(string host, int port)
    {
        Say($"(d) Nhà sản xuất – người tiêu dùng: luồng nhận đọc socket → hàng đợi (tối đa {SucChua}) → {NTho} luồng xử lý → luồng gửi");
        var viec = new BlockingCollection<Goi?>(boundedCapacity: SucChua);
        var ketQua = new BlockingCollection<KetQua?>();
        var dem = new int[NTho + 1];
        int soLanCho = 0;
        using var c = new TcpClient(host, port);
        var (r, w) = DichVu.Luong(c);

        var ths = new List<Thread>
        {
            new(() =>
            {
                Log($"kết nối dịch vụ, gửi PHAT {NGoi}");
                w.WriteLine($"PHAT {NGoi}");
                for (int i = 0; i < NGoi; i++)
                {
                    var p = r.ReadLine()!.Split(' ');
                    var g = new Goi(int.Parse(p[1]), p[2]);
                    if (!viec.TryAdd(g))
                    {
                        soLanCho++;
                        Log($"hàng đợi đầy ({SucChua}/{SucChua}) → chờ chỗ trống (áp lực ngược)");
                        viec.Add(g);                       // chặn tới khi một luồng xử lý lấy bớt
                    }
                    Log($"GOI {g.K} → hàng đợi");
                }
                Log($"hết dữ liệu → đặt {NTho} viên thuốc độc (poison pill) vào hàng đợi");
                for (int i = 0; i < NTho; i++) viec.Add(null);
            }) { Name = "nhận" }
        };
        for (int i = 1; i <= NTho; i++)
        {
            int so = i;
            ths.Add(new Thread(() =>
            {
                while (true)
                {
                    var g = viec.Take();
                    if (g is null)
                    {
                        DatNguCanh("");
                        Log("gặp viên thuốc độc → dừng");
                        ketQua.Add(null);
                        return;
                    }
                    DatNguCanh($"yc={g.K}");                 // ngữ cảnh cục bộ luồng: Log() tự ghi "yc=k"
                    Thread.Sleep(MsXuLy);                    // giả lập xử lý tốn thời gian
                    int v = DichVu.TongByte(g.Text);
                    Log($"xử lý GOI {g.K}: tổng byte = {v}");
                    dem[so]++;                               // mỗi luồng chỉ ghi ô của mình
                    ketQua.Add(new KetQua(g.K, v));
                }
            }) { Name = $"xử-lý-{i}" });
        }
        ths.Add(new Thread(() =>
        {
            for (int conLai = NTho; conLai > 0;)
            {
                var kq = ketQua.Take();
                if (kq is null) { conLai--; continue; }
                w.WriteLine($"KQ {kq.K} {kq.V}");
                Log($"gửi KQ {kq.K} {kq.V}");
            }
            Log($"cả {NTho} luồng xử lý đã dừng → hết việc gửi");
        }) { Name = "gửi" });

        var sw = Stopwatch.StartNew();
        ths.ForEach(t => t.Start());
        ths.ForEach(t => t.Join());
        string traLoi = r.ReadLine() ?? "";
        double dt = sw.Elapsed.TotalSeconds;
        Log($"dịch vụ trả lời: {traLoi}");
        Say();
        Say($"  {"Luồng",-12}{"Số gói đã xử lý",16}");
        for (int i = 1; i <= NTho; i++) Say($"  {"xử-lý-" + i,-12}{dem[i],16}");
        Say($"  Luồng nhận phải chờ vì hàng đợi đầy: {soLanCho} lần; tổng {dt:F2} s (1 luồng xử lý ≈ {NGoi * MsXuLy / 1000.0:F2} s)");
    }
}
