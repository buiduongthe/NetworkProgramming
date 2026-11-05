using System.Collections.Immutable;
using System.Text.Json.Nodes;

namespace BaiMR6_10;

// Bài mở rộng MR6.10 – Lõi máy chủ đăng ký học phần: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// Cổng đăng ký học phần ngày mở đăng ký: vài lớp học phần (mặc định 3 lớp, sĩ số 50), hàng nghìn sinh viên cùng
// bấm "Đăng ký". Hai thao tác ghi:
//     {"loai":"dang_ky","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
//     {"loai":"huy","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
// Luật: lớp còn chỗ (số đã đăng ký < sĩ số) mới nhận; sinh viên đã có trong lớp thì không đăng ký lại
// ("da_dang_ky"); chỉ hủy được lớp mình đã đăng ký ("chua_dang_ky").
//
// Mỗi lớp là MỘT ô của KhoTranhChap: record LopHp(Sv = ImmutableHashSet tên sinh viên); ghi = tạo tập mới (XuLy
// thuần). Khóa theo tài nguyên: hai lớp khác nhau được xử lý song song; một lớp "hot" là điểm nóng.
//
// Sổ kiểm toán (danh sách lớp theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//   vuot_si_so – một lượt đăng ký được chấp nhận khi lớp (theo sổ) đã đủ sĩ số;
//   dk_trung   – một sinh viên được chấp nhận đăng ký một lớp mà sổ ghi là đã có tên (hai thiết bị cùng bấm, hoặc
//                bấm đúp, cùng đọc "chưa có tên").

/// <summary>Danh mục lớp học phần: mã lớp, tên học phần, giảng viên, lịch học, số tín chỉ.</summary>
static class DanhMuc
{
    public static readonly (string Ma, string Ten, string Gv, string Lich, int Tc)[] Lop =
    [
        ("123013-01", "Lập trình mạng", "TS. Lê Hoàng Nam", "T2, tiết 1–3, E301", 3),
        ("123005-01", "Quản trị mạng", "ThS. Phạm Thu Hà", "T3, tiết 4–6, E302", 3),
        ("006124-01", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T4, tiết 7–9, E303", 2),
        ("123013-02", "Lập trình mạng", "TS. Lê Hoàng Nam", "T5, tiết 1–3, E301", 3),
        ("123005-02", "Quản trị mạng", "ThS. Phạm Thu Hà", "T6, tiết 4–6, E302", 3),
        ("006124-02", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T7, tiết 7–9, E303", 2),
    ];
}

/// <summary>Ô "lop:&lt;mã&gt;": tập sinh viên đã đăng ký (bất biến – ghi là thay tập mới).</summary>
sealed record LopHp(ImmutableHashSet<string> Sv);

sealed record GdDangKy(string ThaoTac, string Nguoi, string MaLop);

sealed class SoKiemToanDangKy(long siSo) : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("vuot_si_so", "vượt sĩ số"), ("dk_trung", "ĐK trùng")];
    readonly Dictionary<string, HashSet<string>> lop = [];   // mã lớp -> sinh viên theo sổ

    protected override void KiemTra(object o)
    {
        var gd = (GdDangKy)o;
        if (!lop.TryGetValue(gd.MaLop, out var ds)) lop[gd.MaLop] = ds = [];
        if (gd.ThaoTac == "huy")
        {
            ds.Remove(gd.Nguoi);
            return;
        }
        if (ds.Contains(gd.Nguoi)) SaiPham("dk_trung", $"{gd.Nguoi} ĐK lại {gd.MaLop}");
        else if (ds.Count >= siSo) SaiPham("vuot_si_so", $"{gd.MaLop}: {gd.Nguoi} là SV thứ {ds.Count + 1}");
        ds.Add(gd.Nguoi);
    }

    public long TongSo()
    {
        lock (Khoa) return lop.Values.Sum(x => x.Count);
    }

    public long SoLop(string ma)
    {
        lock (Khoa) return lop.TryGetValue(ma, out var ds) ? ds.Count : 0;
    }
}

sealed class MayChuDangKy(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                          IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new() { ["so_lop"] = 3L, ["si_so"] = 50L };
    public override string Ten => "Đăng ký học phần MR6.10";
    public override string[] LoaiYeuCau => ["dang_ky", "huy"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    (string Ma, string Ten, string Gv, string Lich, int Tc)[] lop = [];

    SoKiemToanDangKy SoDk => (SoKiemToanDangKy)So;

    // ------------------------------------------------------------ tạo / đặt lại đợt đăng ký
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("so_lop", 1L) < 1 || (long)ts.GetValueOrDefault("so_lop", 1L) > DanhMuc.Lop.Length)
            throw new ArgumentException($"số lớp 1–{DanhMuc.Lop.Length}");
        if ((long)ts.GetValueOrDefault("si_so", 1L) is < 1 or > 500) throw new ArgumentException("sĩ số 1–500");
    }

    protected override void KhoiTao()
    {
        var moi = DanhMuc.Lop[..(int)TsL("so_lop")];
        foreach (var x in moi) Kho.DatO("lop:" + x.Ma, new LopHp([]));
        So = new SoKiemToanDangKy(TsL("si_so"));
        lop = moi;
    }

    // ------------------------------------------------------------ đăng ký / hủy
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        string? ma = Chung.Chuoi(tin, "ma_lop");
        if (ma == null || !lop.Any(x => x.Ma == ma))
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = $"không có lớp {Chung.ChuoiPy(tin["ma_lop"])}" });
            return;
        }
        string k = "lop:" + ma, thaoTac = Chung.Chuoi(tin, "loai")!;
        long siSo = TsL("si_so");

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN
        {
            var sv = ((LopHp)doc[k]).Sv;
            ImmutableHashSet<string> moi;
            if (thaoTac == "dang_ky")
            {
                if (sv.Contains(nguoi))
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "da_dang_ky", ["ly_do"] = "đã đăng ký lớp này", ["ma_lop"] = ma, ["da_dk"] = sv.Count }, null);
                if (sv.Count >= siSo)
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "lop_day", ["ly_do"] = "lớp đã đủ", ["ma_lop"] = ma, ["da_dk"] = sv.Count }, null);
                moi = sv.Add(nguoi);
            }
            else
            {
                if (!sv.Contains(nguoi))
                    return (new JsonObject { ["ok"] = false, ["ma_loi"] = "chua_dang_ky", ["ly_do"] = "chưa đăng ký lớp này", ["ma_lop"] = ma, ["da_dk"] = sv.Count }, null);
                moi = sv.Remove(nguoi);
            }
            return (new JsonObject { ["ok"] = true, ["thao_tac"] = thaoTac, ["ma_lop"] = ma, ["nguoi"] = nguoi, ["da_dk"] = moi.Count },
                    new() { [k] = new LopHp(moi) });
        }

        bool gop = khach.SoNguoi != null;
        Kho.ThucHien([k], XuLy, traLoi, kq => DaGhi(kq, gop));
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.</summary>
    void DaGhi(JsonObject kq, bool gop)
    {
        string thaoTac = Chung.Chuoi(kq, "thao_tac")!, nguoi = Chung.Chuoi(kq, "nguoi")!, ma = Chung.Chuoi(kq, "ma_lop")!;
        So.Ghi(new GdDangKy(thaoTac, nguoi, ma));
        if (!gop)                                        // người dùng thật: báo lên nhật ký
        {
            string viec = thaoTac == "dang_ky" ? "đăng ký" : "hủy";
            ThongBao($"{nguoi} {viec} {ma} ({Chung.SoL(kq, "da_dk")}/{TsL("si_so")})");
        }
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        long siSo = TsL("si_so");
        var ds = new JsonArray();
        foreach (var (ma, ten, gv, lich, tc) in lop)
        {
            string k = "lop:" + ma;                      // đang đặt lại (bảng ô mới chưa thay): coi như lớp trống
            var sv = Kho.Co(k) ? ((LopHp)Kho.Doc(k)).Sv : [];
            ds.Add(new JsonObject
            {
                ["ma"] = ma, ["ten"] = ten, ["gv"] = gv, ["lich"] = lich, ["tc"] = tc, ["si_so"] = siSo, ["da_dk"] = sv.Count,
                ["theo_so"] = SoDk.SoLop(ma),
                ["sv"] = new JsonArray([.. sv.Order(StringComparer.Ordinal).Select(x => (JsonNode)JsonValue.Create(x))]),
            });
        }
        return new JsonObject { ["si_so"] = siSo, ["lop"] = ds };
    }

    protected override string TomTat()
    {
        long cho = TsL("so_lop") * TsL("si_so");
        return $"Đã ĐK {SoDk.TongSo()}/{cho} chỗ ({TsL("so_lop")} lớp × {TsL("si_so")})";
    }
}
