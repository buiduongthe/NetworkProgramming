using System.Text.Json.Nodes;

namespace BaiMR6_6;

// Bài mở rộng MR6.6 – Lõi máy chủ đấu giá: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// Một món hàng, n người cùng trả giá. Thao tác ghi duy nhất:
//     {"loai":"tra_gia","ma_yc":k,"gia":5300000[,"nguoi":"u17"]}
// Luật: giá ≥ giá hiện tại + bước giá; phiên còn hạn. Trả giá khi phiên còn < gia_han_s giây (mặc định 10 s) thì
// phiên được gia hạn tới "bây giờ + gia_han_s" ("đóng mềm" – soft close, chống "bắn tỉa" phút cuối – anti-sniping).
// Hết giờ: máy chủ đóng phiên, công bố người thắng.
//
// Trạng thái là MỘT ô "mon" của KhoTranhChap: record bất biến Mon(Gia, Nguoi, SoLan, HetHan, KetThuc); mọi thay đổi
// tạo bản mới bằng "with" (xuLy thuần) nên cả 4 chế độ dùng chung đúng một hàm nghiệp vụ.
//
// Sổ kiểm toán đếm hai sai phạm:
//   gia_thap_hon – một giá được chấp nhận mà thấp hơn (giá cao nhất đã chấp nhận trước + bước giá): hai người cùng
//                  đọc giá cũ, cùng "thắng" kiểm tra, người ghi sau đè người ghi trước;
//   gia_cuoi_sai – giá/người dẫn đầu cuối cùng khác giá cao nhất đã chấp nhận (giá cao bị ghi đè mất).

/// <summary>Trạng thái món hàng (giá trị của ô "mon"). HetHan: thời điểm theo đồng hồ đơn điệu (null = không giới hạn).</summary>
sealed record Mon(long Gia, string? Nguoi, long SoLan, double? HetHan, bool KetThuc);

sealed record GdDauGia(string Nguoi, long Gia);

sealed class SoKiemToanDauGia(long buocGia) : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("gia_thap_hon", "giá thấp hơn"), ("gia_cuoi_sai", "giá cuối sai")];
    long? caoNhat;                                       // giá cao nhất đã chấp nhận (theo sổ)
    string? nguoiCaoNhat;

    protected override void KiemTra(object o)
    {
        var gd = (GdDauGia)o;
        if (caoNhat is { } c && gd.Gia < c + buocGia)
            SaiPham("gia_thap_hon", $"{gd.Nguoi} {Chung.Tien(gd.Gia)} sau {Chung.Tien(c)}");
        if (caoNhat == null || gd.Gia > caoNhat) (caoNhat, nguoiCaoNhat) = (gd.Gia, gd.Nguoi);
    }

    protected override Dictionary<string, long> KiemTraCuoi(IReadOnlyDictionary<string, object> doc)
    {
        if (!doc.TryGetValue("mon", out var o) || caoNhat == null) return [];
        var m = (Mon)o;
        bool sai = m.Gia != caoNhat || m.Nguoi != nguoiCaoNhat;
        return sai ? new() { ["gia_cuoi_sai"] = 1 } : [];
    }
}

sealed class MayChuDauGia(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                          IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new()
    {
        ["ten_mon"] = "Bình gốm hoa lam", ["gia_khoi_diem"] = 5_000_000L, ["buoc_gia"] = 100_000L,
        ["thoi_gian_s"] = 0.0, ["gia_han_s"] = 10.0,   // thoi_gian_s = 0: không giới hạn
    };
    public override string Ten => "Đấu giá MR6.6";
    public override string[] LoaiYeuCau => ["tra_gia"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    readonly object khoaLs = new();
    LinkedList<JsonObject> lichSu = new();              // giá đã chấp nhận, mới nhất trước (để hiển thị), tối đa 50
    volatile bool dangDong;
    double lucBaoGiaHan;

    // ------------------------------------------------------------ tạo / đặt lại phiên
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        if ((long)ts.GetValueOrDefault("buoc_gia", 1L) <= 0 || (long)ts.GetValueOrDefault("gia_khoi_diem", 0L) < 0)
            throw new ArgumentException("bước giá > 0, giá khởi điểm ≥ 0");
        if ((double)ts.GetValueOrDefault("thoi_gian_s", 0.0) < 0 || (double)ts.GetValueOrDefault("gia_han_s", 0.0) < 0)
            throw new ArgumentException("thời gian ≥ 0");
        if (ts.TryGetValue("ten_mon", out var t) && ((string)t).Length is < 1 or > 40)
            throw new ArgumentException("tên món 1–40 ký tự");
    }

    protected override void KhoiTao()
    {
        double tg = TsD("thoi_gian_s");
        double? hetHan = tg > 0 ? Chung.Now + tg : null;
        Kho.DatO("mon", new Mon(TsL("gia_khoi_diem"), null, 0, hetHan, false));
        So = new SoKiemToanDauGia(TsL("buoc_gia"));
        lock (khoaLs) lichSu = new();
        dangDong = false;
        lucBaoGiaHan = 0.0;
    }

    // ------------------------------------------------------------ trả giá
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        if (!Chung.LaSoNguyen(tin["gia"], out long gia) || gia <= 0)
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = "giá phải là số nguyên dương" });
            return;
        }
        long buoc = TsL("buoc_gia");
        double giaHanS = TsD("gia_han_s");

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // hàm THUẦN
        {
            var m = (Mon)doc["mon"];
            double bayGio = Chung.Now;
            if (m.KetThuc || (m.HetHan is { } h && bayGio >= h))
                return (new JsonObject
                {
                    ["ok"] = false, ["ma_loi"] = "het_phien", ["ly_do"] = "phiên đã kết thúc", ["gia"] = m.Gia, ["nguoi_dan_dau"] = m.Nguoi,
                }, null);
            if (gia < m.Gia + buoc)
                return (new JsonObject
                {
                    ["ok"] = false, ["ma_loi"] = "gia_thap", ["ly_do"] = $"giá phải ≥ {Chung.Tien(m.Gia + buoc)}", ["gia"] = m.Gia,
                    ["nguoi_dan_dau"] = m.Nguoi,
                }, null);
            var moi = m with { Gia = gia, Nguoi = nguoi, SoLan = m.SoLan + 1 };
            bool giaHan = m.HetHan is { } hh && hh - bayGio < giaHanS;
            if (giaHan) moi = moi with { HetHan = bayGio + giaHanS };
            return (new JsonObject { ["ok"] = true, ["gia"] = gia, ["nguoi_dan_dau"] = nguoi, ["gia_han"] = giaHan },
                    new() { ["mon"] = moi });
        }

        Kho.ThucHien(["mon"], XuLy, traLoi, DaChapNhan);
    }

    /// <summary>Gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.</summary>
    void DaChapNhan(JsonObject kq)
    {
        string nguoi = Chung.Chuoi(kq, "nguoi_dan_dau")!;
        long gia = Chung.SoL(kq, "gia");
        bool giaHan = Chung.Dung(kq, "gia_han");
        So.Ghi(new GdDauGia(nguoi, gia));
        lock (khoaLs)                                    // khong_khoa: nhiều luồng cùng thêm – danh sách cần khóa riêng
        {
            lichSu.AddFirst(new JsonObject { ["nguoi"] = nguoi, ["gia"] = gia, ["gio"] = DateTime.Now.ToString("HH:mm:ss"), ["gia_han"] = giaHan });
            while (lichSu.Count > 50) lichSu.RemoveLast();
        }
        if (giaHan && Chung.Now - lucBaoGiaHan > 1.0)
        {
            lucBaoGiaHan = Chung.Now;
            ThongBao($"Gia hạn: {nguoi} trả giá phút cuối, còn {Chung.G(TsD("gia_han_s"))} s");
        }
        BaoDoi();
    }

    // ------------------------------------------------------------ đồng hồ phiên (luồng nền, 100 ms)
    protected override void DinhKy(double bayGio)
    {
        var m = (Mon)Kho.Doc("mon");
        if (m.KetThuc || m.HetHan is not { } h || bayGio < h || dangDong) return;
        dangDong = true;

        static (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // đóng phiên cũng là một thao tác ghi
        {
            var m = (Mon)doc["mon"];
            if (m.KetThuc || m.HetHan is not { } h || Chung.Now < h)
                return (new JsonObject { ["ok"] = false }, null);                  // vừa được gia hạn hoặc đã đóng
            return (new JsonObject { ["ok"] = true, ["nguoi"] = m.Nguoi, ["gia"] = m.Gia }, new() { ["mon"] = m with { KetThuc = true } });
        }

        Kho.ThucHien(["mon"], XuLy, _ => dangDong = false, DaDong);
    }

    void DaDong(JsonObject kq)
    {
        if (Chung.Chuoi(kq, "nguoi") is { } ai)
            ThongBao($"Kết thúc phiên: {ai} thắng với {Chung.Tien(Chung.SoL(kq, "gia"))}");
        else ThongBao("Kết thúc phiên: không có ai trả giá");
        BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        var m = (Mon)Kho.Doc("mon");
        long? conLai = m.HetHan is { } h ? Math.Max(0, (long)Math.Round((h - Chung.Now) * 1000)) : null;
        JsonArray ls;
        lock (khoaLs) ls = new JsonArray([.. lichSu.Take(12).Select(x => (JsonNode)x.DeepClone())]);
        return new JsonObject
        {
            ["mon"] = new JsonObject
            {
                ["ten"] = TsS("ten_mon"), ["gia_khoi_diem"] = TsL("gia_khoi_diem"), ["buoc_gia"] = TsL("buoc_gia"), ["gia_han_s"] = TsD("gia_han_s"),
            },
            ["gia"] = m.Gia, ["nguoi_dan_dau"] = m.Nguoi, ["so_lan"] = m.SoLan,
            ["con_lai_ms"] = conLai, ["ket_thuc"] = m.KetThuc,
            ["lich_su"] = ls,
        };
    }

    protected override string TomTat()
    {
        var m = (Mon)Kho.Doc("mon");
        return $"Giá cuối {Chung.Tien(m.Gia)}, {(m.KetThuc ? "thắng" : "dẫn đầu")} {m.Nguoi ?? "–"}";
    }
}
