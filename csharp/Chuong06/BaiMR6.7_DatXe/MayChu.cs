using System.Text.Json.Nodes;

namespace BaiMR6_7;

// Bài mở rộng MR6.7 – Lõi máy chủ đặt xe: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// m chiếc xe (mặc định 1), mỗi xe MỘT chỗ; n người cùng bấm "Đặt xe". Hai thao tác ghi:
//     {"loai":"dat_xe","ma_yc":k[,"don":[x,y],"den":[x,y]][,"nguoi":"u17"]}
//     {"loai":"huy","ma_yc":k[,"nguoi":"u17"]}
// Luật: mỗi người chỉ giữ một chuyến; máy chủ giao xe TRỐNG gần điểm đón nhất; hết xe trống -> "hết chỗ".
// Hủy chuyến trả xe về trạng thái trống.
//
// Bản đồ là lưới 5 × 5 đường (tọa độ 0–1, 1 đơn vị = 5 km); điểm đón/đến là ngã tư. Mỗi xe là MỘT ô "xe1",
// "xe2"… của KhoTranhChap: record bất biến Xe(Ma, TaiXe, BienSo, Mau, ViTri, Khach, Don, Den, Gio, Cuoc, Phut). Vì
// phải biết người đặt đã có chuyến chưa và xe nào trống gần nhất, đặt xe đọc–ghi CẢ m ô trong một thao tác (khoa:
// giữ khóa mọi xe theo thứ tự tên ô; lac_quan: so sánh phiên bản mọi xe).
//
// Sổ kiểm toán đếm một sai phạm:
//   trung_cho – một lần đặt được chấp nhận cho một xe mà (theo sổ) đang chở người khác: hai người cùng đọc "xe
//               trống", cùng được báo "Đặt thành công – tài xế Hùng", người ghi sau đè người ghi trước, người kia
//               đứng đường chờ một chiếc xe không đến.

/// <summary>Bản đồ phố: 5 đường dọc × 5 đường ngang, ngã tư, khoảng cách, giá cước.</summary>
static class BanDoPho
{
    public static readonly double[] Duong = [0.1, 0.3, 0.5, 0.7, 0.9];          // tọa độ 5 đường dọc / 5 đường ngang
    public static readonly string[] TenDoc = ["Lê Lợi", "Hai Bà Trưng", "Nguyễn Huệ", "Pasteur", "Đồng Khởi"];
    public static readonly string[] TenNgang = ["Lý Tự Trọng", "Lê Thánh Tôn", "Nguyễn Du", "Võ Văn Tần", "Điện Biên Phủ"];
    public static readonly (double X, double Y)[] NgaTu = [.. Duong.SelectMany(y => Duong.Select(x => (x, y)))];
    public const double KmMoiDonVi = 5.0;
    public static readonly (string Ten, string BienSo, string Mau)[] TaiXe =
    [
        ("Hùng", "51A-123.45", "trắng"), ("Lan", "51G-678.90", "đỏ"), ("Minh", "51F-246.80", "xanh"),
        ("Tuấn", "51H-135.79", "bạc"), ("Hoa", "51K-864.20", "vàng"), ("Phúc", "51A-975.31", "đen"),
        ("Quang", "51G-112.23", "cam"), ("Thảo", "51F-445.56", "tím"), ("Sơn", "51H-778.89", "xám"),
        ("Vy", "51K-990.01", "hồng"),
    ];
    public static readonly (double X, double Y)[] ViTriXe =
        [(0.9, 0.1), (0.1, 0.9), (0.5, 0.3), (0.3, 0.7), (0.7, 0.5), (0.1, 0.3), (0.9, 0.7), (0.5, 0.9), (0.3, 0.1), (0.7, 0.9)];

    /// <summary>(0.1, 0.5) -> "Lê Lợi × Nguyễn Du".</summary>
    public static string TenDiem((double X, double Y) p) =>
        $"{TenDoc[Array.IndexOf(Duong, p.X)]} × {TenNgang[Array.IndexOf(Duong, p.Y)]}";

    /// <summary>Đường đi trên lưới phố (Manhattan) giữa hai điểm, km.</summary>
    public static double Km((double X, double Y) a, (double X, double Y) b) => (Math.Abs(a.X - b.X) + Math.Abs(a.Y - b.Y)) * KmMoiDonVi;

    /// <summary>Giá cước: 12.000 đ mở cửa + 10.000 đ/km, làm tròn tới 1.000 đ.</summary>
    public static long Cuoc((double X, double Y) a, (double X, double Y) b) => (long)Math.Round((12_000 + 10_000 * Km(a, b)) / 1000) * 1000;

    /// <summary>Thời gian xe tới điểm đón (30 km/h = 0,5 km/phút), tối thiểu 1 phút.</summary>
    public static long PhutDen((double X, double Y) xe, (double X, double Y) don) => Math.Max(1, (long)Math.Round(Km(xe, don) / 0.5));

    /// <summary>Kiểm tra [x, y] là một ngã tư trên bản đồ (làm tròn 2 chữ số); trả về ngã tư hoặc null.</summary>
    public static (double X, double Y)? Diem(JsonNode? n)
    {
        if (n is not JsonArray a || a.Count != 2 || !a.All(Chung.LaSo)) return null;
        double x = Math.Round(Chung.SoD(a[0])!.Value, 2), y = Math.Round(Chung.SoD(a[1])!.Value, 2);
        foreach (var p in NgaTu)
            if (Math.Abs(p.X - x) < 1e-9 && Math.Abs(p.Y - y) < 1e-9) return p;
        return null;
    }

    public static JsonArray Json((double X, double Y) p) => [p.X, p.Y];

    public static (double X, double Y) DocDiem(JsonNode? n) =>
        n is JsonArray { Count: 2 } a ? (Chung.SoD(a[0]) ?? 0, Chung.SoD(a[1]) ?? 0) : (0, 0);
}

/// <summary>Một xe (giá trị của ô "xe1"…). Khach = null: xe trống.</summary>
sealed record Xe(long Ma, string TaiXe, string BienSo, string Mau, (double X, double Y) ViTri, string? Khach = null,
                 (double X, double Y)? Don = null, (double X, double Y)? Den = null, string? Gio = null, long? Cuoc = null,
                 long? Phut = null)
{
    public JsonObject Json() => new()
    {
        ["ma"] = Ma, ["tai_xe"] = TaiXe, ["bien_so"] = BienSo, ["mau"] = Mau, ["vi_tri"] = BanDoPho.Json(ViTri),
        ["khach"] = Khach, ["don"] = Don is { } d ? BanDoPho.Json(d) : null, ["den"] = Den is { } e ? BanDoPho.Json(e) : null,
        ["gio"] = Gio, ["cuoc"] = Cuoc, ["phut"] = Phut,
    };
}

sealed record GdDatXe(string Loai, long Xe, string Nguoi);

sealed class SoKiemToanDatXe : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("trung_cho", "trùng chỗ")];
    readonly Dictionary<long, List<string>> trenXe = [];   // theo sổ: ai đang "có" chỗ trên xe i
    long soDat;                                              // số lần đặt được chấp nhận

    public SoKiemToanDatXe(long soXe)
    {
        for (long i = 1; i <= soXe; i++) trenXe[i] = [];
    }

    public long SoDat => Interlocked.Read(ref soDat);

    protected override void KiemTra(object o)
    {
        var gd = (GdDatXe)o;
        var ds = trenXe[gd.Xe];
        if (gd.Loai == "dat")
        {
            Interlocked.Increment(ref soDat);
            if (ds.Count > 0) SaiPham("trung_cho", $"xe {gd.Xe}: {gd.Nguoi} khi {ds[^1]} đang đặt");
            ds.Add(gd.Nguoi);
        }
        else ds.Remove(gd.Nguoi);                                    // hủy
    }
}

sealed class MayChuDatXe(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                         IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new() { ["so_xe"] = 1L };
    public override string Ten => "Đặt xe MR6.7";
    public override string[] LoaiYeuCau => ["dat_xe", "huy"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    List<string> cacO = [];
    readonly object khoaLs = new();
    LinkedList<JsonObject> lichSu = new();              // tối đa 30 lần ghi gần nhất (để hiển thị)
    long demGhi;                                        // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    long phien;

    SoKiemToanDatXe SoXe => (SoKiemToanDatXe)So;

    // ------------------------------------------------------------ tạo / đặt lại
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("so_xe", 1L) is < 1 or > 50) throw new ArgumentException("số xe 1–50");
    }

    protected override void KhoiTao()
    {
        long m = TsL("so_xe");
        cacO = [.. Enumerable.Range(1, (int)m).Select(i => $"xe{i}")];
        int n = BanDoPho.TaiXe.Length;
        for (int i = 0; i < m; i++)
        {
            var (ten, bien, mau) = BanDoPho.TaiXe[i % n];
            if (i >= n) ten = $"{ten} {i / n + 1}";
            var viTri = i < BanDoPho.ViTriXe.Length ? BanDoPho.ViTriXe[i] : BanDoPho.NgaTu[i * 7 % BanDoPho.NgaTu.Length];
            Kho.DatO($"xe{i + 1}", new Xe(i + 1, ten, bien, mau, viTri));
        }
        So = new SoKiemToanDatXe(m);
        lock (khoaLs) lichSu = new();
    }

    // ------------------------------------------------------------ đặt xe / hủy
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        bool that = khach.SoNguoi == null;              // người dùng thật (không phải bộ mô phỏng): ghi nhật ký
        if (Chung.Chuoi(tin, "loai") == "dat_xe")
        {
            var don = Chung.LaNull(tin["don"]) ? (0.5, 0.5) : BanDoPho.Diem(tin["don"]);
            var den = Chung.LaNull(tin["den"]) ? (0.9, 0.9) : BanDoPho.Diem(tin["den"]);
            if (don is not { } d0 || den is not { } d1)
            {
                traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = "điểm đón/đến phải là ngã tư [x, y]" });
                return;
            }

            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN: chọn xe trống gần nhất
            {
                var ds = doc.Values.Cast<Xe>().ToList();
                foreach (var x in ds)
                    if (x.Khach == nguoi)
                        return (new JsonObject { ["ok"] = false, ["ma_loi"] = "da_dat", ["ly_do"] = $"bạn đã đặt xe {x.TaiXe}" }, null);
                var trong = ds.Where(x => x.Khach == null).ToList();
                if (trong.Count == 0)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_cho", ["ly_do"] = "hết chỗ – không còn xe trống" }, null);
                var xe = trong.OrderBy(x => BanDoPho.Km(x.ViTri, d0)).ThenBy(x => x.Ma).First();
                var moi = xe with
                {
                    Khach = nguoi, Don = d0, Den = d1, Gio = DateTime.Now.ToString("HH:mm:ss"), Cuoc = BanDoPho.Cuoc(d0, d1),
                    Phut = BanDoPho.PhutDen(xe.ViTri, d0),
                };
                var kq = new JsonObject
                {
                    ["ok"] = true, ["xe"] = xe.Ma, ["khach"] = nguoi, ["tai_xe"] = xe.TaiXe, ["bien_so"] = xe.BienSo, ["mau"] = xe.Mau,
                    ["phut"] = moi.Phut, ["cuoc"] = moi.Cuoc,
                };
                return (kq, new() { [$"xe{xe.Ma}"] = moi });
            }

            Kho.ThucHien(cacO, XuLy, traLoi, kq => DaGhi("dat", kq, that));
        }
        else
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)
            {
                foreach (var x in doc.Values.Cast<Xe>())
                    if (x.Khach == nguoi)
                    {
                        var moi = x with { Khach = null, Don = null, Den = null, Gio = null, Cuoc = null, Phut = null };
                        return (new JsonObject { ["ok"] = true, ["xe"] = x.Ma, ["khach"] = nguoi, ["tai_xe"] = x.TaiXe },
                                new() { [$"xe{x.Ma}"] = moi });
                    }
                return (new JsonObject { ["ok"] = false, ["ma_loi"] = "chua_dat", ["ly_do"] = "bạn chưa đặt xe" }, null);
            }

            Kho.ThucHien(cacO, XuLy, traLoi, kq => DaGhi("huy", kq, that));
        }
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự lần
    /// ghi; cap_nhat mang "phien" lúc chụp -> client biết một cap_nhat chụp trước hay sau lần ghi của mình.</summary>
    void DaGhi(string loai, JsonObject kq, bool that)
    {
        long p = Interlocked.Increment(ref demGhi);
        Volatile.Write(ref phien, p);
        kq["phien"] = p;
        long xe = Chung.SoL(kq, "xe");
        string khach = Chung.Chuoi(kq, "khach")!, taiXe = Chung.Chuoi(kq, "tai_xe")!;
        So.Ghi(new GdDatXe(loai, xe, khach));
        lock (khoaLs)
        {
            lichSu.AddFirst(new JsonObject { ["gio"] = DateTime.Now.ToString("HH:mm:ss"), ["loai"] = loai, ["xe"] = xe, ["tai_xe"] = taiXe, ["nguoi"] = khach });
            while (lichSu.Count > 30) lichSu.RemoveLast();
        }
        if (that)
            ThongBao(loai == "dat" ? $"Xe {xe} ({taiXe}) nhận khách {khach}" : $"{khach} hủy chuyến, xe {xe} trống");
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        long ph = Volatile.Read(ref phien);              // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        var ds = Kho.AnhChup().Values.OfType<Xe>().ToList();
        JsonArray ls;
        lock (khoaLs) ls = new JsonArray([.. lichSu.Take(8).Select(x => (JsonNode)x.DeepClone())]);
        return new JsonObject
        {
            ["phien"] = ph, ["so_xe"] = ds.Count, ["trong"] = ds.Count(x => x.Khach == null),
            ["xe"] = new JsonArray([.. ds.Select(x => (JsonNode)x.Json())]), ["lich_su"] = ls,
        };
    }

    protected override string TomTat()
    {
        var ds = Kho.AnhChup().Values.OfType<Xe>().ToList();
        return $"Đã đặt {ds.Count(x => x.Khach != null)}/{ds.Count} xe, {SoXe.SoDat} lượt báo đặt được";
    }
}
