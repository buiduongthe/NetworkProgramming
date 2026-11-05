using System.Text.Json.Nodes;

namespace BaiMR6_11;

// Bài mở rộng MR6.11 – Lõi máy chủ flash sale: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// "Giờ vàng" flash sale: kho có 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm "MUA NGAY" đúng
// lúc mở. Thao tác ghi duy nhất:
//     {"loai":"mua","ma_yc":k,"so_luong":1|2[,"nguoi":"u17"]}
// Luật: chưa tới giờ mở bán -> "chua_mo" (kèm con_ms); mỗi đơn 1–toi_da (mặc định 2) sản phẩm; tồn kho phải đủ (còn 1
// mà mua 2 -> "khong_du"); hết -> "het_hang".
//
// Trạng thái là MỘT ô "kho" của KhoTranhChap: record KhoHang(Ton, DaBan, SoDon) – điểm nóng mà mọi đơn hàng cùng
// tranh. Hai việc KHÔNG cần đi qua vùng găng: kiểm tra giờ mở bán (chỉ đọc đồng hồ) và "hết hàng nhanh" (đọc tồn kho
// không khóa: tồn chỉ giảm, nên đã thấy 0 thì chắc chắn là 0) – giúp hàng nghìn yêu cầu đến sau khi cháy hàng được
// trả lời ngay mà không phải xếp hàng chờ khóa.
//
// Sổ kiểm toán (tổng đã bán theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//   ton_am  – một đơn được chấp nhận làm tổng đã bán vượt tồn đầu kỳ (tồn kho thật bị âm – bán vượt);
//   lech_so – cuối buổi: tổng đã bán theo sổ ≠ tồn đầu kỳ − tồn hiện tại (đếm số sản phẩm lệch).

/// <summary>Ô "kho": tồn, đã bán, số đơn (bất biến – ghi là thay bản mới).</summary>
sealed record KhoHang(long Ton, long DaBan, long SoDon);

sealed record GdMua(string Nguoi, long SoLuong, string MaDon);

sealed class SoKiemToanFlashSale(long tonDau) : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("ton_am", "tồn âm"), ("lech_so", "lệch sổ")];
    long tongBan, soDon;                                 // theo sổ

    public long TongBan => Interlocked.Read(ref tongBan);
    public long SoDon => Interlocked.Read(ref soDon);

    protected override void KiemTra(object o)
    {
        var gd = (GdMua)o;
        Interlocked.Increment(ref soDon);
        long tong = Interlocked.Add(ref tongBan, gd.SoLuong);
        if (tong > tonDau) SaiPham("ton_am", $"{gd.Nguoi} mua {gd.SoLuong}: bán {tong}/{tonDau}");
    }

    protected override Dictionary<string, long> KiemTraCuoi(IReadOnlyDictionary<string, object> doc)
    {
        if (!doc.TryGetValue("kho", out var o) || o is not KhoHang k) return [];
        long lech = Math.Abs(tonDau - k.Ton - tongBan);
        return lech != 0 ? new() { ["lech_so"] = lech } : [];
    }
}

sealed class MayChuFlashSale(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                             IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new()
    {
        ["ten_sp"] = "Tai nghe không dây TWS Pro", ["gia_goc"] = 1_990_000L, ["gia_sale"] = 199_000L, ["ton_dau"] = 100L,
        ["mo_sau_s"] = 30.0, ["toi_da"] = 2L,
    };
    public override string Ten => "Flash sale MR6.11";
    public override string[] LoaiYeuCau => ["mua"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    double moLuc;                                        // thời điểm mở bán (đồng hồ đơn điệu)
    readonly object khoaDon = new();
    LinkedList<(string Nguoi, long SoLuong, string Gio)> donGan = new();   // đơn mới nhất trước (để hiển thị), tối đa 30
    volatile bool daBaoMo;
    int daBaoHet;

    SoKiemToanFlashSale SoFs => (SoKiemToanFlashSale)So;

    // ------------------------------------------------------------ tạo / đặt lại đợt sale
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("ton_dau", 1L) is < 1 or > 1_000_000) throw new ArgumentException("tồn đầu kỳ 1–1 000 000");
        if ((double)ts.GetValueOrDefault("mo_sau_s", 0.0) < 0) throw new ArgumentException("mo_sau_s ≥ 0");
        if ((long)ts.GetValueOrDefault("toi_da", 1L) is < 1 or > 10) throw new ArgumentException("tối đa mỗi đơn 1–10");
        if ((long)ts.GetValueOrDefault("gia_sale", 1L) <= 0 || (long)ts.GetValueOrDefault("gia_goc", 1L) <= 0) throw new ArgumentException("giá > 0");
    }

    protected override void KhoiTao()
    {
        Kho.DatO("kho", new KhoHang(TsL("ton_dau"), 0, 0));
        So = new SoKiemToanFlashSale(TsL("ton_dau"));
        moLuc = Chung.Now + TsD("mo_sau_s");
        lock (khoaDon) donGan = new();
        daBaoMo = TsD("mo_sau_s") <= 0;
        daBaoHet = 0;
    }

    // ------------------------------------------------------------ mua
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        long toiDa = TsL("toi_da"), gia = TsL("gia_sale");
        if (!Chung.LaSoNguyen(tin["so_luong"], out long sl) || sl < 1 || sl > toiDa)
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = $"số lượng 1–{toiDa}" });
            return;
        }
        double con = moLuc - Chung.Now;
        if (con > 0)                                     // chưa tới giờ: chỉ đọc đồng hồ, không chạm kho
        {
            traLoi(new JsonObject
            {
                ["ok"] = false, ["ma_loi"] = "chua_mo", ["ly_do"] = $"chưa mở bán, còn {Chung.F(con, "F1")} s", ["con_ms"] = (long)Math.Round(con * 1000),
            });
            return;
        }
        if (((KhoHang)Kho.Doc("kho")).Ton <= 0)          // hết hàng nhanh: tồn chỉ giảm, đọc không khóa
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "het_hang", ["ly_do"] = "hết hàng", ["ton"] = 0 });
            return;
        }

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN
        {
            var k = (KhoHang)doc["kho"];
            if (k.Ton <= 0) return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_hang", ["ly_do"] = "hết hàng", ["ton"] = 0 }, null);
            if (k.Ton < sl)
                return (new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_du", ["ly_do"] = $"chỉ còn {k.Ton} sản phẩm", ["ton"] = k.Ton }, null);
            var moi = new KhoHang(k.Ton - sl, k.DaBan + sl, k.SoDon + 1);
            return (new JsonObject
            {
                ["ok"] = true, ["ma_don"] = $"FS{moi.SoDon:00000}", ["so_luong"] = sl, ["ton"] = moi.Ton, ["nguoi"] = nguoi, ["thanh_tien"] = sl * gia,
            }, new() { ["kho"] = moi });
        }

        bool gop = khach.SoNguoi != null;
        Kho.ThucHien(["kho"], XuLy, traLoi, kq => DaBan(kq, gop));
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.</summary>
    void DaBan(JsonObject kq, bool gop)
    {
        string nguoi = Chung.Chuoi(kq, "nguoi")!, maDon = Chung.Chuoi(kq, "ma_don")!;
        long sl = Chung.SoL(kq, "so_luong"), ton = Chung.SoL(kq, "ton");
        So.Ghi(new GdMua(nguoi, sl, maDon));
        lock (khoaDon)
        {
            donGan.AddFirst((nguoi, sl, DateTime.Now.ToString("HH:mm:ss")));
            while (donGan.Count > 30) donGan.RemoveLast();
        }
        if (!gop) ThongBao($"{nguoi} mua {sl} – đơn {maDon}, còn {ton}");
        if (ton == 0 && Interlocked.Exchange(ref daBaoHet, 1) == 0)
            ThongBao($"HẾT HÀNG sau {Chung.F(Chung.Now - moLuc, "F2")} s mở bán");
        BaoDoi();
    }

    // ------------------------------------------------------------ đồng hồ mở bán (luồng nền, 100 ms)
    protected override void DinhKy(double bayGio)
    {
        if (daBaoMo || bayGio < moLuc) return;
        daBaoMo = true;
        ThongBao($"MỞ BÁN! {TsL("ton_dau")} sản phẩm");
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        var k = (KhoHang)Kho.Doc("kho");
        double con = moLuc - Chung.Now;
        List<(string Nguoi, long SoLuong, string Gio)> don;
        lock (khoaDon) don = [.. donGan.Take(10)];
        return new JsonObject
        {
            ["sp"] = new JsonObject { ["ten"] = TsS("ten_sp"), ["gia_goc"] = TsL("gia_goc"), ["gia_sale"] = TsL("gia_sale"), ["toi_da"] = TsL("toi_da") },
            ["ton_dau"] = TsL("ton_dau"), ["ton"] = k.Ton, ["da_ban"] = k.DaBan, ["so_don"] = k.SoDon, ["ban_theo_so"] = SoFs.TongBan,
            ["mo_ban"] = con <= 0, ["mo_con_ms"] = Math.Max(0L, (long)Math.Round(con * 1000)),
            ["don_gan"] = new JsonArray([.. don.Select(x => (JsonNode)new JsonObject { ["nguoi"] = x.Nguoi, ["so_luong"] = x.SoLuong, ["gio"] = x.Gio })]),
        };
    }

    protected override string TomTat() =>
        $"Bán {SoFs.TongBan}/{TsL("ton_dau")} sp ({SoFs.SoDon} đơn), tồn {((KhoHang)Kho.Doc("kho")).Ton}";
}
