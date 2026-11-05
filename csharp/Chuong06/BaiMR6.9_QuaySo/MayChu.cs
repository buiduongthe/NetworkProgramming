using System.Text.Json.Nodes;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Lõi máy chủ quay số trúng thưởng: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// Chương trình khuyến mãi "Vòng quay may mắn": 10 món quà, hàng chục nghìn lượt quay. Thao tác ghi duy nhất:
//     {"loai":"quay","ma_yc":k[,"nguoi":"u17"]}
// Luật: mỗi người quay ĐÚNG MỘT lần (quay lần nữa – dù bằng ma_yc khác, từ thiết bị khác – bị từ chối "da_quay"
// kèm kết quả cũ: idempotency theo tên); mỗi lượt trúng với xác suất p (mặc định 1/2000) nếu còn quà; trúng thì
// nhận ngẫu nhiên một món còn lại.
//
// Tài nguyên trong KhoTranhChap:
//   - ô "qua"      : KhoQua(Con = mã các món còn lại) – ô "nóng" mà mọi lượt TRÚNG cùng tranh;
//   - ô "ng:<tên>" : LuotQuay(DaQuay, Qua) – một ô cho mỗi người, tạo khi người đó quay lần đầu. Lượt không trúng
//                    chỉ chạm ô của chính mình nên các lượt chạy song song được (khóa theo tài nguyên); chỉ lượt
//                    trúng mới khóa thêm ô "qua".
// Rút thăm (NextDouble() < p) làm TRƯỚC, ngoài vùng găng: kết quả rút thăm không phụ thuộc trạng thái.
//
// Sổ kiểm toán đếm hai sai phạm:
//   thua_qua    – một món quà đã trao lại được trao lần nữa (tổng số quà trao > số quà có);
//   quay_2_lan  – một người có hai lượt quay được chấp nhận (hai thiết bị cùng bấm, ô của người đó bị ghi đè).

/// <summary>Danh mục quà và cách viết xác suất.</summary>
static class Qua
{
    public static readonly string[] TenQua = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô",
                                              "Nồi cơm điện", "Loa", "Phiếu 500k"];

    /// <summary>0.0005 -> "1/2000"; 0.3 -> "0.3".</summary>
    public static string ChuP(double p)
    {
        if (p > 0 && Math.Abs(1 / p - Math.Round(1 / p)) < 1e-6 && Math.Round(1 / p) > 1)
            return $"1/{(long)Math.Round(1 / p)}";
        return Chung.G(p);
    }
}

/// <summary>Ô "qua": mã các món còn lại (mảng không sửa tại chỗ – ghi là thay mảng mới).</summary>
sealed record KhoQua(int[] Con);

/// <summary>Ô "ng:&lt;tên&gt;": đã quay chưa, trúng món nào (null: không trúng).</summary>
sealed record LuotQuay(bool DaQuay, int? Qua)
{
    public static readonly LuotQuay Chua = new(false, null);
}

sealed record GdQuay(string Nguoi, int? Qua);

sealed class SoKiemToanQuaySo : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("thua_qua", "thừa quà"), ("quay_2_lan", "quay 2 lần")];
    readonly HashSet<string> daQuay = [];                 // người đã có lượt quay được chấp nhận (theo sổ)
    readonly Dictionary<int, string> daTrao = [];         // mã quà -> người nhận đầu tiên
    long soTrao, soGd;                                    // số lượt trúng / số lượt quay được chấp nhận (theo sổ)

    public long SoTrao => Interlocked.Read(ref soTrao);
    public long SoGd => Interlocked.Read(ref soGd);

    protected override void KiemTra(object o)
    {
        var gd = (GdQuay)o;
        Interlocked.Increment(ref soGd);
        if (!daQuay.Add(gd.Nguoi)) SaiPham("quay_2_lan", $"{gd.Nguoi} quay lần 2");
        if (gd.Qua is not { } q) return;
        Interlocked.Increment(ref soTrao);
        if (daTrao.TryGetValue(q, out var dau)) SaiPham("thua_qua", $"{Qua.TenQua[q]}: {dau} rồi {gd.Nguoi}");
        else daTrao[q] = gd.Nguoi;
    }
}

sealed class MayChuQuaySo(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                          IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new() { ["so_qua"] = 10L, ["p"] = 0.0005 };
    public override string Ten => "Quay số MR6.9";
    public override string[] LoaiYeuCau => ["quay"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    readonly object khoaTao = new();                      // tạo ô "ng:<tên>" đúng một lần
    HashSet<string> daTao = [];
    readonly object khoaTrung = new();
    LinkedList<(string Nguoi, int Qua, string Gio)> nguoiTrung = new();   // mới nhất trước (để hiển thị), tối đa 50
    int baoHet;

    SoKiemToanQuaySo SoQ => (SoKiemToanQuaySo)So;

    // ------------------------------------------------------------ tạo / đặt lại chương trình
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("so_qua", 1L) is < 1 or > 10) throw new ArgumentException($"số quà 1–{Qua.TenQua.Length}");
        if ((double)ts.GetValueOrDefault("p", 0.0) is not (>= 0 and <= 1)) throw new ArgumentException("xác suất p trong [0, 1]");
    }

    protected override void KhoiTao()
    {
        int soQua = (int)TsL("so_qua");
        Kho.DatO("qua", new KhoQua([.. Enumerable.Range(0, soQua)]));
        So = new SoKiemToanQuaySo();
        lock (khoaTao) daTao = [];
        lock (khoaTrung) nguoiTrung = new();
        baoHet = 0;
    }

    string ONguoi(string nguoi)
    {
        string k = "ng:" + nguoi;
        lock (khoaTao)
            if (daTao.Add(k)) Kho.DatO(k, LuotQuay.Chua);
        return k;
    }

    // ------------------------------------------------------------ quay
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        string kNg = ONguoi(nguoi);
        bool rutTrung = Random.Shared.NextDouble() < TsD("p");          // kim dừng ở ô quà với xác suất p
        var cacKhoa = new List<string> { kNg };
        if (rutTrung && ((KhoQua)Kho.Doc("qua")).Con.Length > 0) cacKhoa.Add("qua");   // hết quà: khỏi chạm ô "qua"

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN
        {
            var cu = (LuotQuay)doc[kNg];
            if (cu.DaQuay)
            {
                string truoc = cu.Qua is { } c ? $"trúng {Qua.TenQua[c]}" : "không trúng";
                return (new JsonObject { ["ok"] = false, ["ma_loi"] = "da_quay", ["ly_do"] = $"đã quay rồi ({truoc})", ["qua"] = cu.Qua }, null);
            }
            if (doc.TryGetValue("qua", out var o) && o is KhoQua { Con.Length: > 0 } q)
            {
                int chon = q.Con[Random.Shared.Next(q.Con.Length)];
                int[] con = [.. q.Con.Where(x => x != chon)];
                return (new JsonObject
                {
                    ["ok"] = true, ["trung"] = true, ["qua"] = chon, ["ten_qua"] = Qua.TenQua[chon], ["nguoi"] = nguoi, ["con_lai"] = con.Length,
                }, new() { [kNg] = new LuotQuay(true, chon), ["qua"] = new KhoQua(con) });
            }
            return (new JsonObject { ["ok"] = true, ["trung"] = false, ["qua"] = null, ["nguoi"] = nguoi },
                    new() { [kNg] = new LuotQuay(true, null) });
        }

        bool gop = khach.SoNguoi != null;                 // người ảo của bộ mô phỏng: không báo từng người
        Kho.ThucHien(cacKhoa, XuLy, traLoi, kq => DaQuay(kq, gop));
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.</summary>
    void DaQuay(JsonObject kq, bool gop)
    {
        string nguoi = Chung.Chuoi(kq, "nguoi")!;
        int? qua = Chung.SoLN(kq["qua"]) is { } q ? (int)q : null;
        So.Ghi(new GdQuay(nguoi, qua));
        if (Chung.Dung(kq, "trung") && qua is { } m)
        {
            lock (khoaTrung)
            {
                nguoiTrung.AddFirst((nguoi, m, DateTime.Now.ToString("HH:mm:ss")));
                while (nguoiTrung.Count > 50) nguoiTrung.RemoveLast();
            }
            if (!gop) ThongBao($"{nguoi} trúng {Chung.Chuoi(kq, "ten_qua")}");
            if (Chung.SoL(kq, "con_lai") == 0 && Interlocked.Exchange(ref baoHet, 1) == 0)
                ThongBao($"Đã trao hết quà sau {SoQ.SoGd} lượt");
        }
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        var con = ((KhoQua)Kho.Doc("qua")).Con.ToHashSet();
        List<(string Nguoi, int Qua, string Gio)> trung;
        lock (khoaTrung) trung = [.. nguoiTrung];
        var nhan = new Dictionary<int, string>();
        for (int i = trung.Count - 1; i >= 0; i--) nhan[trung[i].Qua] = trung[i].Nguoi;   // người nhận gần nhất của mỗi món
        int soQua = (int)TsL("so_qua");
        var dsQua = new JsonArray();
        for (int i = 0; i < soQua; i++)
            dsQua.Add(new JsonObject
            {
                ["ma"] = i, ["ten"] = Qua.TenQua[i], ["con"] = con.Contains(i),
                ["nguoi"] = con.Contains(i) ? null : nhan.GetValueOrDefault(i),
            });
        return new JsonObject
        {
            ["so_qua"] = soQua, ["p"] = TsD("p"), ["con_lai"] = con.Count, ["qua"] = dsQua, ["da_quay"] = SoQ.SoGd,
            ["so_trung"] = SoQ.SoTrao,
            ["nguoi_trung"] = new JsonArray([.. trung.Take(20).Select(x => (JsonNode)new JsonObject { ["nguoi"] = x.Nguoi, ["qua"] = x.Qua, ["gio"] = x.Gio })]),
        };
    }

    protected override string TomTat() => $"Trao {SoQ.SoTrao}/{TsL("so_qua")} quà, {SoQ.SoGd} lượt, p {Qua.ChuP(TsD("p"))}";
}
