// Bài mở rộng MR5.4 – Luồng (thread) và đa luồng trong chương trình mạng (C#).
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6) và Chương 6 (Bài 6.1, 6.2).
// Khác MR5.2 (so sánh mô hình trên một khối việc), bài này đi từng bước vào chính luồng:
//   (a) Thread: tạo – Start – Join, truyền tham số, lấy kết quả        → luong
//   (b) server: mỗi client một Thread / nhóm luồng + BlockingCollection giới hạn (503) → server, tai
//   (c) trạng thái dùng chung: tranh chấp, lock, Interlocked, ConcurrentDictionary → khoa
//   (d) nhà sản xuất – người tiêu dùng, BlockingCollection, poison pill   → hang-doi
//   (e) SemaphoreSlim giới hạn số kết nối ra ngoài                       → luong
//   (f) bế tắc: Monitor.TryEnter có hạn, thứ tự lấy khóa                 → khoa
//   (g) nhật ký theo luồng: Thread.Name + ThreadLocal<string> (NhatKy.cs)
// Chạy:  dotnet run -- dich-vu|server|luong|hang-doi|tai|khoa <host> <port>
using System.Diagnostics;
using System.Globalization;
using System.Net.Sockets;
using BaiMR5_4;
using static BaiMR5_4.NhatKy;

Console.OutputEncoding = System.Text.Encoding.UTF8;
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;   // số thập phân dùng dấu chấm
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
Thread.CurrentThread.Name = "chính";

string[] vaiTro = { "dich-vu", "server", "luong", "hang-doi", "tai", "khoa" };
if (args.Length < 3 || !vaiTro.Contains(args[0]))
{
    Console.WriteLine("Cách dùng: dotnet run -- dich-vu|server|luong|hang-doi|tai|khoa <host> <port>");
    return 1;
}
string host = args[1];
int port = int.Parse(args[2]);
switch (args[0])
{
    case "dich-vu": DichVu.Run(host, port); break;
    case "server": MayChu.RunServer(host, port); break;
    case "luong": Luong.Run(host, port); break;
    case "hang-doi": HangDoi.Run(host, port); break;
    case "tai": MayChu.RunTai(host, port); break;
    case "khoa": Khoa.Run(host, port); break;
}
return 0;

static class Luong
{
    const int SoSuat = 3, NGoiRa = 8, MsGoiRa = 150;

    // Một lời gọi mạng kiểu chặn: kết nối, gửi một dòng, đọc một dòng trả lời
    static string GoiDichVu(string host, int port, string line)
    {
        using var c = new TcpClient(host, port);
        var (r, w) = DichVu.Luong(c);
        w.WriteLine(line);
        return r.ReadLine() ?? "";
    }

    // (a) tạo, chạy, chờ luồng; tham số và kết quả
    static void CoBan(string host, int port)
    {
        Say("(a) Tạo – chạy – chờ luồng: 3 luồng, mỗi luồng nhận tham số (mã, thời gian xử lý) và trả kết quả");
        (int ma, int ms)[] thamSo = { (1, 300), (2, 100), (3, 200) };
        var ketQua = new (string traLoi, double ms)[thamSo.Length];   // mỗi luồng ghi ô RIÊNG → không cần khóa
        Log("tạo 3 luồng tải-1..tải-3 rồi cho chạy (start)");
        var sw = Stopwatch.StartNew();
        var ths = new List<Thread>();
        for (int i = 0; i < thamSo.Length; i++)
        {
            var (ma, ms) = thamSo[i];
            int o = i;                                // biểu thức lambda "bắt" biến → truyền tham số
            ths.Add(new Thread(() =>
            {
                DatNguCanh($"yc={ma}");
                Log($"kết nối dịch vụ, gửi VIEC {ma} {ms}");
                var t = Stopwatch.StartNew();
                string tl = GoiDichVu(host, port, $"VIEC {ma} {ms}");
                double dt = t.Elapsed.TotalMilliseconds;
                Log($"nhận {tl} sau {dt:F0} ms");
                ketQua[o] = (tl, dt);                 // Thread không trả giá trị → ghi vào ô dùng riêng
            }) { Name = $"tải-{ma}" });
        }
        ths.ForEach(t => t.Start());                  // luồng bắt đầu chạy song song với luồng chính
        ths.ForEach(t => t.Join());                   // luồng chính chờ từng luồng kết thúc
        double tong = sw.Elapsed.TotalSeconds;
        Log($"chờ (join) xong cả 3 luồng sau {tong:F2} s (tuần tự sẽ ≈ {thamSo.Sum(x => x.ms) / 1000.0:F2} s)");
        Say();
        Say($"  {"Luồng",-8}{"Tham số",-16}{"Kết quả",-10}{"Thời gian",10}");
        for (int i = 0; i < thamSo.Length; i++)
            Say($"  {"tải-" + thamSo[i].ma,-8}{$"mã {thamSo[i].ma}, {thamSo[i].ms} ms",-16}{ketQua[i].traLoi,-10}{ketQua[i].ms,7:F0} ms");
        GoiDichVu(host, port, "THONGKE");             // đặt lại thống kê của dịch vụ trước phần (e)
    }

    // (e) SemaphoreSlim giới hạn kết nối ra ngoài
    static void GioiHan(string host, int port)
    {
        Say();
        Say($"(e) Giới hạn kết nối ra ngoài: {NGoiRa} luồng gọi dịch vụ, tối đa {SoSuat} kết nối cùng lúc (semaphore)");
        var suat = new SemaphoreSlim(SoSuat, SoSuat);   // SoSuat "suất"; hết suất thì Wait() chặn
        int dangDung = 0;
        var sw = Stopwatch.StartNew();
        var ths = Enumerable.Range(1, NGoiRa).Select(k => new Thread(() =>
        {
            DatNguCanh($"yc={k}");
            if (!suat.Wait(0))                        // thử lấy suất không chờ để biết có phải chờ không
            {
                Log("hết suất → chờ");
                suat.Wait();
            }
            int n = Interlocked.Increment(ref dangDung);
            Log($"được suất (đang dùng {n}/{SoSuat}), gửi VIEC {10 + k} {MsGoiRa}");
            try { GoiDichVu(host, port, $"VIEC {10 + k} {MsGoiRa}"); }
            finally
            {
                Interlocked.Decrement(ref dangDung);
                suat.Release();                       // luôn trả suất, kể cả khi lời gọi lỗi
            }
            Log("xong, trả suất");
        }) { Name = $"gọi-{k}" }).ToList();
        ths.ForEach(t => t.Start());
        ths.ForEach(t => t.Join());
        double dt = sw.Elapsed.TotalSeconds;
        var p = GoiDichVu(host, port, "THONGKE").Split(' ');
        int dot = (NGoiRa + SoSuat - 1) / SoSuat;
        Log($"dịch vụ xác nhận: {p[1]} yêu cầu, đồng thời tối đa {p[2]}; tổng {dt:F2} s (≈ ⌈{NGoiRa}/{SoSuat}⌉ × {MsGoiRa} ms = {dot * MsGoiRa / 1000.0:F2} s)");
    }

    public static void Run(string host, int port)
    {
        CoBan(host, port);
        GioiHan(host, port);
        Say();
        Say("(g) Nhật ký theo luồng: mỗi dòng ghi tên luồng; ngữ cảnh \"yc=\" lấy từ biến cục bộ luồng,");
        Say("    nên hàm ghi nhật ký không cần nhận mã yêu cầu qua tham số.");
        Say();
        ThreadPool.GetMinThreads(out int minTho, out _);
        Say($"Đặc thù C#: new Thread tạo một luồng HĐH riêng; ThreadPool (Task.Run) dùng lại luồng – tối thiểu {minTho}");
        Say("  luồng thợ, thiếu thì tự thêm dần; lock là cú pháp gọn của Monitor.Enter/Exit; Interlocked cho thao");
        Say("  tác nguyên tử; ThreadLocal<T> là biến cục bộ luồng (mã async/await dùng AsyncLocal<T>).");
    }
}
