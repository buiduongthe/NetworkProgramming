using System.Collections.Concurrent;
using System.Text.Json.Nodes;

namespace BaiMR6_14;

// Bài mở rộng MR6.14 – Lõi máy chủ đặt vé xe khách theo chặng: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối) và MR6.8.
// Chuyến xe qua các trạm Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang (3 chặng). Hai loại xe:
//     giuong – xe giường nằm 2 tầng, 40 giường: tầng dưới D01–D20, tầng trên T01–T20 (3 dãy × 7 hàng, hàng cuối 2 giường);
//     ghe    – xe ghế ngồi 50 chỗ 01–50 (11 hàng × 4 ghế, lối đi ở giữa, hàng cuối 6 ghế liền).
// Khách đặt một ĐOẠN [tu, den) (chỉ số trạm, tu < den), nên cùng một giường bán được cho nhiều người nếu các đoạn
// không chồng nhau. Ba thao tác ghi:
//     {"loai":"giu_cho","ma_yc":k,"ghe":["D01"],"tu":0,"den":2[,"nguoi":"u17"]}   giữ chỗ có thời hạn (TTL)
//     {"loai":"thanh_toan","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                 chỗ mình đang giữ -> vé
//     {"loai":"nha","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                        trả lại chỗ đang giữ
// Mỗi cặp (ghế, chặng) là MỘT ô của KhoTranhChap, tên ô "D01#1" (chặng 1 = Bảo Lộc → Đà Lạt), giá trị là record bất
// biến ChoChang(Tt, Nguoi, HetHan, MaVe, NguoiHetHan), Tt = "trong" | "giu" | "ban", HetHan theo đồng hồ đơn điệu.
// Giữ 2 giường cho đoạn 3 chặng = khóa 6 ô theo thứ tự tên ô (không khóa chết). Luật như MR6.8: giữ được khi MỌI ô
// đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); hết hạn sau ttl_s giây – luồng nền dọn ô quá hạn mỗi
// 100 ms; thanh toán chỉ nhận ô mình giữ, còn hạn.
//
// Sổ kiểm toán đếm hai sai phạm:
//   trung_cho – một (ghế, chặng) được giữ/bán cho người thứ hai khi người khác đang giữ (còn hạn) hoặc đã mua;
//   tt_sai    – thanh toán được chấp nhận cho chỗ không do mình giữ hoặc lượt giữ đã hết hạn.

/// <summary>Một chỗ trên sơ đồ xe.</summary>
sealed record ChoXe(string Ma, int Tang, int Hang, int Cot);

/// <summary>Tuyến, giá, sơ đồ hai loại xe.</summary>
static class XeKhach
{
    public static readonly string[] Tram = ["Sài Gòn", "Bảo Lộc", "Đà Lạt", "Nha Trang"];
    public static readonly string[] MaTram = ["SG", "BL", "DL", "NT"];
    public static readonly int SoChang = Tram.Length - 1;
    public static readonly Dictionary<string, long[]> GiaChang = new()     // giá minh họa mỗi chặng, đồng/chỗ
    {
        ["giuong"] = [220_000, 130_000, 170_000], ["ghe"] = [170_000, 100_000, 130_000],
    };
    public static readonly Dictionary<string, string> TenXe = new() { ["giuong"] = "Xe giường nằm 40 chỗ", ["ghe"] = "Xe ghế ngồi 50 chỗ" };
    public const int ToiDaGhe = 6;                       // một yêu cầu tối đa 6 chỗ

    /// <summary>Sơ đồ xe theo thứ tự hiển thị. giuong: tầng 0 = dưới (D), 1 = trên (T); cột 0, 1, 2; hàng 6 chỉ có cột
    /// 0 và 2. ghe: tầng 0; 6 cột: hàng 0–10 dùng cột 0, 1 | lối đi | 4, 5; hàng cuối (11) đủ 6 cột.</summary>
    public static List<ChoXe> BoTri(string loaiXe)
    {
        var ds = new List<ChoXe>();
        if (loaiXe == "giuong")
        {
            for (int tang = 0; tang < 2; tang++)
            {
                int n = 0;
                for (int hang = 0; hang < 7; hang++)
                    foreach (int cot in hang < 6 ? new[] { 0, 1, 2 } : new[] { 0, 2 })
                        ds.Add(new ChoXe($"{(tang == 0 ? "D" : "T")}{++n:00}", tang, hang, cot));
            }
        }
        else
        {
            int n = 0;
            for (int hang = 0; hang < 12; hang++)
                foreach (int cot in hang < 11 ? new[] { 0, 1, 4, 5 } : new[] { 0, 1, 2, 3, 4, 5 })
                    ds.Add(new ChoXe($"{++n:00}", 0, hang, cot));
        }
        return ds;
    }

    public static List<string> DsMaGhe(string loaiXe) => [.. BoTri(loaiXe).Select(x => x.Ma)];

    public static string TenChang(int k) => $"{Tram[k]}–{Tram[k + 1]}";

    public static string TenDoan(int tu, int den) => $"{Tram[tu]} → {Tram[den]}";

    public static string TenO(string o)
    {
        var p = o.Split('#');
        return $"{p[0]} ({TenChang(int.Parse(p[1], Chung.Ic))})";
    }

    public static List<string> OCua(IEnumerable<string> ghe, int tu, int den) =>
        [.. ghe.SelectMany(g => Enumerable.Range(tu, den - tu).Select(k => $"{g}#{k}"))];

    public static JsonArray Json(IEnumerable<string> ds) => new([.. ds.Select(g => (JsonNode)JsonValue.Create(g))]);
}

/// <summary>Một (ghế, chặng) – giá trị của ô "D01#1".</summary>
sealed record ChoChang(string Tt, string? Nguoi, double? HetHan, string? MaVe, string? NguoiHetHan)
{
    public static ChoChang Trong(string? lanCuoi = null) => new("trong", null, null, null, lanCuoi);
}

sealed record GdDatVeXe(string Loai, List<string> O, string Nguoi, double? Luc = null, double? HetHan = null, long SoVe = 0);

sealed class SoKiemToanDatVeXe : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("trung_cho", "trùng chỗ"), ("tt_sai", "thanh toán sai")];
    readonly Dictionary<string, (string Tt, string Nguoi, double? HetHan)> o = [];     // theo sổ: ô -> (tt, nguoi, het_han)
    long soVe;                                                                          // số vé (ghế × đoạn) đã bán theo sổ

    public long SoVe => Interlocked.Read(ref soVe);

    protected override void KiemTra(object x)
    {
        var gd = (GdDatVeXe)x;
        string nguoi = gd.Nguoi;
        if (gd.Loai == "ban") Interlocked.Add(ref soVe, gd.SoVe);
        foreach (var oo in gd.O)
        {
            bool co = o.TryGetValue(oo, out var cu);
            if (gd.Loai == "giu")
            {
                if (co && cu.Tt == "ban")
                {
                    SaiPham("trung_cho", $"{XeKhach.TenO(oo)} giữ cho {nguoi} khi đã bán cho {cu.Nguoi}");
                    continue;
                }
                if (co && cu.Nguoi != nguoi && gd.Luc < cu.HetHan)
                    SaiPham("trung_cho", $"{XeKhach.TenO(oo)} giữ cho {nguoi} khi {cu.Nguoi} đang giữ");
                o[oo] = ("giu", nguoi, gd.HetHan);
            }
            else if (gd.Loai == "ban")
            {
                if (co && cu.Tt == "ban")
                {
                    SaiPham("trung_cho", $"{XeKhach.TenO(oo)} bán cho {nguoi} khi đã bán cho {cu.Nguoi}");
                    continue;
                }
                if (!co || cu.Nguoi != nguoi) SaiPham("tt_sai", $"{XeKhach.TenO(oo)}: {nguoi} trả tiền chỗ không do mình giữ");
                else if (gd.Luc >= cu.HetHan) SaiPham("tt_sai", $"{XeKhach.TenO(oo)}: {nguoi} trả tiền khi đã hết hạn giữ");
                o[oo] = ("ban", nguoi, null);
            }
            else if (co && cu.Tt == "giu" && cu.Nguoi == nguoi) o.Remove(oo);         // nhả / hết hạn
        }
    }
}

sealed class MayChuDatVeXe(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                           IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new()
    {
        ["ttl_s"] = 30.0, ["loai_xe"] = "giuong", ["ghe_nong"] = "D01", ["chuyen"] = "07:30", ["nha_xe"] = "Xe khách Cao Nguyên",
    };
    public override string Ten => "Đặt vé xe khách MR6.14";
    public override string[] LoaiYeuCau => ["giu_cho", "thanh_toan", "nha"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    // các trường dưới đây do KhoiTao() đặt (gọi từ hàm tạo của lớp cha và mỗi lần đặt lại)
    volatile List<ChoXe> boTri = [];
    volatile List<string> dsGhe = [];
    volatile HashSet<string> gheHopLe = [];
    volatile List<string> tatCaO = [];
    ConcurrentDictionary<string, byte> dangDon = new();           // ô quá hạn đang được luồng nền dọn
    ConcurrentDictionary<string, byte> nguoiThat = new();         // tên người dùng thật (in nhật ký cho họ)
    long demGhi;                                                  // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    long phien;

    SoKiemToanDatVeXe SoVe => (SoKiemToanDatVeXe)So;

    /// <summary>Danh sách mã chỗ của loại xe hiện tại (theo thứ tự sơ đồ).</summary>
    public IReadOnlyList<string> DsGhe => dsGhe;

    // ------------------------------------------------------------ tạo / đặt lại
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((double)ts.GetValueOrDefault("ttl_s", 30.0) is not (>= 1 and <= 600)) throw new ArgumentException("thời hạn giữ chỗ 1–600 s");
        string loai = (string)ts.GetValueOrDefault("loai_xe", ThamSo["loai_xe"]);
        if (!XeKhach.GiaChang.ContainsKey(loai)) throw new ArgumentException("loại xe: giuong hoặc ghe");
        string nong = (string)ts.GetValueOrDefault("ghe_nong", "");
        if (nong != "" && !XeKhach.DsMaGhe(loai).Contains(nong))
            throw new ArgumentException($"ghế nóng phải là mã chỗ của {XeKhach.TenXe[loai].ToLowerInvariant()} hoặc rỗng");
        if (ts.TryGetValue("nha_xe", out var p) && new System.Globalization.StringInfo((string)p).LengthInTextElements is < 1 or > 40)
            throw new ArgumentException("tên nhà xe 1–40 ký tự");
    }

    /// <summary>Ghế nóng đang dùng: tham số ghe_nong nếu là chỗ của loại xe hiện tại, không thì chỗ đầu (D01 / 01) – đổi
    /// loại xe mà không đặt ghe_nong thì ghế nóng tự về chỗ đầu; rỗng = không có ghế nóng.</summary>
    public string GheNong()
    {
        string g = TsS("ghe_nong");
        return g == "" || gheHopLe.Contains(g) ? g : dsGhe[0];
    }

    protected override void KhoiTao()
    {
        var bt = XeKhach.BoTri(TsS("loai_xe"));
        var ds = bt.Select(x => x.Ma).ToList();
        var tatCa = XeKhach.OCua(ds, 0, XeKhach.SoChang);
        foreach (var o in tatCa) Kho.DatO(o, ChoChang.Trong());
        boTri = bt;
        dsGhe = ds;
        gheHopLe = [.. ds];
        tatCaO = tatCa;
        So = new SoKiemToanDatVeXe();
        dangDon = new();
        nguoiThat = new();
    }

    /// <summary>Đọc ô để hiển thị; ô chưa có (đang dựng lại kho cho loại xe khác) -> null.</summary>
    ChoChang? DocO(string o)
    {
        try
        {
            return (ChoChang)Kho.Doc(o);
        }
        catch (KeyNotFoundException)
        {
            return null;
        }
    }

    // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    /// <summary>Danh sách chỗ hợp lệ (1–6 mã của loại xe hiện tại, không trùng) hoặc null.</summary>
    List<string>? DocGhe(JsonNode? x)
    {
        if (x is not JsonArray a || a.Count is < 1 or > XeKhach.ToiDaGhe) return null;
        var ds = new List<string>();
        foreach (var g in a)
        {
            if (g is not JsonValue v || v.GetValueKind() != System.Text.Json.JsonValueKind.String) return null;
            ds.Add(v.GetValue<string>());
        }
        var hl = gheHopLe;
        return ds.Distinct().Count() == ds.Count && ds.All(hl.Contains) ? ds : null;
    }

    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        var ghe = DocGhe(tin["ghe"]);
        if (ghe == null)
        {
            traLoi(new JsonObject
            {
                ["ok"] = false, ["ma_loi"] = "sai_du_lieu",
                ["ly_do"] = $"ghế: danh sách 1–{XeKhach.ToiDaGhe} mã chỗ của {XeKhach.TenXe[TsS("loai_xe")].ToLowerInvariant()}",
            });
            return;
        }
        if (!Chung.LaSoNguyen(tin["tu"], out long a) || !Chung.LaSoNguyen(tin["den"], out long b) || a < 0 || a >= b || b > XeKhach.SoChang)
        {
            traLoi(new JsonObject
            {
                ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = $"đoạn đường: 0 ≤ tu < den ≤ {XeKhach.SoChang} (chỉ số trạm)",
            });
            return;
        }
        int tu = (int)a, den = (int)b;
        var oYC = XeKhach.OCua(ghe, tu, den);
        bool that = khach.SoNguoi == null;
        if (that) nguoiThat[nguoi] = 0;
        double ttl = TsD("ttl_s");
        string chuyen = TsS("chuyen");
        long gia = XeKhach.GiaChang[TsS("loai_xe")][tu..den].Sum() * ghe.Count;

        void Tra(JsonObject kq)                          // bỏ các trường nội bộ ("_…") trước khi gửi
        {
            foreach (var k in kq.Select(x => x.Key).Where(k => k.StartsWith('_')).ToList()) kq.Remove(k);
            traLoi(kq);
        }

        static (string G, int K) Tach(string o)
        {
            var p = o.Split('#');
            return (p[0], int.Parse(p[1], Chung.Ic));
        }

        string loai = Chung.Chuoi(tin, "loai")!;
        if (loai == "giu_cho")
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN: mọi ô phải trống / quá hạn / của mình
            {
                double luc = Chung.Now;
                var moi = new List<string>();
                double conLai = ttl;
                foreach (var o in oYC)
                {
                    var v = (ChoChang)doc[o];
                    var (g, k) = Tach(o);
                    if (v.Tt == "ban")
                        return (new JsonObject
                        {
                            ["ok"] = false, ["ma_loi"] = "da_ban", ["ly_do"] = $"chỗ {g} chặng {XeKhach.TenChang(k)} đã bán",
                            ["ghe_ban"] = XeKhach.Json([g]), ["tu"] = tu, ["den"] = den,
                        }, null);
                    if (v.Tt == "giu" && luc < v.HetHan)
                    {
                        if (v.Nguoi != nguoi)
                            return (new JsonObject
                            {
                                ["ok"] = false, ["ma_loi"] = "dang_giu", ["ly_do"] = $"chỗ {g} chặng {XeKhach.TenChang(k)} đang có người giữ",
                                ["ghe_ban"] = XeKhach.Json([g]), ["tu"] = tu, ["den"] = den,
                            }, null);
                        conLai = Math.Min(conLai, v.HetHan!.Value - luc);
                    }
                    else moi.Add(o);
                }
                double hetHan = luc + ttl;
                var ghi = moi.ToDictionary(o => o, _ => (object)new ChoChang("giu", nguoi, hetHan, null, null));
                return (new JsonObject
                {
                    ["ok"] = true, ["ghe"] = XeKhach.Json(ghe), ["tu"] = tu, ["den"] = den, ["nguoi"] = nguoi,
                    ["het_han_ms"] = (long)Math.Round(conLai * 1000), ["tien"] = gia, ["_moi"] = XeKhach.Json(moi), ["_luc"] = luc,
                    ["_het_han"] = hetHan,
                }, ghi.Count > 0 ? ghi : null);
            }

            Kho.ThucHien(oYC, XuLy, Tra, kq => DaGiu(kq, that));
        }
        else if (loai == "thanh_toan")
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)
            {
                double luc = Chung.Now;
                string? hetHan = null, khongGiu = null;  // ưu tiên báo "hết hạn" nếu có dấu vết lượt giữ của mình
                foreach (var o in oYC)
                {
                    var v = (ChoChang)doc[o];
                    var (g, _) = Tach(o);
                    bool cuaToi = v.Tt == "giu" && v.Nguoi == nguoi;
                    if (cuaToi && luc < v.HetHan) continue;
                    if (cuaToi || (v.Tt == "trong" && v.NguoiHetHan == nguoi)) hetHan ??= g;
                    else khongGiu ??= g;
                }
                if (hetHan != null)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_han", ["ly_do"] = $"giữ chỗ {hetHan} đã hết hạn" }, null);
                if (khongGiu != null)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_giu", ["ly_do"] = $"chỗ {khongGiu} không do bạn giữ" }, null);
                string ma = "XK" + chuyen.Replace(":", "");
                var maVe = ghe.ToDictionary(g => g, g => $"{ma}-{g}-{XeKhach.MaTram[tu]}{XeKhach.MaTram[den]}");
                var ghi = oYC.ToDictionary(o => o, o => (object)new ChoChang("ban", nguoi, null, maVe[Tach(o).G], null));
                return (new JsonObject
                {
                    ["ok"] = true, ["ghe"] = XeKhach.Json(ghe), ["tu"] = tu, ["den"] = den, ["nguoi"] = nguoi,
                    ["ma_ve"] = XeKhach.Json(ghe.Select(g => maVe[g])), ["tien"] = gia, ["_luc"] = luc,
                }, ghi);
            }

            Kho.ThucHien(oYC, XuLy, Tra, kq => DaBan(kq, that));
        }
        else
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)
            {
                var nha = oYC.Where(o => doc[o] is ChoChang { Tt: "giu" } v && v.Nguoi == nguoi).ToList();
                if (nha.Count == 0)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_giu", ["ly_do"] = "bạn không giữ chỗ nào trong số này" }, null);
                return (new JsonObject
                {
                    ["ok"] = true, ["ghe"] = XeKhach.Json(ghe), ["tu"] = tu, ["den"] = den, ["nguoi"] = nguoi, ["_o"] = XeKhach.Json(nha),
                }, nha.ToDictionary(o => o, _ => (object)ChoChang.Trong()));
            }

            Kho.ThucHien(oYC, XuLy, Tra, kq => DaNha(kq, that));
        }
    }

    // khiGhi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ tự lần
    // ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
    void DanhSo(JsonObject kq)
    {
        long p = Interlocked.Increment(ref demGhi);
        Volatile.Write(ref phien, p);
        kq["phien"] = p;
    }

    static List<string> Ds(JsonObject kq, string k) => [.. (kq[k] as JsonArray ?? []).Select(Chung.ChuoiPy)];

    static string Doan(JsonObject kq) => XeKhach.TenDoan((int)Chung.SoL(kq, "tu"), (int)Chung.SoL(kq, "den"));

    void DaGiu(JsonObject kq, bool that)
    {
        DanhSo(kq);
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVeXe("giu", Ds(kq, "_moi"), nguoi, Chung.SoD(kq, "_luc"), Chung.SoD(kq, "_het_han")));
        if (that) ThongBao($"{nguoi} giữ {string.Join(", ", Ds(kq, "ghe"))} ({Doan(kq)}, {Chung.G(TsD("ttl_s"))} s)");
        BaoDoi();
    }

    void DaBan(JsonObject kq, bool that)
    {
        DanhSo(kq);
        var ghe = Ds(kq, "ghe");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        var o = XeKhach.OCua(ghe, (int)Chung.SoL(kq, "tu"), (int)Chung.SoL(kq, "den"));
        So.Ghi(new GdDatVeXe("ban", o, nguoi, Chung.SoD(kq, "_luc"), SoVe: ghe.Count));
        if (that) ThongBao($"{nguoi} mua {ghe.Count} vé {string.Join(", ", ghe)} {Doan(kq)} ({Chung.Tien(Chung.SoL(kq, "tien"))})");
        BaoDoi();
    }

    void DaNha(JsonObject kq, bool that)
    {
        DanhSo(kq);
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVeXe("nha", Ds(kq, "_o"), nguoi));
        if (that) ThongBao($"{nguoi} nhả {string.Join(", ", Ds(kq, "ghe"))} ({Doan(kq)})");
        BaoDoi();
    }

    // ------------------------------------------------------------ dọn ô quá hạn (luồng nền, 100 ms)
    protected override void DinhKy(double bayGio)
    {
        var dd = dangDon;
        foreach (var o in tatCaO)
        {
            var v = DocO(o);
            if (v == null || v.Tt != "giu" || bayGio < v.HetHan || !dd.TryAdd(o, 0)) continue;

            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // nhả ô quá hạn cũng là một thao tác ghi
            {
                var x = (ChoChang)doc[o];
                if (x.Tt != "giu" || Chung.Now < x.HetHan) return (new JsonObject { ["ok"] = false }, null);   // vừa được thanh toán / giữ lại
                return (new JsonObject { ["ok"] = true, ["o"] = XeKhach.Json([o]), ["nguoi"] = x.Nguoi },
                        new() { [o] = ChoChang.Trong(x.Nguoi) });
            }

            Kho.ThucHien([o], XuLy, _ => dd.TryRemove(o, out byte _), DaHetHan);
        }
    }

    void DaHetHan(JsonObject kq)
    {
        DanhSo(kq);
        var o = Ds(kq, "o");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVeXe("het_han", o, nguoi));
        if (nguoiThat.ContainsKey(nguoi)) ThongBao($"Hết hạn giữ {XeKhach.TenO(o[0])} ({nguoi}), chỗ trống lại");
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    /// <summary>so_do[i] = chuỗi SoChang ký tự cho chỗ ghe[i]: '.' trống, 'g' đang giữ, 'b' đã bán (từng chặng);
    /// chu / con_lai_ms theo tên ô "D01#1".</summary>
    protected override JsonObject TrangThaiNghiepVu()
    {
        long ph = Volatile.Read(ref phien);              // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        double bayGio = Chung.Now;
        var bt = boTri;
        var soDo = new JsonArray();
        var chu = new JsonObject();
        var conLai = new JsonObject();
        long trong = 0, giu = 0, ban = 0;
        foreach (var x in bt)
        {
            var dong = new System.Text.StringBuilder();
            for (int k = 0; k < XeKhach.SoChang; k++)
            {
                string o = $"{x.Ma}#{k}";
                var v = DocO(o);
                if (v is { Tt: "giu" } && bayGio < v.HetHan)
                {
                    dong.Append('g');
                    chu[o] = v.Nguoi;
                    conLai[o] = (long)Math.Round((v.HetHan!.Value - bayGio) * 1000);
                    giu++;
                }
                else if (v is { Tt: "ban" })
                {
                    dong.Append('b');
                    chu[o] = v.Nguoi;
                    ban++;
                }
                else
                {
                    dong.Append('.');
                    trong++;
                }
            }
            soDo.Add(dong.ToString());
        }
        string loai = TsS("loai_xe");
        return new JsonObject
        {
            ["phien"] = ph, ["nha_xe"] = TsS("nha_xe"), ["chuyen"] = TsS("chuyen"), ["loai_xe"] = loai, ["ten_xe"] = XeKhach.TenXe[loai],
            ["tram"] = XeKhach.Json(XeKhach.Tram), ["gia_chang"] = new JsonArray([.. XeKhach.GiaChang[loai].Select(g => (JsonNode)g)]),
            ["ttl_s"] = TsD("ttl_s"), ["ghe_nong"] = GheNong(), ["ghe"] = XeKhach.Json(bt.Select(x => x.Ma)),
            ["bo_tri"] = new JsonArray([.. bt.Select(x => (JsonNode)new JsonArray(x.Tang, x.Hang, x.Cot))]),
            ["so_do"] = soDo, ["chu"] = chu, ["con_lai_ms"] = conLai,
            ["dem"] = new JsonObject { ["trong"] = trong, ["giu"] = giu, ["ban"] = ban },
        };
    }

    protected override string TomTat()
    {
        long ban = 0, giu = 0;
        double bayGio = Chung.Now;
        var tatCa = tatCaO;
        foreach (var o in tatCa)
        {
            var v = DocO(o);
            if (v is { Tt: "ban" }) ban++;
            if (v is { Tt: "giu" } && bayGio < v.HetHan) giu++;
        }
        return $"Bán {ban}/{tatCa.Count} chỗ-chặng, giữ {giu}, sổ {SoVe.SoVe} vé";
    }
}
