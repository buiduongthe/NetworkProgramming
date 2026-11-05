using System.Diagnostics;
using System.Globalization;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Client phòng hội thoại âm thanh.
// Mỗi 20 ms client làm hai việc trong cùng một vòng lặp có nhịp cố định:
//   1. Thu: lấy 320 mẫu từ nguồn âm (sóng sin tổng hợp – mỗi client một tần số khác nhau để phân tích
//      được ai đang nói; hoặc tệp WAV), đóng gói RTP, gửi UDP tới server nếu mic đang mở.
//   2. Phát: lấy một khung từ bộ đệm jitter (luồng trộn server gửi về), nhân âm lượng nghe của chính
//      client, ghi vào bản ghi (--ghi tệp.wav).
// Mỗi giây in thanh đo mức (dBFS). Sự kiện từ server được đánh mốc theo vị trí trong bản ghi
// (tệp <tên>_moc.json) để vai trò phantich chia đoạn và chứng minh lệnh điều khiển có hiệu lực.
// Lệnh (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi client chạy):
//   tat_mic   bat_mic   am_luong <hệ số>   thoat
interface INguonAm { short[] LayKhung(); }

/// <summary>Âm tổng hợp: sin tần số f, biên độ tương đối (0..1). Pha liên tục giữa các khung.</summary>
class NguonSin(double f, double bienDo) : INguonAm
{
    readonly double a = bienDo * 32767;
    long n;

    public short[] LayKhung()
    {
        double w = 2 * Math.PI * f / TanSoMau;
        var k = new short[MauMoiKhung];
        for (int i = 0; i < MauMoiKhung; i++) k[i] = (short)Math.Round(a * Math.Sin(w * (n + i)));
        n += MauMoiKhung;
        return k;
    }
}

/// <summary>Phát lặp một tệp WAV 16 bit mono 16 kHz.</summary>
class NguonWav(string path) : INguonAm
{
    readonly short[] mau = DocWav(path);
    long vt;

    public short[] LayKhung()
    {
        var k = new short[MauMoiKhung];
        for (int i = 0; i < MauMoiKhung; i++) k[i] = mau[(vt + i) % mau.Length];
        vt += MauMoiKhung;
        return k;
    }
}

static class KhachAmThanh
{
    public static int Run(string[] args)
    {
        // <tên> <host> <cổng_tcp> [--tan-so 440] [--bien-do 0.25] [--wav tệp.wav] [--ghi out/ghi.wav]
        // [--thoi-gian 10] [--mat-goi 0.05] [--dem 3]
        var pos = new List<string>();
        var o = new Dictionary<string, string>();
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i].StartsWith("--") && i + 1 < args.Length) o[args[i]] = args[++i];
            else pos.Add(args[i]);
        }
        double D(string k, double d) => o.TryGetValue(k, out var v) ? double.Parse(v, CultureInfo.InvariantCulture) : d;
        string ten = pos[0], host = pos[1];
        int cong = int.Parse(pos[2]);
        double tanSo = D("--tan-so", 440), bienDo = D("--bien-do", 0.25), thoiGian = D("--thoi-gian", 10), matGoi = D("--mat-goi", 0);
        int dem = (int)D("--dem", 3);
        string? ghi = o.GetValueOrDefault("--ghi"), wav = o.GetValueOrDefault("--wav");
        string tag = $"[khach {ten}]";
        void Log(string text) => Console.WriteLine($"{tag} {text}");

        // ---- Nguồn âm
        INguonAm nguon = new NguonSin(tanSo, bienDo);
        string moTa = $"sin {tanSo.ToString(CultureInfo.InvariantCulture)} Hz, biên độ {bienDo.ToString(CultureInfo.InvariantCulture)}";
        if (wav != null)
        {
            nguon = new NguonWav(wav);
            moTa = $"tệp {wav}";
        }

        // ---- Kênh điều khiển TCP: vào phòng
        var tcp = new Socket(SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        tcp.Connect(host, cong);
        var f = new StreamReader(new NetworkStream(tcp), Encoding.UTF8);
        GuiJson(tcp, new JsonObject { ["loai"] = "vao", ["ten"] = ten });
        var chao = JsonNode.Parse(f.ReadLine()!)!.AsObject();
        uint ma = chao["ma"]!.GetValue<uint>();
        Log($"Vào phòng, mã {ma}; nguồn: {moTa}");
        Log($"Gửi {MauMoiKhung} mẫu/{KhungMs} ms -> 50 gói/s, 256 kbit/s PCM");

        // ---- Kênh dữ liệu UDP: Connect() để chỉ nhận gói từ đúng địa chỉ server
        var udp = new Socket(SocketType.Dgram, ProtocolType.Udp);
        udp.Connect(host, chao["cong_udp"]!.GetValue<int>());

        var khoa = new object();
        bool micMo = false, dung = false;
        double amLuong = 1.0;
        var boDem = new BoDemJitter(dem);
        var banGhi = new List<short>();
        var moc = new JsonArray();                               // (giây trong bản ghi, sự kiện)
        var sw = Stopwatch.StartNew();
        double Now() => sw.Elapsed.TotalSeconds;

        void DanhMoc(string text)
        {
            lock (khoa) moc.Add(new JsonObject { ["t"] = Math.Round(banGhi.Count / (double)TanSoMau, 3), ["su_kien"] = text });
        }

        void DocTcp()
        {
            try
            {
                string? dong;
                while ((dong = f.ReadLine()) != null)
                {
                    var tin = JsonNode.Parse(dong)!.AsObject();
                    switch (tin["loai"]?.GetValue<string>())
                    {
                        case "mic":
                            lock (khoa) micMo = tin["mo"]!.GetValue<bool>();
                            Log($"Mic: {tin["ly_do"]}");
                            break;
                        case "su_kien":
                            Log($"Sự kiện: {tin["noi_dung"]}");
                            DanhMoc(tin["noi_dung"]!.GetValue<string>());
                            break;
                        case "tu_choi":
                            Log($"Bị từ chối: {tin["ly_do"]}");
                            break;
                    }
                }
            }
            catch (Exception e) when (e is IOException || e is SocketException || e is JsonException || e is ObjectDisposedException) { }
            lock (khoa) dung = true;                             // server đóng kết nối -> client dừng
        }

        void NhanUdp()
        {
            var buf = new byte[2048];
            while (true)
            {
                int n;
                try { n = udp.Receive(buf); }
                catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }
                var r = TachRtp(buf.AsSpan(0, n));
                if (r is { } g && g.Pt == PtAmThanh)
                    lock (khoa) boDem.NhanGoi(g.Seq, g.Ts, g.Payload, Now());
            }
        }

        void Lenh(string dong)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (p[0] == "tat_mic")                     // tự tắt: có hiệu lực ngay, không cần server đồng ý
            {
                lock (khoa) micMo = false;
                Log("> tat_mic");
                GuiJson(tcp, new JsonObject { ["loai"] = "tu_tat_mic" });
            }
            else if (p[0] == "bat_mic")                // tự bật: phải chờ server cho phép
            {
                Log("> bat_mic (xin server)");
                GuiJson(tcp, new JsonObject { ["loai"] = "tu_bat_mic" });
            }
            else if (p[0] == "am_luong" && p.Length == 2)
            {
                double v = double.Parse(p[1], CultureInfo.InvariantCulture);
                lock (khoa) amLuong = v;
                Log($"> am_luong {F(v, "F2")} (chỉ áp dụng tại client)");
                DanhMoc($"{ten} tự đặt âm lượng nghe {F(v, "F2")}");
                GuiJson(tcp, new JsonObject { ["loai"] = "am_luong_nghe", ["gia_tri"] = v });
            }
            else if (p[0] == "thoat") lock (khoa) dung = true;
            else Log($"lệnh không hợp lệ: {dong}");
        }

        void DocLenh()
        {
            string? dong;
            while ((dong = Console.ReadLine()) != null)
            {
                dong = dong.Split('#')[0].Trim();
                if (dong.StartsWith('@'))
                {
                    int sp = dong.IndexOf(' ');
                    string mocT = sp < 0 ? dong : dong[..sp];
                    dong = sp < 0 ? "" : dong[(sp + 1)..];
                    double cho = double.Parse(mocT[1..], CultureInfo.InvariantCulture) - Now();
                    if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
                }
                if (dong != "") Lenh(dong);
            }
        }

        foreach (var h in new Action[] { DocTcp, NhanUdp, DocLenh })
            new Thread(() => h()) { IsBackground = true }.Start();

        // ---- Vòng lặp 20 ms: thu + gửi, nhận + phát
        var rng = new Random((int)ma);                       // giả lập mất gói tất định (tái lập được)
        long seq = 0, ts = 0;
        int daGui = 0, boGiaLap = 0;
        long tongByte = 0;
        int soKhung = (int)(thoiGian * 1000 / KhungMs);
        var giu = DongGoiRtp(PtGiuKetNoi, 0, 0, ma);
        udp.Send(giu);                                       // đăng ký địa chỉ UDP với server ngay từ đầu
        var cuaSo = new List<short>();                       // mẫu của 1 giây gần nhất (để đo mức)
        double keTiep = Now();
        for (int n = 1; n <= soKhung; n++)
        {
            bool mo;
            double al;
            lock (khoa)
            {
                if (dung) break;
                mo = micMo; al = amLuong;
            }
            // 1. Thu và gửi
            var khung = nguon.LayKhung();
            if (mo)
            {
                if (rng.NextDouble() < matGoi) boGiaLap++;   // "mạng" làm rơi gói: số thứ tự vẫn tăng
                else
                {
                    var goi = DongGoiRtp(PtAmThanh, seq, ts, ma, PcmSangByte(khung));
                    udp.Send(goi);
                    daGui++;
                    tongByte += goi.Length + 28;             // + header UDP (8) và IPv4 (20)
                }
                seq++;
            }
            else if (n % 50 == 0) udp.Send(giu);             // mic tắt: mỗi giây một gói giữ kết nối
            ts += MauMoiKhung;                               // nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
            // 2. Nhận và phát
            byte[]? p;
            lock (khoa) p = boDem.Lay();
            var mau = p != null ? ByteSangPcm(p) : new short[MauMoiKhung];
            if (al != 1.0) mau = mau.Select(x => CatNgon(Math.Round(x * al))).ToArray();
            lock (khoa) banGhi.AddRange(mau);
            cuaSo.AddRange(mau);
            if (n % 50 == 0)
            {
                double db = MucDbfs(cuaSo);
                Log($"{F(n * KhungMs / 1000.0, "F1"),4}s nghe {ThanhMuc(db)} {ChuDb(db)} dBFS mic {(mo ? "mở" : "tắt")}");
                cuaSo.Clear();
            }
            keTiep += KhungMs / 1000.0;
            double cho = keTiep - Now();
            if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
        }

        // ---- Kết thúc: thống kê, lưu bản ghi
        double giay = Now();
        tcp.Close();
        udp.Close();
        Log($"Kết thúc sau {F(giay, "F1")} s");
        Log($"Gửi {daGui} gói ({F(tongByte * 8 / giay / 1000, "F0")} kbit/s gồm header), giả lập mất {boGiaLap}");
        var b = boDem;
        Log($"Nhận {b.Nhan} gói, mất {b.Mat}, trễ {b.Tre}, jitter {F(b.JitterMs, "F2")} ms");
        Log($"Phát: thiếu {b.Thieu} khung, cạn {b.Can} lần, đệm {dem} khung = {dem * KhungMs} ms");
        if (ghi != null)
        {
            string dir = Path.GetDirectoryName(ghi) ?? "";
            if (dir != "") Directory.CreateDirectory(dir);
            lock (khoa)
            {
                GhiWav(ghi, banGhi);
                string duongMoc = Path.Combine(dir, Path.GetFileNameWithoutExtension(ghi) + "_moc.json");
                var doc = new JsonObject { ["ten"] = ten, ["moc"] = moc };
                File.WriteAllText(duongMoc, doc.ToJsonString(new JsonSerializerOptions
                    { WriteIndented = true, Encoder = System.Text.Encodings.Web.JavaScriptEncoder.UnsafeRelaxedJsonEscaping }));
                Log($"Đã ghi {ghi} ({F(banGhi.Count / (double)TanSoMau, "F2")} s), {moc.Count} mốc sự kiện");
            }
        }
        return 0;
    }
}
