// Bài mở rộng MR5.5 – Phối hợp đa luồng và bất đồng bộ trong server mạng (C#).
// Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6), Chương 6 (Bài 6.1, 6.2), MR5.4.
// Server async (vòng lặp sự kiện một luồng tự dựng, VongLap.cs) phải:
//   (a) đẩy việc CPU (băm SHA-256) sang ThreadPool bằng Task.Run mà không chặn vòng lặp;
//   (b) gọi hàm thư viện cũ kiểu CHẶN an toàn (Task.Run, không gọi thẳng trên vòng lặp);
//   (c) nhận kết quả từ luồng nền (cảm biến) qua Channel<T> và TaskCompletionSource;
//   (d) truyền hủy, hạn chót qua ranh giới (WaitAsync + CancellationToken);
//   (e) áp lực ngược giữa phần async và ThreadPool (SemaphoreSlim.WaitAsync);
//   (f) cách làm SAI – việc CPU ngay trên vòng lặp – đo bằng nhịp tim 10 ms; thêm: bế tắc .Wait()/.Result.
// Chạy:  dotnet run -- server <host> <port>
//        dotnet run -- tai    <host> <port>
using System.Diagnostics;
using System.Globalization;
using BaiMR5_5;
using static BaiMR5_5.Chung;

Console.OutputEncoding = System.Text.Encoding.UTF8;
CultureInfo.DefaultThreadCurrentCulture = CultureInfo.InvariantCulture;
CultureInfo.CurrentCulture = CultureInfo.InvariantCulture;
if (args.Length < 3 || (args[0] != "server" && args[0] != "tai"))
{
    Console.WriteLine("Cách dùng: dotnet run -- server|tai <host> <port>");
    return 1;
}
if (args[0] == "server") MayChu.Run(args[1], int.Parse(args[2]));
else await Tai.RunAsync(args[1], int.Parse(args[2]));
return 0;

static class Tai
{
    // Máy tạo tải: 16 yêu cầu đồng thời (Task.WhenAll) cho từng chế độ, in bảng so sánh
    public static async Task RunAsync(string host, int port)
    {
        byte[] data = DuLieu(SoByte);
        Say($"Máy tạo tải: {NYC} yêu cầu đồng thời mỗi đợt, mỗi yêu cầu {SoByte / 1024} KB, băm SHA-256 {SoVong} vòng (≈ {SoByte * (double)SoVong / 1e6:F0} MB dữ liệu băm)");
        var cheDo = new[] { ("chan", "A. CPU ngay trên vòng lặp"), ("tach", "B. đẩy sang nhóm luồng"), ("gioihan", "C. nhóm luồng + áp lực ngược") };
        var bang = new List<(string Nhan, double Dt, string[] Tk)>();
        string? mauHex = null;
        foreach (var (cd, nhan) in cheDo)
        {
            await GuiAsync(host, port, "DATLAI");
            var sw = Stopwatch.StartNew();
            var kq = await Task.WhenAll(Enumerable.Range(1, NYC)
                .Select(k => GuiAsync(host, port, $"BAM {k} {cd} {SoByte} {SoVong} 10000", data)));
            double dt = sw.Elapsed.TotalSeconds;
            var ok = kq.Where(x => x.StartsWith("OK")).Select(x => x.Split(' ')[2]).ToList();
            mauHex ??= ok.FirstOrDefault();
            var tk = (await GuiAsync(host, port, $"THONGKE {nhan}")).Split(' ');
            Say($"Đợt {nhan}: {ok.Count}/{NYC} OK, {dt:F2} s, {NYC / dt:F1} yc/s, {ok.Distinct().Count()} giá trị băm khác nhau");
            bang.Add((nhan, dt, tk));
        }
        Say();
        Say($"  {"Chế độ",-18}{"Thời gian",10}{"yc/s",7}{"Trễ nhịp",10}{"Cảm biến",10}{"Nhóm",6}");
        string[] ngan = { "A. chặn vòng lặp", "B. nhóm luồng", "C. + áp lực ngược" };   // nhãn ngắn trong bảng
        for (int i = 0; i < bang.Count; i++)
        {
            var (_, dt, tk) = bang[i];
            Say($"  {ngan[i],-18}{dt,8:F2} s{NYC / dt,7:F1}{double.Parse(tk[1]),7:F1} ms{double.Parse(tk[3]),7:F1} ms{int.Parse(tk[4]),6}");
        }
        Say("  (Trễ nhịp, Cảm biến: độ trễ tối đa của nhịp tim 10 ms và của");
        Say("   mẫu cảm biến; Nhóm: số việc ở nhóm tính cùng lúc, tối đa)");
        Say($"Băm kiểm tra (16 hex đầu): {mauHex} – giống nhau ở mọi bản ngôn ngữ");
        Say("Nhận xét: A chặn vòng lặp → nhịp tim, mẫu cảm biến và mọi kết nối khác đều đứng chờ; B, C giữ");
        Say($"  vòng lặp thông suốt; C chỉ cho {GioiHan} việc vào nhóm {NhomTinh} luồng, phần còn lại chờ");
        Say("  bất đồng bộ ở cổng vào – hàng đợi của nhóm luồng không phình theo tải.");
        Say();
        Say("(d) Hạn chót và hủy xuyên ranh giới async → luồng (chế độ tach):");
        foreach (var (ma, vong, han) in new[] { (101, SoVong, 1000), (102, SoVong * 8, 30) })
        {
            var sw = Stopwatch.StartNew();
            string tl = await GuiAsync(host, port, $"BAM {ma} tach {SoByte} {vong} {han}", data);
            Say($"  yêu cầu {ma} ({vong} vòng, hạn {han} ms): {tl.Split(' ')[0]} sau {sw.Elapsed.TotalMilliseconds:F0} ms");
        }
        Say();
        Say("(c) Luồng nền giao kết quả vào vòng lặp sự kiện:");
        foreach (int ma in new[] { 201, 202 })
        {
            var tl = (await GuiAsync(host, port, $"CAMBIEN {ma}")).Split(' ');
            Say($"  CAMBIEN {ma} → giá trị {tl[2]}, future do luồng {tl[3]} hoàn tất");
        }
    }
}
