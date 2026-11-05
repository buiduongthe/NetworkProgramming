// Bài mở rộng MR5.4 (c), (f) – Trạng thái dùng chung giữa các luồng xử lý kết nối; bế tắc.
// (c) Server đếm chạy ngay trong chương trình: 4 luồng "gửi-k" mở 4 kết nối TCP, mỗi kết nối gửi 2000 dòng
//     "DEM GET"/"DEM PUT"; mỗi kết nối một luồng "xử-lý-k" cập nhật bốn bộ đếm dùng chung:
//     1. không khóa: đọc → Thread.Yield() (nhường CPU, giống ghi nhật ký giữa chừng) → ghi → mất cập nhật;
//     2. lock (Monitor) quanh đúng đoạn đọc – nhường – ghi; 3. Interlocked.Increment (nguyên tử);
//     4. ConcurrentDictionary<string,long>.AddOrUpdate theo loại thông điệp (bảng đồng thời).
//     Lưu ý: Dictionary thường bị nhiều luồng cùng ghi có thể hỏng cấu trúc, ném ngoại lệ – không thử ở đây.
// (f) "chuyển-AB" khóa phiên A rồi B, "chuyển-BA" khóa B rồi A; Monitor.TryEnter(khóa, 500) chờ có hạn để
//     PHÁT HIỆN bế tắc; sửa bằng thứ tự lấy khóa cố định (A trước B).
using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Text;
using static BaiMR5_4.NhatKy;

namespace BaiMR5_4;

static class Khoa
{
    const int NKetNoi = 4, NTin = 2000, GiuMs = 50, HanChoMs = 500;

    static long khongKhoa, coKhoa, nguyenTu;
    static readonly object KhoaDem = new();
    static readonly ConcurrentDictionary<string, long> Bang = new();

    static void CapNhat(string loai)
    {
        long v = khongKhoa;          // 1. đọc
        Thread.Yield();              //    nhường CPU: luồng khác có thể chen vào đây
        khongKhoa = v + 1;           //    ghi – có thể đè lên kết quả của luồng khác
        lock (KhoaDem)               // 2. đoạn găng có khóa
        {
            v = coKhoa;
            Thread.Yield();
            coKhoa = v + 1;
        }
        Interlocked.Increment(ref nguyenTu);           // 3. đọc – cộng – ghi trong một lệnh CPU
        Bang.AddOrUpdate(loai, 1, (_, cu) => cu + 1);  // 4. bảng đồng thời
    }

    static void TranhChap(string host, int port)
    {
        Say($"(c) Trạng thái dùng chung: {NKetNoi} kết nối × {NTin} thông điệp DEM, mỗi kết nối một luồng xử lý");
        var ln = new TcpListener(IPAddress.Parse(host), port);
        ln.Start(16);
        Log($"server đếm lắng nghe {host}:{port}; {NKetNoi} luồng gửi bắt đầu");
        var sb = new StringBuilder();
        for (int i = 0; i < NTin; i++) sb.Append(i % 2 == 0 ? "DEM GET\n" : "DEM PUT\n");
        byte[] data = DichVu.Utf8.GetBytes(sb.ToString());
        var ths = new List<Thread>();
        for (int k = 1; k <= NKetNoi; k++)
            ths.Add(new Thread(() => { using var c = new TcpClient(host, port); c.GetStream().Write(data); })
                    { Name = $"gửi-{k}" });
        ths.ForEach(t => t.Start());
        for (int k = 1; k <= NKetNoi; k++)
        {
            var c = ln.AcceptTcpClient();
            var t = new Thread(() =>
            {
                int soTin = 0;
                using (c)
                {
                    var (r, _) = DichVu.Luong(c);
                    while (r.ReadLine() is { } line)
                    {
                        var p = line.Split(' ');
                        if (p.Length == 2 && p[0] == "DEM") { CapNhat(p[1]); soTin++; }
                    }
                }
                Log($"kết nối đóng: đã xử lý {soTin} thông điệp");
            }) { Name = $"xử-lý-{k}" };
            t.Start();
            ths.Add(t);
        }
        ths.ForEach(t => t.Join());
        ln.Stop();
        long dung = NKetNoi * NTin;
        long get = Bang.GetValueOrDefault("GET"), put = Bang.GetValueOrDefault("PUT");
        Say();
        Say($"  {"Cách cập nhật biến chung",-32}{"Giá trị",9}{"Đúng",6}{"Mất cập nhật",13}");
        foreach (var (ten, gt, so) in new[] {
                     ("không khóa (đọc – nhường – ghi)", khongKhoa.ToString(), khongKhoa),
                     ("khóa (mutex/lock)", coKhoa.ToString(), coKhoa),
                     ("nguyên tử (atomic)", nguyenTu.ToString(), nguyenTu),
                     ("bảng đồng thời: GET + PUT", $"{get}+{put}", get + put) })
            Say($"  {ten,-32}{gt,9}{dung,6}{dung - so,13}");
        Say("Nhận xét: \"đọc – làm việc khác – ghi\" không khóa làm mất cập nhật; khóa, thao tác nguyên tử");
        Say("  hoặc cấu trúc dữ liệu đồng thời đều cho kết quả đúng.");
    }

    static void BeTac()
    {
        Say();
        Say("(f) Bế tắc: hai luồng chuyển tin giữa phiên A và phiên B, mỗi luồng cần khóa của cả hai phiên");
        var khoa = new Dictionary<string, object> { ["A"] = new(), ["B"] = new() };
        var bang = new List<(string, int, int)>();
        foreach (var (lan, coThuTu) in new[] { (1, false), (2, true) })
        {
            Say(coThuTu ? $"Lần {lan} – mọi luồng lấy khóa theo cùng thứ tự (A trước B):"
                        : $"Lần {lan} – lấy khóa theo thứ tự ngược nhau:");
            int xong = 0, bo = 0;
            void Chuyen(string nguon, string dich)
            {
                var (dau, sau) = coThuTu && string.CompareOrdinal(nguon, dich) > 0 ? (dich, nguon) : (nguon, dich);
                lock (khoa[dau])
                {
                    Log($"giữ khóa phiên {dau}, chờ khóa phiên {sau}");
                    Thread.Sleep(GiuMs);                         // giả lập ghi vào socket của phiên đang giữ
                    if (!Monitor.TryEnter(khoa[sau], HanChoMs))  // chờ có hạn
                    {
                        Log($"chờ khóa phiên {sau} quá {HanChoMs} ms → nghi bế tắc, nhả khóa phiên {dau}, bỏ cuộc");
                        Interlocked.Increment(ref bo);
                        return;
                    }
                    try
                    {
                        Log($"đã chuyển tin từ phiên {nguon} sang phiên {dich}");
                        Interlocked.Increment(ref xong);
                    }
                    finally { Monitor.Exit(khoa[sau]); }
                }
            }
            var t1 = new Thread(() => Chuyen("A", "B")) { Name = "chuyển-AB" };
            var t2 = new Thread(() => Chuyen("B", "A")) { Name = "chuyển-BA" };
            t1.Start();
            Thread.Sleep(10);
            t2.Start();
            t1.Join();
            t2.Join();
            bang.Add((coThuTu ? "cùng thứ tự" : "ngược thứ tự", xong, bo));
        }
        Say();
        Say($"  {"Cách lấy khóa",-20}{"Hoàn tất",10}{"Bỏ cuộc (hết hạn chờ)",24}");
        foreach (var (ten, xong, bo) in bang) Say($"  {ten,-20}{xong,10}{bo,24}");
        Say("Nhận xét: cùng một thứ tự lấy khóa cho mọi luồng loại bỏ vòng chờ – điều kiện cần của bế tắc.");
    }

    public static void Run(string host, int port)
    {
        TranhChap(host, port);
        BeTac();
    }
}
