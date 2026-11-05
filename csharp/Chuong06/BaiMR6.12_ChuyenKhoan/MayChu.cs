using System.Text.Json.Nodes;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Lõi máy chủ ngân hàng: chuyển khoản + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// 5–20 tài khoản, mỗi tài khoản là MỘT ô của KhoTranhChap (giá trị = số dư, long, đồng). Thao tác ghi:
//     {"loai":"chuyen","ma_yc":k,"tu":"TK01","den":"TK02","so_tien":300000[,"noi_dung":"..."][,"nguoi":"u17"]}
// Luật: trừ tài khoản nguồn, cộng tài khoản đích trong CÙNG một thao tác (hai ô); số dư nguồn không đủ thì từ chối.
// Nhiều người chuyển chéo cùng lúc (A→B và B→A) nên một thao tác luôn đụng HAI tài nguyên:
//   khoa            khóa hai tài khoản theo THỨ TỰ MÃ (TK01 trước TK02) -> mọi luồng xin khóa cùng thứ tự, không thể có
//                   vòng chờ -> không khóa chết;
//   khoa_ngay_tho=1 khóa nguồn rồi mới khóa đích: A→B giữ A chờ B, B→A giữ B chờ A -> KHÓA CHẾT (deadlock). Khung
//                   phát hiện bằng hết thời gian chờ khóa (Monitor.TryEnter 2 s): giao dịch "nạn nhân" bị hủy, nhả khóa,
//                   giao dịch kia đi tiếp; máy chủ báo "khóa chết: …";
//   lac_quan        đọc (số dư, phiên bản) của cả hai tài khoản, rồi so sánh-và-đổi CẢ HAI một cách nguyên tử.
//
// Sổ kiểm toán ghi mọi giao dịch được chấp nhận và tự tính số dư theo sổ; đếm ba sai phạm:
//   so_du_am  "thấu chi"  – theo sổ, một giao dịch làm tài khoản nguồn âm (hai luồng cùng thấy "đủ tiền" rồi cùng trừ);
//   so_du_sai "lệch TK"   – số tài khoản có số dư cuối khác số dư theo sổ (mất cập nhật);
//   tong_lech "tổng lệch" – tổng tiền toàn hệ thống khác tổng ban đầu (tiền tự sinh ra / mất đi).

/// <summary>Chủ tài khoản, mã tài khoản, cách viết số tiền có dấu.</summary>
static class NganHang
{
    public static readonly string[] ChuTk = ["An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Lan", "Minh", "Nam",
                                             "Oanh", "Phúc", "Quân", "Sơn", "Tâm", "Uyên", "Vy", "Xuân", "Yến", "Thảo"];

    public static string MaTk(int i) => $"TK{i + 1:00}";

    /// <summary>+300.000 đ / -300.000 đ / 0 đ.</summary>
    public static string TienDau(long x) => (x > 0 ? "+" : "") + Chung.Tien(x);
}

sealed record GdChuyen(string Nguoi, string Tu, string Den, long SoTien);

sealed class SoKiemToanNganHang(Dictionary<string, long> soDuDau) : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("tong_lech", "tổng lệch"), ("so_du_sai", "lệch TK"), ("so_du_am", "thấu chi")];
    readonly Dictionary<string, long> soDu = new(soDuDau);   // số dư theo sổ (cộng trừ đúng từng giao dịch đã chấp nhận)
    readonly long tongDau = soDuDau.Values.Sum();

    protected override void KiemTra(object o)
    {
        var gd = (GdChuyen)o;
        soDu[gd.Tu] -= gd.SoTien;
        soDu[gd.Den] += gd.SoTien;
        if (soDu[gd.Tu] < 0) SaiPham("so_du_am", $"{gd.Nguoi} {gd.Tu}→{gd.Den}: {gd.Tu} còn {Chung.Tien(soDu[gd.Tu])}");
    }

    protected override Dictionary<string, long> KiemTraCuoi(IReadOnlyDictionary<string, object> doc)
    {
        if (soDu.Keys.Any(k => !doc.ContainsKey(k))) return [];
        long sai = soDu.Count(x => (long)doc[x.Key] != x.Value);
        long tong = soDu.Keys.Sum(k => (long)doc[k]);
        return new() { ["so_du_sai"] = sai, ["tong_lech"] = tong != tongDau ? 1 : 0 };
    }
}

sealed class MayChuNganHang(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                            IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new() { ["so_tk"] = 10L, ["so_du_dau"] = 5_000_000L, ["khoa_ngay_tho"] = 0L };
    public override string Ten => "Ngân hàng MR6.12";
    public override string[] LoaiYeuCau => ["chuyen"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    Dictionary<string, string> chu = [];                 // mã tài khoản -> chủ tài khoản
    List<string> dsTk = [];
    long tongDau;
    readonly object khoaLs = new();
    LinkedList<JsonObject> lichSu = new();               // giao dịch đã chấp nhận, mới nhất trước (tối đa 50)
    long maGd;                                           // mã giao dịch tăng mãi, kể cả qua các lần đặt lại
    long soGd;
    readonly object khoaKc = new();
    long khoaChet, kcChuaBao;
    double lucBaoKc = double.NegativeInfinity;

    // ------------------------------------------------------------ tạo / đặt lại
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("so_tk", 10L) < 5 || (long)ts.GetValueOrDefault("so_tk", 10L) > NganHang.ChuTk.Length)
            throw new ArgumentException($"số tài khoản 5–{NganHang.ChuTk.Length}");
        if ((long)ts.GetValueOrDefault("so_du_dau", 0L) < 0) throw new ArgumentException("số dư ban đầu ≥ 0");
        if ((long)ts.GetValueOrDefault("khoa_ngay_tho", 0L) is not (0 or 1)) throw new ArgumentException("khoa_ngay_tho = 0 hoặc 1");
    }

    protected override void KhoiTao()
    {
        int soTk = (int)TsL("so_tk");
        long dau = TsL("so_du_dau");
        var ds = Enumerable.Range(0, soTk).Select(NganHang.MaTk).ToList();
        foreach (var m in ds) Kho.DatO(m, dau);
        Kho.KhoaNgayTho = TsL("khoa_ngay_tho") == 1;
        So = new SoKiemToanNganHang(ds.ToDictionary(m => m, _ => dau));
        chu = ds.Select((m, i) => (m, i)).ToDictionary(x => x.m, x => NganHang.ChuTk[x.i]);
        dsTk = ds;
        tongDau = soTk * dau;
        lock (khoaLs) lichSu = new();
        Interlocked.Exchange(ref soGd, 0);
        lock (khoaKc) (khoaChet, kcChuaBao, lucBaoKc) = (0, 0, double.NegativeInfinity);
    }

    // ------------------------------------------------------------ chuyển khoản
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        string? tu = Chung.Chuoi(tin, "tu"), den = Chung.Chuoi(tin, "den");
        string? noiDung = tin.ContainsKey("noi_dung") ? Chung.Chuoi(tin, "noi_dung") : "";
        static JsonObject Loi(string ma, string lyDo) => new() { ["ok"] = false, ["ma_loi"] = ma, ["ly_do"] = lyDo };
        if (!Chung.LaSoNguyen(tin["so_tien"], out long x) || x <= 0)
        {
            traLoi(Loi("sai_du_lieu", "số tiền phải là số nguyên dương"));
            return;
        }
        var bangChu = chu;
        if (tu == null || den == null || !bangChu.ContainsKey(tu) || !bangChu.ContainsKey(den))
        {
            traLoi(Loi("khong_co", "tài khoản không tồn tại"));
            return;
        }
        if (tu == den)
        {
            traLoi(Loi("sai_du_lieu", "tài khoản nguồn và đích trùng nhau"));
            return;
        }
        if (noiDung == null || noiDung.EnumerateRunes().Count() > 40)
        {
            traLoi(Loi("sai_du_lieu", "nội dung tối đa 40 ký tự"));
            return;
        }

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN: đọc hai số dư
        {
            long a = (long)doc[tu], b = (long)doc[den];
            if (a < x)
                return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_du", ["ly_do"] = $"không đủ số dư ({tu} còn {Chung.Tien(a)})", ["so_du"] = a }, null);
            return (new JsonObject
            {
                ["ok"] = true, ["tu"] = tu, ["den"] = den, ["so_tien"] = x, ["so_du"] = a - x, ["nguoi"] = nguoi, ["noi_dung"] = noiDung,
            }, new() { [tu] = a - x, [den] = b + x });
        }

        void Tra(JsonObject kq)
        {
            if (Chung.Chuoi(kq, "ma_loi") == "khoa_cho_lau") BaoKhoaChet(nguoi, tu, den, kq);
            traLoi(kq);
        }

        // thứ tự [nguồn, đích]: chế độ khoa sắp xếp lại theo mã, khóa ngây thơ thì giữ nguyên
        Kho.ThucHien([tu, den], XuLy, Tra, DaChuyen);
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi hai số dư: ghi sổ kiểm toán đúng thứ tự ghi.</summary>
    void DaChuyen(JsonObject kq)
    {
        long ma = Interlocked.Increment(ref maGd);
        kq["ma_gd"] = ma;
        Interlocked.Increment(ref soGd);
        string nguoi = Chung.Chuoi(kq, "nguoi")!, tu = Chung.Chuoi(kq, "tu")!, den = Chung.Chuoi(kq, "den")!;
        long x = Chung.SoL(kq, "so_tien");
        So.Ghi(new GdChuyen(nguoi, tu, den, x));
        lock (khoaLs)
        {
            lichSu.AddFirst(new JsonObject
            {
                ["ma"] = ma, ["gio"] = DateTime.Now.ToString("HH:mm:ss"), ["nguoi"] = nguoi, ["tu"] = tu, ["den"] = den, ["so_tien"] = x,
                ["noi_dung"] = Chung.Chuoi(kq, "noi_dung"),
            });
            while (lichSu.Count > 50) lichSu.RemoveLast();
        }
        BaoDoi();
    }

    /// <summary>Chờ khóa quá 2 s: với khóa ngây thơ đó là khóa chết. Báo (tối đa 1 dòng/giây) và hủy giao dịch.</summary>
    void BaoKhoaChet(string nguoi, string tu, string den, JsonObject kq)
    {
        string k = (Chung.Chuoi(kq, "ly_do") ?? "").Split(' ', StringSplitOptions.RemoveEmptyEntries).ElementAtOrDefault(2) ?? "?";   // "chờ khóa TK02 quá 2 s (khóa chết?)"
        bool ngayTho = Kho.KhoaNgayTho;
        bool bao;
        long them = 0;
        lock (khoaKc)
        {
            khoaChet++;
            double bayGio = Chung.Now;
            bao = bayGio - lucBaoKc >= 1.0;
            if (bao) (lucBaoKc, them, kcChuaBao) = (bayGio, kcChuaBao, 0);
            else kcChuaBao++;
        }
        string g = Chung.G(Chung.ChoKhoaToiDaS);
        if (bao)
            ThongBao($"{(ngayTho ? "khóa chết" : "chờ khóa quá hạn")}: {nguoi} {tu}→{den}, chờ {k} quá {g} s" + (them > 0 ? $" (+{them} vụ)" : ""));
        kq["ly_do"] = (ngayTho ? "khóa chết: " : "") + $"chờ {k} quá {g} s, đã hủy giao dịch";
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        var snap = Kho.AnhChup();
        var ds = dsTk;
        var bangChu = chu;
        var tk = new JsonArray();
        long tong = 0;
        foreach (var m in ds)
        {
            long soDu = snap.TryGetValue(m, out var v) ? (long)v : 0;
            tong += soDu;
            tk.Add(new JsonObject { ["ma"] = m, ["chu"] = bangChu.GetValueOrDefault(m, "?"), ["so_du"] = soDu });
        }
        List<JsonObject> ls;
        lock (khoaLs) ls = [.. lichSu.Take(20)];
        long kc;
        lock (khoaKc) kc = khoaChet;
        return new JsonObject
        {
            ["tai_khoan"] = tk, ["tong"] = tong, ["tong_dau"] = tongDau, ["khoa_ngay_tho"] = Kho.KhoaNgayTho, ["khoa_chet"] = kc,
            ["so_gd"] = Interlocked.Read(ref soGd), ["lich_su"] = new JsonArray([.. ls.Select(x => x.DeepClone())]),
        };
    }

    protected override string TomTat()
    {
        var snap = Kho.AnhChup();
        long tong = dsTk.Sum(m => snap.TryGetValue(m, out var v) ? (long)v : 0);
        long lech = tong - tongDau;
        return lech != 0 ? $"Tổng tiền {Chung.Tien(tong)}, LỆCH {NganHang.TienDau(lech)}" : $"Tổng tiền {Chung.Tien(tong)} (khớp), {dsTk.Count} tài khoản";
    }
}
