using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text.Json.Nodes;
using static BaiMR5_1.Khung;

namespace BaiMR5_1;

// Client truyền nhiều tệp đồng thời, có điều khiển start/pause/resume/stop,
// giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.
// Vai:
//   client <host> <port> [thư_mục_tải]   – giao diện dòng lệnh: nhập lệnh qua bàn phím.
//   demo   <host> <port>                 – tự dựng server cục bộ, tạo tệp thử, chạy kịch bản tự động.
// Lệnh của giao diện client (mỗi dòng một lệnh):
//   start <tên> [download|upload] [tốc_độ]   bắt đầu một lượt (mặc định download); tốc_độ = giới hạn tệp
//   pause <tên>        tạm dừng (giữ tệp .part)        resume <tên>   tải tiếp từ offset
//   stop  <tên>        dừng hẳn (xóa tệp .part)         speed <tốc_độ> đổi giới hạn tốc độ phiên
//   status             xem tiến trình                   quit           thoát
// Đơn vị tốc độ: 512K, 1M, 2MiB, hoặc số byte/giây.
static class TT
{
    public const string Chay = "chạy", TamDung = "tạm dừng", Dung = "dừng", Xong = "xong", Loi = "lỗi";
}

/// <summary>Một lượt truyền (một tệp). Chạy trong luồng riêng, điều khiển qua thuộc tính TrangThai.</summary>
class LuotTruyen(string ten, string huong, string cucBo, double rateTep)
{
    public readonly string Ten = ten;
    public readonly string Huong = huong;          // "download" | "upload"
    public readonly string CucBo = cucBo;          // đường tệp phía client
    public volatile string TrangThai = TT.Chay;
    public long Tong, XongByte;
    public string? ShaServer;
    public bool? ShaOk;
    public double T0 = Now, ThoiGian;
    public readonly ThungToken Bucket = new(rateTep);   // giới hạn riêng của tệp
    public bool DungKhi() => TrangThai == TT.TamDung || TrangThai == TT.Dung;
}

class QuanLy
{
    readonly string host, thuMuc, khoServer;
    readonly int port;
    public ThungToken Session = new(KhongGioiHan);
    public readonly Dictionary<string, LuotTruyen> Luot = [];   // chỉ luồng điều khiển ghi

    public QuanLy(string host, int port, string thuMucTai, string khoServer)
    {
        this.host = host; this.port = port; thuMuc = thuMucTai; this.khoServer = khoServer;
        Directory.CreateDirectory(thuMucTai);
    }

    // ------------- Điều khiển -------------
    public void DatTocDoPhien(double rate)
    {
        Session.DatRate(rate);
        Console.WriteLine($"[client] Đặt tốc độ phiên = {DinhDangByte((long)rate)}/s");
    }

    public void BatDau(string ten, string huong = "download", double rateTep = KhongGioiHan)
    {
        if (Luot.TryGetValue(ten, out var cu) && cu.TrangThai == TT.Chay)
        {
            Console.WriteLine($"[client] {ten} đang chạy");
            return;
        }
        var t = new LuotTruyen(ten, huong, Path.Combine(thuMuc, ten), rateTep);
        Luot[ten] = t;
        Start(t);
        string gh = rateTep >= KhongGioiHan ? "" : $", giới hạn tệp {DinhDangByte((long)rateTep)}/s";
        Console.WriteLine($"[client] BẮT ĐẦU {huong} {ten}{gh}");
    }

    public void Start(LuotTruyen t)
    {
        t.T0 = Now;
        new Thread(() => { if (t.Huong == "download") TaiXuong(t); else TaiLen(t); }) { IsBackground = true }.Start();
    }

    public void TamDung(string ten)
    {
        if (Luot.TryGetValue(ten, out var t) && t.TrangThai == TT.Chay)
        {
            t.TrangThai = TT.TamDung;
            Console.WriteLine($"[client] TẠM DỪNG {ten} (giữ .part)");
        }
    }

    public void TaiTiep(string ten)
    {
        if (Luot.TryGetValue(ten, out var t) && t.TrangThai == TT.TamDung)
        {
            Console.WriteLine($"[client] TẢI TIẾP {ten}");
            BatDau(ten, t.Huong, t.Bucket.Rate);
        }
    }

    public void DungHan(string ten)
    {
        if (Luot.TryGetValue(ten, out var t) && (t.TrangThai == TT.Chay || t.TrangThai == TT.TamDung))
        {
            t.TrangThai = TT.Dung;
            Console.WriteLine($"[client] DỪNG {ten} (xóa .part)");
        }
    }

    public void TrangThai()
    {
        Console.WriteLine("[client] --- Trạng thái ---");
        foreach (var (ten, t) in Luot)
        {
            double pct = t.Tong > 0 ? 100.0 * t.XongByte / t.Tong : 0;
            Console.WriteLine($"[client]   {ten,-12} {t.Huong,-8} {t.TrangThai,-8} " +
                              $"{ThanhTienDo(pct)} {pct.ToString("F1", CultureInfo.InvariantCulture),5}%  " +
                              $"{DinhDangByte(t.XongByte)}/{DinhDangByte(t.Tong)}");
        }
    }

    public static string ThanhTienDo(double pct, int rong = 20)
    {
        int day = (int)(pct / 100 * rong);
        return "[" + new string('#', day) + new string('-', rong - day) + "]";
    }

    Socket Connect()
    {
        var s = new Socket(SocketType.Stream, ProtocolType.Tcp);
        s.Connect(host, port);
        return s;
    }

    // ------------- Thân luồng truyền -------------
    void TaiXuong(LuotTruyen t)
    {
        string part = t.CucBo + ".part";
        long off = File.Exists(part) ? new FileInfo(part).Length : 0;
        try
        {
            using var s = Connect();
            GuiKhung(s, new JsonObject { ["op"] = "download", ["name"] = t.Ten, ["offset"] = off });
            var resp = NhanKhung(s);
            if (resp["ok"]?.GetValue<bool>() != true)
            {
                t.TrangThai = TT.Loi;
                Console.WriteLine($"[client] {t.Ten}: lỗi {resp["loi"]}");
                return;
            }
            t.Tong = resp["size"]!.GetValue<long>();
            t.ShaServer = resp["sha256"]!.GetValue<string>();
            t.XongByte = off;
            if (off >= t.Tong)                          // đã đủ: chỉ cần kiểm và đổi tên
            {
                s.Close();
                HoanTatTaiXuong(t, part);
                return;
            }
            using (var f = new FileStream(part, off > 0 ? FileMode.Open : FileMode.Create))
            {
                f.Seek(off, SeekOrigin.Begin);
                var buf = new byte[Chunk];
                while (t.XongByte < t.Tong)
                {
                    if (t.TrangThai == TT.Dung)
                    {
                        f.Close(); s.Close();
                        File.Delete(part);
                        return;
                    }
                    if (t.TrangThai == TT.TamDung) return;   // 'using' đóng tệp và socket, giữ .part
                    int n = s.Receive(buf);
                    if (n == 0) break;
                    f.Write(buf, 0, n);
                    t.XongByte += n;
                    Session.TieuThu(n, t.DungKhi);           // điều tiết theo phiên
                    t.Bucket.TieuThu(n, t.DungKhi);          // và theo tệp
                }
            }
            s.Close();
            if (t.XongByte >= t.Tong) HoanTatTaiXuong(t, part);
        }
        catch (Exception e) when (e is IOException || e is SocketException)
        {
            if (t.TrangThai != TT.Dung && t.TrangThai != TT.TamDung)
            {
                t.TrangThai = TT.Loi;
                Console.WriteLine($"[client] {t.Ten}: lỗi kết nối {e.Message}");
            }
        }
    }

    void HoanTatTaiXuong(LuotTruyen t, string part)
    {
        t.ShaOk = Sha256Tep(part) == t.ShaServer;
        File.Move(part, t.CucBo, overwrite: true);
        t.ThoiGian = Now - t.T0;
        t.TrangThai = TT.Xong;
        Console.WriteLine($"[client] XONG download {t.Ten}: {DinhDangByte(t.Tong)} trong {t.ThoiGian.ToString("F1", CultureInfo.InvariantCulture)}s " +
                          $"({DinhDangByte((long)(t.Tong / Math.Max(t.ThoiGian, 0.001)))}/s), " +
                          $"SHA-256 {(t.ShaOk == true ? "KHỚP" : "SAI")}");
    }

    public void TaiLen(LuotTruyen t)
    {
        try
        {
            long total = new FileInfo(t.CucBo).Length;
            string partServer = Path.Combine(khoServer, t.Ten + ".part");
            long off = File.Exists(partServer) ? new FileInfo(partServer).Length : 0;
            string sha = Sha256Tep(t.CucBo);
            using var s = Connect();
            GuiKhung(s, new JsonObject { ["op"] = "upload", ["name"] = t.Ten, ["offset"] = off, ["size"] = total, ["sha256"] = sha });
            var resp = NhanKhung(s);
            if (resp["ok"]?.GetValue<bool>() != true)
            {
                t.TrangThai = TT.Loi;
                return;
            }
            t.Tong = total; t.XongByte = off;
            using (var f = File.OpenRead(t.CucBo))
            {
                f.Seek(off, SeekOrigin.Begin);
                var buf = new byte[Chunk];
                while (t.XongByte < total)
                {
                    if (t.DungKhi()) return;
                    int n = f.Read(buf, 0, buf.Length);
                    if (n == 0) break;
                    Session.TieuThu(n, t.DungKhi);
                    t.Bucket.TieuThu(n, t.DungKhi);
                    SendAll(s, buf, n);
                    t.XongByte += n;
                }
            }
            var ket = NhanKhung(s);
            t.ThoiGian = Now - t.T0;
            t.ShaOk = ket["sha256_ok"]?.GetValue<bool>();
            t.TrangThai = TT.Xong;
            Console.WriteLine($"[client] XONG upload {t.Ten}: {DinhDangByte(total)} trong {t.ThoiGian.ToString("F1", CultureInfo.InvariantCulture)}s " +
                              $"({DinhDangByte((long)(total / Math.Max(t.ThoiGian, 0.001)))}/s), " +
                              $"server kiểm SHA-256 {(t.ShaOk == true ? "KHỚP" : "SAI")}");
        }
        catch (Exception e) when (e is IOException || e is SocketException)
        {
            if (!t.DungKhi())
            {
                t.TrangThai = TT.Loi;
                Console.WriteLine($"[client] {t.Ten}: lỗi kết nối {e.Message}");
            }
        }
    }
}

static class TruyenClient
{
    // ----------------------------- Vai: giao diện dòng lệnh -----------------------------
    public static int ChayGiaoDien(string host, int port, string thuMuc)
    {
        var ql = new QuanLy(host, port, thuMuc, khoServer: "kho_server");
        Console.WriteLine("[client] Nhập lệnh (start/pause/resume/stop/speed/status/quit).");
        string? dong;
        while ((dong = Console.ReadLine()) != null)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (p.Length == 0) continue;
            try
            {
                switch (p[0].ToLowerInvariant())
                {
                    case "start":
                        ql.BatDau(p[1], p.Length > 2 ? p[2] : "download", p.Length > 3 ? DocSoByte(p[3]) : KhongGioiHan);
                        break;
                    case "pause": ql.TamDung(p[1]); break;
                    case "resume": ql.TaiTiep(p[1]); break;
                    case "stop": ql.DungHan(p[1]); break;
                    case "speed": ql.DatTocDoPhien(DocSoByte(p[1])); break;
                    case "status": ql.TrangThai(); break;
                    case "quit" or "exit": return 0;
                    default: Console.WriteLine("[client] Lệnh lạ"); break;
                }
            }
            catch (IndexOutOfRangeException)
            {
                Console.WriteLine("[client] Thiếu tham số");
            }
        }
        return 0;
    }

    // ----------------------------- Vai: demo tự động -----------------------------
    static bool ChoDen(Func<bool> dieuKien, double han = 30)
    {
        double het = Now + han;
        while (Now < het)
        {
            if (dieuKien()) return true;
            Thread.Sleep(100);
        }
        return false;
    }

    public static int ChayDemo(string host, int port)
    {
        string goc = Directory.CreateTempSubdirectory("mr51_").FullName;
        string kho = Path.Combine(goc, "kho_server"), tai = Path.Combine(goc, "tai_ve"), nguon = Path.Combine(goc, "nguon");
        foreach (var d in new[] { kho, tai, nguon }) Directory.CreateDirectory(d);

        // Server cục bộ trong một luồng (cùng mã với vai server).
        var srv = new TcpListener(IPAddress.Parse(host), port);
        srv.Start(16);
        new Thread(() => TruyenServer.VongServer(srv, kho)) { IsBackground = true }.Start();
        Console.WriteLine($"[demo] Server cục bộ tại {host}:{port}, thư mục tạm {goc}");

        // Tạo 3 tệp thử (3 MiB mỗi tệp) trực tiếp trong kho server để tải xuống.
        string[] cacTep = ["tep1.bin", "tep2.bin", "tep3.bin"];
        foreach (var ten in cacTep) File.WriteAllBytes(Path.Combine(kho, ten), RandomNumberGenerator.GetBytes(3 * 1024 * 1024));
        Console.WriteLine($"[demo] Đã tạo {cacTep.Length} tệp thử, mỗi tệp 3 MiB");

        var ql = new QuanLy(host, port, tai, kho);

        // Luồng in tiến trình định kỳ.
        bool dangChay = true;
        new Thread(() =>
        {
            var truoc = new Dictionary<string, (long Byte, double Tg)>();
            while (dangChay)
            {
                Thread.Sleep(800);
                foreach (var (ten, t) in ql.Luot.ToList())
                {
                    if (t.TrangThai != TT.Chay || t.Tong == 0) continue;
                    var (truocByte, truocTg) = truoc.GetValueOrDefault(ten, (0, t.T0));
                    double now = Now;
                    // tệp bị dừng rồi chạy lại (lượt mới có T0 mới): đo từ lúc lượt mới bắt đầu,
                    // từ offset cũ nếu tải tiếp, từ 0 nếu chạy lại từ đầu
                    if (t.XongByte < truocByte || truocTg < t.T0)
                        (truocByte, truocTg) = (t.XongByte < truocByte ? 0 : truocByte, t.T0);
                    double toc = Math.Max(0, t.XongByte - truocByte) / Math.Max(now - truocTg, 0.001);
                    truoc[ten] = (t.XongByte, now);
                    double pct = 100.0 * t.XongByte / t.Tong;
                    Console.WriteLine($"[tiến trình] {ten} {QuanLy.ThanhTienDo(pct)} {pct.ToString("F1", CultureInfo.InvariantCulture),5}%  " +
                                      $"{DinhDangByte((long)toc)}/s");
                }
            }
        }) { IsBackground = true }.Start();

        Console.WriteLine("\n===== KỊCH BẢN: tải xuống 3 tệp đồng thời, giới hạn phiên 2 MiB/s =====");
        ql.DatTocDoPhien(DocSoByte("2M"));
        foreach (var ten in cacTep) ql.BatDau(ten, "download");

        Thread.Sleep(1200);
        ql.TamDung("tep2.bin");                        // tạm dừng giữa chừng
        Thread.Sleep(800);
        ql.DungHan("tep3.bin");                        // dừng hẳn, xóa .part
        Thread.Sleep(300);
        Console.WriteLine("\n===== Nâng tốc độ phiên lên 8 MiB/s, tải tiếp tep2, chạy lại tep3 =====");
        ql.DatTocDoPhien(DocSoByte("8M"));
        ql.TaiTiep("tep2.bin");                        // tải tiếp từ offset
        ql.BatDau("tep3.bin", "download");             // chạy lại từ đầu

        ChoDen(() => cacTep.All(t => ql.Luot[t].TrangThai is TT.Xong or TT.Loi), 30);
        ql.TrangThai();

        Console.WriteLine("\n===== KỊCH BẢN upload: đẩy tep1 (đã tải) lên server, giới hạn tệp 1 MiB/s =====");
        // Dùng tệp vừa tải về làm nguồn upload; đặt tên đích khác để không đè tệp gốc.
        var ql2 = new QuanLy(host, port, tai, kho) { Session = ql.Session };
        string dich = Path.Combine(nguon, "tep1_tai_len.bin");
        File.Copy(Path.Combine(tai, "tep1.bin"), dich);
        var up = new LuotTruyen("tep1_tai_len.bin", "upload", dich, DocSoByte("1M"));
        ql2.Luot[up.Ten] = up;
        ql2.Start(up);
        Console.WriteLine("[client] BẮT ĐẦU upload tep1_tai_len.bin, giới hạn tệp 1.00 MiB/s");
        ChoDen(() => up.TrangThai is TT.Xong or TT.Loi, 20);

        dangChay = false;
        Thread.Sleep(200);
        Console.WriteLine("\n===== TỔNG KẾT =====");
        foreach (var ten in cacTep)
        {
            var tt = ql.Luot[ten];
            Console.WriteLine($"[demo] {ten}: {tt.TrangThai}, {DinhDangByte(tt.XongByte)}, SHA-256 {(tt.ShaOk == true ? "KHỚP" : "—")}");
        }
        Console.WriteLine($"[demo] tep1_tai_len.bin: {up.TrangThai}, server kiểm SHA-256 {(up.ShaOk == true ? "KHỚP" : "—")}");
        var conLai = Directory.GetFiles(kho).Select(Path.GetFileName).Order(StringComparer.Ordinal).Select(n => $"'{n}'");
        Console.WriteLine($"[demo] Server còn giữ: [{string.Join(", ", conLai)}]");

        srv.Stop();
        try { Directory.Delete(goc, recursive: true); } catch (IOException) { }
        Console.WriteLine($"[demo] Đã dọn thư mục tạm {goc}");
        return 0;
    }
}
