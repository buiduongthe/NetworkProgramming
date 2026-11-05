using System.Collections.Concurrent;
using System.Text.Json.Nodes;

namespace BaiMR6_8;

// Bài mở rộng MR6.8 – Lõi máy chủ đặt vé xem phim: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// Phòng chiếu 8 hàng (A–H) × 12 ghế; n người cùng chọn ghế. Ba thao tác ghi:
//     {"loai":"giu_cho","ma_yc":k,"ghe":["E7","E8"][,"nguoi":"u17"]}    giữ chỗ có thời hạn (TTL)
//     {"loai":"thanh_toan","ma_yc":k,"ghe":["E7","E8"]}                  ghế mình đang giữ, còn hạn -> vé
//     {"loai":"nha","ma_yc":k,"ghe":["E7"]}                              trả lại ghế đang giữ
// Luật: giữ được khi MỌI ghế trong yêu cầu đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); lượt giữ
// hết hạn sau ttl_s giây (mặc định 30 s) – luồng nền của máy chủ (100 ms/lần) "dọn" ghế quá hạn, và một lượt giữ quá
// hạn cũng bị coi là trống ngay cả khi chưa kịp dọn; thanh toán chỉ nhận ghế mình đang giữ và còn hạn (quá hạn ->
// "het_han"). Mỗi ghế là MỘT ô của KhoTranhChap: record bất biến Ghe(Tt, Nguoi, HetHan, MaVe, NguoiHetHan) với
// Tt = "trong" | "giu" | "ban", HetHan theo đồng hồ đơn điệu – nên khóa theo từng ghế: giữ nhiều ghế = khóa nhiều ô
// theo thứ tự tên ô (không khóa chết).
//
// Sổ kiểm toán đếm hai sai phạm:
//   trung_ghe – một ghế được giữ/bán cho người thứ hai khi đang được người khác giữ (còn hạn) hoặc đã bán;
//   tt_sai    – thanh toán được chấp nhận cho ghế không do mình giữ hoặc lượt giữ đã hết hạn.

/// <summary>Sơ đồ phòng chiếu.</summary>
static class PhongChieu
{
    public const string Hang = "ABCDEFGH";
    public const int Cot = 12;
    public static readonly string[] TatCaGhe = [.. Hang.SelectMany(r => Enumerable.Range(1, Cot).Select(c => $"{r}{c}"))];
    public static readonly HashSet<string> GheHopLe = [.. TatCaGhe];
    public const int ToiDaGhe = 8;                       // một yêu cầu tối đa 8 ghế

    /// <summary>Kiểm tra danh sách ghế: 1–8 mã ghế hợp lệ, không trùng; trả về danh sách hoặc null.</summary>
    public static List<string>? DsGhe(JsonNode? x)
    {
        if (x is not JsonArray a || a.Count is < 1 or > ToiDaGhe) return null;
        var ds = new List<string>();
        foreach (var g in a)
        {
            if (g is not JsonValue v || v.GetValueKind() != System.Text.Json.JsonValueKind.String) return null;
            ds.Add(v.GetValue<string>());
        }
        return ds.Distinct().Count() == ds.Count && ds.All(GheHopLe.Contains) ? ds : null;
    }

    public static JsonArray Json(IEnumerable<string> ds) => new([.. ds.Select(g => (JsonNode)JsonValue.Create(g))]);
}

/// <summary>Một ghế (giá trị của ô "E7"…).</summary>
sealed record Ghe(string Tt, string? Nguoi, double? HetHan, string? MaVe, string? NguoiHetHan)
{
    public static Ghe Trong(string? lanCuoi = null) => new("trong", null, null, null, lanCuoi);
}

sealed record GdDatVe(string Loai, List<string> Ghe, string Nguoi, double? Luc = null, double? HetHan = null);

sealed class SoKiemToanDatVe : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("trung_ghe", "ghế trùng"), ("tt_sai", "thanh toán sai")];
    readonly Dictionary<string, (string Tt, string Nguoi, double? HetHan)> ghe = [];   // theo sổ
    long soVe;                                                                          // số vé đã bán theo sổ

    public long SoVe => Interlocked.Read(ref soVe);

    protected override void KiemTra(object o)
    {
        var gd = (GdDatVe)o;
        string nguoi = gd.Nguoi;
        foreach (var g in gd.Ghe)
        {
            bool co = ghe.TryGetValue(g, out var cu);
            if (gd.Loai == "giu")
            {
                if (co && cu.Tt == "ban")
                {
                    SaiPham("trung_ghe", $"{g} giữ cho {nguoi} khi đã bán cho {cu.Nguoi}");
                    continue;
                }
                if (co && cu.Nguoi != nguoi && gd.Luc < cu.HetHan)
                    SaiPham("trung_ghe", $"{g} giữ cho {nguoi} khi {cu.Nguoi} đang giữ");
                ghe[g] = ("giu", nguoi, gd.HetHan);
            }
            else if (gd.Loai == "ban")
            {
                Interlocked.Increment(ref soVe);
                if (co && cu.Tt == "ban")
                {
                    SaiPham("trung_ghe", $"{g} bán cho {nguoi} khi đã bán cho {cu.Nguoi}");
                    continue;
                }
                if (!co || cu.Nguoi != nguoi) SaiPham("tt_sai", $"{g}: {nguoi} trả tiền ghế không do mình giữ");
                else if (gd.Luc >= cu.HetHan) SaiPham("tt_sai", $"{g}: {nguoi} trả tiền khi đã hết hạn giữ");
                ghe[g] = ("ban", nguoi, null);
            }
            else if (co && cu.Tt == "giu" && cu.Nguoi == nguoi) ghe.Remove(g);          // nhả / hết hạn
        }
    }
}

sealed class MayChuDatVe(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                         IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new()
    {
        ["ttl_s"] = 30.0, ["ghe_nong"] = "E7", ["gia_ve"] = 90_000L, ["phim"] = "Hành tinh xanh", ["suat"] = "19:30",
    };
    public override string Ten => "Đặt vé xem phim MR6.8";
    public override string[] LoaiYeuCau => ["giu_cho", "thanh_toan", "nha"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    ConcurrentDictionary<string, byte> dangDon = new();           // ghế quá hạn đang được luồng nền dọn
    ConcurrentDictionary<string, byte> nguoiThat = new();         // tên người dùng thật (in nhật ký cho họ)
    long demGhi;                                                  // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
    long phien;

    SoKiemToanDatVe SoVe => (SoKiemToanDatVe)So;

    // ------------------------------------------------------------ tạo / đặt lại
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((double)ts.GetValueOrDefault("ttl_s", 30.0) is not (>= 1 and <= 600)) throw new ArgumentException("thời hạn giữ chỗ 1–600 s");
        string nong = (string)ts.GetValueOrDefault("ghe_nong", "");
        if (nong != "" && !PhongChieu.GheHopLe.Contains(nong)) throw new ArgumentException("ghế nóng phải là mã ghế A1–H12 hoặc rỗng");
        if ((long)ts.GetValueOrDefault("gia_ve", 1L) <= 0) throw new ArgumentException("giá vé > 0");
        if (ts.TryGetValue("phim", out var p) && ((string)p).Length is < 1 or > 40) throw new ArgumentException("tên phim 1–40 ký tự");
    }

    protected override void KhoiTao()
    {
        foreach (var g in PhongChieu.TatCaGhe) Kho.DatO(g, Ghe.Trong());
        So = new SoKiemToanDatVe();
        dangDon = new();
        nguoiThat = new();
    }

    // ------------------------------------------------------------ giữ chỗ / thanh toán / nhả
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        var ghe = PhongChieu.DsGhe(tin["ghe"]);
        if (ghe == null)
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = $"ghế: danh sách 1–{PhongChieu.ToiDaGhe} mã A1–H12" });
            return;
        }
        bool that = khach.SoNguoi == null;
        if (that) nguoiThat[nguoi] = 0;
        double ttl = TsD("ttl_s");
        long gia = TsL("gia_ve");
        string suat = TsS("suat");

        void Tra(JsonObject kq)                          // bỏ các trường nội bộ ("_…") trước khi gửi
        {
            foreach (var k in kq.Select(x => x.Key).Where(k => k.StartsWith('_')).ToList()) kq.Remove(k);
            traLoi(kq);
        }

        string loai = Chung.Chuoi(tin, "loai")!;
        if (loai == "giu_cho")
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN: mọi ghế phải trống / quá hạn / của mình
            {
                double luc = Chung.Now;
                var moi = new List<string>();
                double conLai = ttl;
                foreach (var g in ghe)
                {
                    var v = (Ghe)doc[g];
                    if (v.Tt == "ban")
                        return (new JsonObject { ["ok"] = false, ["ma_loi"] = "da_ban", ["ly_do"] = $"ghế {g} đã bán", ["ghe_ban"] = PhongChieu.Json([g]) }, null);
                    if (v.Tt == "giu" && luc < v.HetHan)
                    {
                        if (v.Nguoi != nguoi)
                            return (new JsonObject
                            {
                                ["ok"] = false, ["ma_loi"] = "dang_giu", ["ly_do"] = $"ghế {g} đang có người giữ", ["ghe_ban"] = PhongChieu.Json([g]),
                            }, null);
                        conLai = Math.Min(conLai, v.HetHan!.Value - luc);
                    }
                    else moi.Add(g);
                }
                double hetHan = luc + ttl;
                var ghi = moi.ToDictionary(g => g, g => (object)new Ghe("giu", nguoi, hetHan, null, null));
                return (new JsonObject
                {
                    ["ok"] = true, ["ghe"] = PhongChieu.Json(ghe), ["moi"] = PhongChieu.Json(moi), ["het_han_ms"] = (long)Math.Round(conLai * 1000),
                    ["nguoi"] = nguoi, ["_luc"] = luc, ["_het_han"] = hetHan,
                }, ghi.Count > 0 ? ghi : null);
            }

            Kho.ThucHien(ghe, XuLy, Tra, kq => DaGiu(kq, that));
        }
        else if (loai == "thanh_toan")
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)
            {
                double luc = Chung.Now;
                foreach (var g in ghe)
                {
                    var v = (Ghe)doc[g];
                    if (v.Tt == "giu" && v.Nguoi == nguoi && luc < v.HetHan) continue;
                    if ((v.Tt == "giu" && v.Nguoi == nguoi) || (v.Tt == "trong" && v.NguoiHetHan == nguoi))
                        return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_han", ["ly_do"] = $"giữ chỗ {g} đã hết hạn" }, null);
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_giu", ["ly_do"] = $"ghế {g} không do bạn giữ" }, null);
                }
                var ve = ghe.Select(g => $"S{suat.Replace(":", "")}-{g}").ToList();
                var ghi = new Dictionary<string, object>();
                for (int i = 0; i < ghe.Count; i++) ghi[ghe[i]] = new Ghe("ban", nguoi, null, ve[i], null);
                return (new JsonObject
                {
                    ["ok"] = true, ["ghe"] = PhongChieu.Json(ghe), ["ma_ve"] = PhongChieu.Json(ve), ["tien"] = gia * ghe.Count,
                    ["nguoi"] = nguoi, ["_luc"] = luc,
                }, ghi);
            }

            Kho.ThucHien(ghe, XuLy, Tra, kq => DaBan(kq, that));
        }
        else
        {
            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)
            {
                var nha = ghe.Where(g => doc[g] is Ghe { Tt: "giu" } v && v.Nguoi == nguoi).ToList();
                if (nha.Count == 0)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_giu", ["ly_do"] = "bạn không giữ ghế nào trong số này" }, null);
                return (new JsonObject { ["ok"] = true, ["ghe"] = PhongChieu.Json(nha), ["nguoi"] = nguoi },
                        nha.ToDictionary(g => g, _ => (object)Ghe.Trong()));
            }

            Kho.ThucHien(ghe, XuLy, Tra, kq => DaNha(kq, that));
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

    void DaGiu(JsonObject kq, bool that)
    {
        DanhSo(kq);
        var moi = Ds(kq, "moi");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVe("giu", moi, nguoi, Chung.SoD(kq, "_luc"), Chung.SoD(kq, "_het_han")));
        if (that) ThongBao($"{nguoi} giữ {string.Join(", ", moi)} ({Chung.G(TsD("ttl_s"))} s)");
        BaoDoi();
    }

    void DaBan(JsonObject kq, bool that)
    {
        DanhSo(kq);
        var ghe = Ds(kq, "ghe");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVe("ban", ghe, nguoi, Chung.SoD(kq, "_luc")));
        if (that) ThongBao($"{nguoi} mua {ghe.Count} vé {string.Join(", ", ghe)} ({Chung.Tien(Chung.SoL(kq, "tien"))})");
        BaoDoi();
    }

    void DaNha(JsonObject kq, bool that)
    {
        DanhSo(kq);
        var ghe = Ds(kq, "ghe");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVe("nha", ghe, nguoi));
        if (that) ThongBao($"{nguoi} nhả {string.Join(", ", ghe)}");
        BaoDoi();
    }

    // ------------------------------------------------------------ dọn ghế quá hạn (luồng nền, 100 ms)
    protected override void DinhKy(double bayGio)
    {
        var dd = dangDon;
        foreach (var g in PhongChieu.TatCaGhe)
        {
            var v = (Ghe)Kho.Doc(g);
            if (v.Tt != "giu" || bayGio < v.HetHan || !dd.TryAdd(g, 0)) continue;

            (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // nhả ghế quá hạn cũng là một thao tác ghi
            {
                var x = (Ghe)doc[g];
                if (x.Tt != "giu" || Chung.Now < x.HetHan) return (new JsonObject { ["ok"] = false }, null);   // vừa được thanh toán / giữ lại
                return (new JsonObject { ["ok"] = true, ["ghe"] = PhongChieu.Json([g]), ["nguoi"] = x.Nguoi },
                        new() { [g] = Ghe.Trong(x.Nguoi) });
            }

            Kho.ThucHien([g], XuLy, _ => dd.TryRemove(g, out byte _), DaHetHan);
        }
    }

    void DaHetHan(JsonObject kq)
    {
        DanhSo(kq);
        var ghe = Ds(kq, "ghe");
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        So.Ghi(new GdDatVe("het_han", ghe, nguoi));
        if (nguoiThat.ContainsKey(nguoi)) ThongBao($"Hết hạn giữ {ghe[0]} ({nguoi}), ghế trống lại");
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        long ph = Volatile.Read(ref phien);              // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
        double bayGio = Chung.Now;
        var soDo = new JsonArray();
        var chu = new JsonObject();
        var conLai = new JsonObject();
        long trong = 0, giu = 0, ban = 0;
        foreach (char r in PhongChieu.Hang)
        {
            var dong = new System.Text.StringBuilder();
            for (int c = 1; c <= PhongChieu.Cot; c++)
            {
                string g = $"{r}{c}";
                var v = (Ghe)Kho.Doc(g);
                if (v.Tt == "giu" && bayGio < v.HetHan)
                {
                    dong.Append('g');
                    chu[g] = v.Nguoi;
                    conLai[g] = (long)Math.Round((v.HetHan!.Value - bayGio) * 1000);
                    giu++;
                }
                else if (v.Tt == "ban")
                {
                    dong.Append('b');
                    chu[g] = v.Nguoi;
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
        return new JsonObject
        {
            ["phien"] = ph, ["phim"] = TsS("phim"), ["suat"] = TsS("suat"), ["gia_ve"] = TsL("gia_ve"), ["ttl_s"] = TsD("ttl_s"),
            ["ghe_nong"] = TsS("ghe_nong"), ["hang"] = PhongChieu.Hang, ["cot"] = PhongChieu.Cot, ["so_do"] = soDo, ["chu"] = chu,
            ["con_lai_ms"] = conLai, ["dem"] = new JsonObject { ["trong"] = trong, ["giu"] = giu, ["ban"] = ban },
        };
    }

    protected override string TomTat()
    {
        long ban = 0, giu = 0;
        double bayGio = Chung.Now;
        foreach (var g in PhongChieu.TatCaGhe)
        {
            var v = (Ghe)Kho.Doc(g);
            if (v.Tt == "ban") ban++;
            if (v.Tt == "giu" && bayGio < v.HetHan) giu++;
        }
        return $"Đã bán {ban}/{PhongChieu.TatCaGhe.Length} ghế, đang giữ {giu}, sổ {SoVe.SoVe} vé";
    }
}
