using System.Text.Json.Nodes;

namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Lõi máy chủ bán vé có phòng chờ ảo: nghiệp vụ + sổ kiểm toán (khung ở Chung.cs).
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Bài 6.1 (server đa luồng – một luồng mỗi kết nối).
// Bán 500 vé concert cho rất nhiều người cùng lúc. Ba thao tác của người dùng:
//     {"loai":"xep_hang","ma_yc":k}         vào phòng chờ -> số thứ tự, số người phía trước, thời gian chờ ước tính
//     {"loai":"cho_luot","ma_yc":k}         "chờ dài" (long polling): máy chủ CHỈ trả lời khi tới lượt (kèm han_ms –
//                                           thời gian còn lại để mua) hoặc khi hết vé
//     {"loai":"mua","ma_yc":k,"so_ve":2}    mua 1–4 vé; khi phòng chờ bật phải đang có lượt còn hạn
// (kết nối gộp của bộ mô phỏng thêm "nguoi":"u17").
//
// Phòng chờ ảo (virtual waiting room): máy chủ cấp lượt theo THÙNG TOKEN (token bucket): token rót vào đều toc_do
// token/giây, thùng chứa tối đa dung_luong token; mỗi 100 ms luồng nền lấy người đầu hàng (số thứ tự nhỏ nhất – hàng
// ưu tiên PriorityQueue) ra, mỗi người tốn một token -> lượt có hạn luot_s giây (mặc định 20 s) để mua. Nhờ vậy phía
// "mua" (phần đắt: kiểm tra, ghi kho) chỉ nhận đều đặn vài chục người mỗi giây dù hàng chờ có hàng nghìn người.
// Tắt phòng chờ (phong_cho = 0): ai cũng gọi mua ngay -> dồn cục; máy chủ chỉ cho tối đa gioi_han_xu_ly yêu cầu mua
// đang xử lý cùng lúc, quá số đó từ chối ngay "qua_tai" (như HTTP 503) -> nhiều từ chối, độ trễ cao.
//
// Các ô của KhoTranhChap (đi qua 4 cách xử lý tranh chấp):
//   "hang" HangCho(Cuoi = số thứ tự cuối đã phát)  – xep_hang đọc, +1, ghi: phát số thứ tự;
//   "kho"  KhoVe(Con = vé còn, DaBan = đã bán)     – mua đọc, kiểm tra số vé, trừ kho;
//   "luot:<tên>" Luot(Con = còn lượt?, Han)        – tạo khi cấp lượt; mua đọc cùng "kho" và ghi Con = false trong
//                                                    CÙNG thao tác (tiêu lượt nguyên tử với trừ kho).
// Sổ kiểm toán đếm ba sai phạm:
//   ban_qua    "vượt vé"  – số vé bán vượt tổng số vé (theo sổ: cộng số vé mọi đơn được chấp nhận);
//   khong_luot "sai lượt" – một đơn được chấp nhận khi người đó không còn lượt (lượt đã dùng ở tab khác);
//   dao_thu_tu "đảo số"   – người vào hàng sau nhận số thứ tự không lớn hơn người vào trước (FIFO bị phá).

sealed record KhoVe(long Con, long DaBan);

sealed record HangCho(long Cuoi);

sealed record Luot(bool Con, double Han);

/// <summary>Giao dịch ghi sổ: "xep" (phát số), "luot" (cấp lượt), "mua" (đơn được chấp nhận).</summary>
sealed record GdPhongCho(string Loai, string Nguoi, long So = 0, long SoVe = 0, bool CanLuot = false);

sealed class SoKiemToanPhongCho(long tongVe) : SoKiemToan
{
    protected override (string Ma, string Ten)[] TenSaiPham => [("ban_qua", "vượt vé"), ("khong_luot", "sai lượt"), ("dao_thu_tu", "đảo số")];
    long daBan;                                          // tổng vé theo sổ
    long soMax;                                          // số thứ tự lớn nhất đã phát (theo thứ tự ghi sổ)
    readonly HashSet<string> luot = [];                  // người đang có lượt chưa dùng (theo sổ)

    public long DaBan => Interlocked.Read(ref daBan);

    protected override void KiemTra(object o)
    {
        var gd = (GdPhongCho)o;
        if (gd.Loai == "xep")
        {
            if (gd.So <= soMax) SaiPham("dao_thu_tu", $"{gd.Nguoi} vào sau nhưng nhận số {gd.So} ≤ {soMax}");
            else soMax = gd.So;
        }
        else if (gd.Loai == "luot") luot.Add(gd.Nguoi);
        else if (gd.Loai == "mua")
        {
            if (gd.CanLuot && !luot.Remove(gd.Nguoi)) SaiPham("khong_luot", $"{gd.Nguoi} mua {gd.SoVe} vé khi lượt đã dùng");
            long truoc = daBan, sau = Interlocked.Add(ref daBan, gd.SoVe);
            long vuot = sau - Math.Max(truoc, tongVe);
            if (vuot > 0)                                // đếm theo SỐ VÉ bán vượt
            {
                CongSaiPham("ban_qua", vuot - 1);
                SaiPham("ban_qua", $"{gd.Nguoi} mua {gd.SoVe} vé, sổ đã bán {sau}");
            }
        }
    }
}

sealed class MayChuPhongCho(string cheDo = "khoa", double treMs = 2.0, Action<string>? bao = null,
                            IEnumerable<KeyValuePair<string, object>>? thamSo = null) : MayChuCoSo(cheDo, treMs, bao, thamSo)
{
    static readonly OrderedDictionary<string, object> MacDinh = new()
    {
        ["ten_su_kien"] = "Đêm nhạc Mùa Đông 2026", ["gia_ve"] = 1_500_000L, ["tong_ve"] = 500L, ["ve_toi_da"] = 4L, ["phong_cho"] = 1L,
        ["toc_do"] = 50.0, ["dung_luong"] = 50L, ["luot_s"] = 20.0, ["gioi_han_xu_ly"] = 64L,
    };
    public override string Ten => "Bán vé MR6.13";
    public override string[] LoaiYeuCau => ["xep_hang", "cho_luot", "mua"];
    public override OrderedDictionary<string, object> ThamSoMacDinh => MacDinh;

    static JsonObject KqHetVe() => new() { ["ok"] = false, ["ma_loi"] = "het_ve", ["ly_do"] = "đã bán hết vé" };

    /// <summary>Một người trong phòng chờ: số thứ tự, trạng thái "cho" | "luot" | "da_mua" | "het_han" | "het_ve", hạn
    /// lượt, các yêu cầu cho_luot đang "chờ dài".</summary>
    sealed class NguoiCho(long so, string trangThai)
    {
        public readonly long So = so;
        public string TrangThai = trangThai;
        public double? Han;
        public List<Action<JsonObject>> Cho = [];
    }

    readonly object khoaPc = new();                      // bảo vệ phòng chờ (danh sách người, hàng, thùng token)
    long maDon;                                          // mã đơn tăng mãi, kể cả qua các lần đặt lại
    Dictionary<string, NguoiCho> nguoi = [];
    PriorityQueue<string, (long So, long Stt)> heap = new();   // người đang chờ, theo (số thứ tự, thứ tự ghi)
    long stt;
    Queue<(double Han, string Ten)> coLuot = new();      // theo thứ tự cấp lượt – để thu hồi lượt quá hạn
    long dangPhucVu, dangCho, daCapLuot, hetHan;         // số thứ tự lớn nhất đã được cấp lượt, …
    double token, tToken;
    bool hetVe;
    long dangXuLy, xuLyMax, quaTai, soDon;
    LinkedList<JsonObject> lichSu = new();               // đơn mới nhất trước (tối đa 30)

    SoKiemToanPhongCho SoPc => (SoKiemToanPhongCho)So;

    // ------------------------------------------------------------ tạo / đặt lại
    protected override void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        base.KiemThamSo(ts);
        long L(string k, long md) => (long)ts.GetValueOrDefault(k, md);
        double D(string k, double md) => (double)ts.GetValueOrDefault(k, md);
        if (L("tong_ve", 1) < 1 || L("ve_toi_da", 1) is < 1 or > 10 || L("gia_ve", 0) < 0)
            throw new ArgumentException("tổng vé ≥ 1, vé tối đa 1–10, giá ≥ 0");
        if (L("phong_cho", 0) is not (0 or 1)) throw new ArgumentException("phong_cho = 0 hoặc 1");
        if (!(D("toc_do", 1) > 0 && D("toc_do", 1) <= 100_000) || L("dung_luong", 1) < 1)
            throw new ArgumentException("tốc độ cấp lượt 0–100000 người/s, dung lượng thùng ≥ 1");
        if (!(D("luot_s", 1) > 0) || L("gioi_han_xu_ly", 1) < 1) throw new ArgumentException("thời hạn lượt > 0, giới hạn xử lý ≥ 1");
        if (ts.TryGetValue("ten_su_kien", out var t) && ((string)t).EnumerateRunes().Count() is < 1 or > 40)
            throw new ArgumentException("tên sự kiện 1–40 ký tự");
    }

    protected override void KhoiTao()
    {
        Kho.DatO("kho", new KhoVe(TsL("tong_ve"), 0));
        Kho.DatO("hang", new HangCho(0));
        So = new SoKiemToanPhongCho(TsL("tong_ve"));
        List<Action<JsonObject>> cu;
        lock (khoaPc)
        {
            cu = [.. nguoi.Values.SelectMany(ng => ng.Cho)];
            nguoi = [];
            heap = new();
            stt = 0;
            coLuot = new();
            (dangPhucVu, dangCho, daCapLuot, hetHan) = (0, 0, 0, 0);
            (token, tToken) = (TsL("dung_luong"), Chung.Now);
            hetVe = false;
            (dangXuLy, xuLyMax, quaTai, soDon) = (0, 0, 0, 0);
            lichSu = new();
        }
        foreach (var cb in cu)                           // ai đang "chờ dài" từ phiên trước: trả lời ngay
            cb(new JsonObject { ["ok"] = false, ["ma_loi"] = "dat_lai", ["ly_do"] = "phiên bán vé được đặt lại" });
    }

    /// <summary>Đổi tốc độ cấp lượt ngay khi đang chạy (không đặt lại phiên).</summary>
    public void DatTocDo(double tocDo)
    {
        KiemThamSo(new OrderedDictionary<string, object> { ["toc_do"] = tocDo });
        ThamSo["toc_do"] = tocDo;
        ThongBao($"Tốc độ cấp lượt: {Chung.G(tocDo)} người/s");
        BaoDoi();
    }

    // ------------------------------------------------------------ yêu cầu
    protected override void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi)
    {
        string loai = Chung.Chuoi(tin, "loai")!;
        if (loai == "mua") Mua(nguoi, tin, traLoi);
        else if (TsL("phong_cho") == 0)
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "pc_tat", ["ly_do"] = "phòng chờ đang tắt – mua trực tiếp" });
        else if (loai == "xep_hang") XepHang(nguoi, traLoi);
        else ChoLuot(nguoi, traLoi);
    }

    /// <summary>Số người phía trước và thời gian chờ ước tính (gọi dưới khoaPc), ghi vào kq.</summary>
    void ViTri(NguoiCho ng, JsonObject kq)
    {
        long truoc = ng.TrangThai == "cho" ? Math.Max(0, ng.So - dangPhucVu - 1) : 0;
        kq["so"] = ng.So;
        kq["truoc"] = truoc;
        kq["cho_s"] = Math.Round((truoc + 1) / TsD("toc_do"), 1, MidpointRounding.ToEven);
        kq["trang_thai"] = ng.TrangThai;
    }

    void XepHang(string ten, Action<JsonObject> traLoi)
    {
        JsonObject? kq = null;
        lock (khoaPc)
        {
            if (hetVe) kq = KqHetVe();
            else if (nguoi.TryGetValue(ten, out var co))  // đã trong hàng (tải lại trang, tab thứ hai): giữ chỗ cũ
            {
                kq = new JsonObject { ["ok"] = true, ["da_co"] = true };
                ViTri(co, kq);
            }
        }
        if (kq != null)
        {
            traLoi(kq);
            return;
        }

        static (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // phát số thứ tự: đọc – cộng 1 – ghi
        {
            long so = ((HangCho)doc["hang"]).Cuoi + 1;
            return (new JsonObject { ["ok"] = true, ["so"] = so }, new() { ["hang"] = new HangCho(so) });
        }

        void KhiGhi(JsonObject kq)                       // trong vùng găng: đưa vào hàng, ghi sổ
        {
            long so = Chung.SoL(kq, "so");
            bool moi;
            lock (khoaPc)
            {
                moi = !nguoi.TryGetValue(ten, out var ng);
                if (moi && hetVe) ng = nguoi[ten] = new NguoiCho(so, "het_ve");   // vừa hết vé trong lúc xếp hàng: cho_luot báo hết vé ngay
                else if (moi)
                {
                    ng = nguoi[ten] = new NguoiCho(so, "cho");
                    heap.Enqueue(ten, (so, stt++));
                    dangCho++;
                }
                else kq["da_co"] = true;
                ViTri(ng!, kq);
            }
            if (moi) So.Ghi(new GdPhongCho("xep", ten, So: so));
            BaoDoi();
        }

        Kho.ThucHien(["hang"], XuLy, traLoi, KhiGhi);
    }

    /// <summary>Chờ dài: chưa tới lượt thì GIỮ traLoi, luồng nền gọi khi cấp lượt / hết vé.</summary>
    void ChoLuot(string ten, Action<JsonObject> traLoi)
    {
        JsonObject kq;
        lock (khoaPc)
        {
            if (!nguoi.TryGetValue(ten, out var ng)) kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "chua_xep_hang", ["ly_do"] = "chưa vào hàng chờ" };
            else if (ng.TrangThai == "cho")
            {
                ng.Cho.Add(traLoi);
                return;
            }
            else if (ng.TrangThai == "luot")
                kq = new JsonObject { ["ok"] = true, ["luot"] = true, ["so"] = ng.So, ["han_ms"] = Math.Max(0L, (long)Math.Round((ng.Han!.Value - Chung.Now) * 1000)) };
            else if (ng.TrangThai == "da_mua") kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "da_mua", ["ly_do"] = "lượt này đã dùng để mua" };
            else if (ng.TrangThai == "het_han") kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "het_luot", ["ly_do"] = "lượt mua đã hết hạn" };
            else kq = KqHetVe();
        }
        traLoi(kq);
    }

    void Mua(string ten, JsonObject tin, Action<JsonObject> traLoi)
    {
        long veToiDa = TsL("ve_toi_da");
        if (!Chung.LaSoNguyen(tin["so_ve"], out long soVe) || soVe < 1 || soVe > veToiDa)
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "sai_du_lieu", ["ly_do"] = $"số vé 1–{veToiDa}" });
            return;
        }
        bool qt;
        lock (khoaPc)                                    // giới hạn số yêu cầu mua đang xử lý cùng lúc
        {
            qt = dangXuLy >= TsL("gioi_han_xu_ly");
            if (qt) quaTai++;
            else
            {
                dangXuLy++;
                xuLyMax = Math.Max(xuLyMax, dangXuLy);
            }
        }
        if (qt)
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "qua_tai", ["ly_do"] = "hệ thống quá tải, vui lòng thử lại" });
            return;
        }
        bool pc = TsL("phong_cho") == 1;
        long gia = TsL("gia_ve");
        string oLuot = $"luot:{ten}";                    // ô lượt của người này (tạo khi được cấp lượt)

        (JsonObject, Dictionary<string, object>?) XuLy(IReadOnlyDictionary<string, object> doc)   // kiểm tra lượt + số vé còn; trừ kho VÀ tiêu lượt
        {
            var k = (KhoVe)doc["kho"];
            Luot? luot = null;
            if (pc)
            {
                luot = (Luot)doc[oLuot];
                if (!luot.Con) return (new JsonObject { ["ok"] = false, ["ma_loi"] = "da_mua", ["ly_do"] = "lượt này đã dùng để mua" }, null);
                if (Chung.Now > luot.Han) return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_luot", ["ly_do"] = "lượt mua đã hết hạn" }, null);
            }
            if (k.Con <= 0)
            {
                var het = KqHetVe();
                het["con"] = 0;
                return (het, null);
            }
            if (k.Con < soVe) return (new JsonObject { ["ok"] = false, ["ma_loi"] = "het_ve", ["ly_do"] = $"chỉ còn {k.Con} vé", ["con"] = k.Con }, null);
            var ghi = new Dictionary<string, object> { ["kho"] = new KhoVe(k.Con - soVe, k.DaBan + soVe) };
            if (luot != null) ghi[oLuot] = luot with { Con = false };
            return (new JsonObject { ["ok"] = true, ["so_ve"] = soVe, ["thanh_tien"] = soVe * gia, ["con"] = k.Con - soVe }, ghi);
        }

        void KhiGhi(JsonObject kq)                       // trong vùng găng: tiêu lượt, ghi sổ
        {
            long ma = Interlocked.Increment(ref maDon);
            kq["ma_don"] = ma;
            if (pc)
                lock (khoaPc)
                    if (nguoi.TryGetValue(ten, out var ng)) ng.TrangThai = "da_mua";
            So.Ghi(new GdPhongCho("mua", ten, SoVe: soVe, CanLuot: pc));
            lock (khoaPc)
            {
                soDon++;
                lichSu.AddFirst(new JsonObject { ["ma"] = ma, ["gio"] = DateTime.Now.ToString("HH:mm:ss"), ["nguoi"] = ten, ["so_ve"] = soVe });
                while (lichSu.Count > 30) lichSu.RemoveLast();
            }
            BaoDoi();
        }

        void Tra(JsonObject kq)
        {
            lock (khoaPc) dangXuLy--;
            if (Chung.Chuoi(kq, "ma_loi") == "khong_co")  // chưa có ô lượt: chưa được cấp lượt
                kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_luot", ["ly_do"] = "chưa tới lượt – hãy vào hàng chờ" };
            traLoi(kq);
        }

        Kho.ThucHien(pc ? ["kho", oLuot] : ["kho"], XuLy, Tra, KhiGhi);
    }

    // ------------------------------------------------------------ luồng nền (100 ms): thùng token
    protected override void DinhKy(double bayGio)
    {
        if (!Kho.Co("kho")) return;                      // đang đặt lại
        var k = (KhoVe)Kho.Doc("kho");
        var tha = new List<(Action<JsonObject> Cb, JsonObject Kq)>();
        bool baoHet = false;
        long tongVe = TsL("tong_ve");
        lock (khoaPc)
        {
            while (coLuot.Count > 0 && coLuot.Peek().Han <= bayGio)   // thu hồi lượt quá hạn
            {
                var (han, ten) = coLuot.Dequeue();
                if (nguoi.TryGetValue(ten, out var ng) && ng.TrangThai == "luot" && ng.Han == han)
                {
                    ng.TrangThai = "het_han";
                    hetHan++;
                }
            }
            if (k.Con <= 0 && !hetVe)                    // hết vé: báo mọi người đang chờ, đóng hàng
            {
                hetVe = baoHet = true;
                foreach (var ng in nguoi.Values.Where(ng => ng.TrangThai == "cho"))
                {
                    ng.TrangThai = "het_ve";
                    tha.AddRange(ng.Cho.Select(cb => (cb, KqHetVe())));
                    ng.Cho = [];
                }
                heap = new();
                dangCho = 0;
            }
            else if (TsL("phong_cho") == 1 && !hetVe)
            {
                double tocDo = TsD("toc_do"), luotS = TsD("luot_s");
                token = Math.Min(TsL("dung_luong"), token + tocDo * (bayGio - tToken));
                while (heap.Count > 0 && token >= 1)
                {
                    heap.TryDequeue(out var ten, out var uu);
                    if (!nguoi.TryGetValue(ten!, out var ng) || ng.TrangThai != "cho" || ng.So != uu.So) continue;
                    token -= 1;
                    dangCho--;
                    So.Ghi(new GdPhongCho("luot", ten!));            // sổ biết lượt TRƯỚC khi người đó thấy lượt
                    (ng.TrangThai, ng.Han) = ("luot", bayGio + luotS);
                    Kho.DatO($"luot:{ten}", new Luot(true, ng.Han.Value));   // ô mới: chưa ai chạm tới
                    coLuot.Enqueue((ng.Han.Value, ten!));
                    dangPhucVu = Math.Max(dangPhucVu, uu.So);
                    daCapLuot++;
                    foreach (var cb in ng.Cho)
                        tha.Add((cb, new JsonObject { ["ok"] = true, ["luot"] = true, ["so"] = uu.So, ["han_ms"] = (long)Math.Round(luotS * 1000) }));
                    ng.Cho = [];
                }
            }
            tToken = bayGio;
        }
        if (baoHet) ThongBao($"Đã bán hết {tongVe} vé");
        foreach (var (cb, kq) in tha) cb(kq);
        if (tha.Count > 0) BaoDoi();
    }

    // ------------------------------------------------------------ trạng thái gửi client
    protected override JsonObject TrangThaiNghiepVu()
    {
        var k = Kho.Co("kho") ? (KhoVe)Kho.Doc("kho") : new KhoVe(0, 0);
        long soCap = Kho.Co("hang") ? ((HangCho)Kho.Doc("hang")).Cuoi : 0;
        var d = new JsonObject
        {
            ["su_kien"] = new JsonObject
            {
                ["ten"] = TsS("ten_su_kien"), ["gia_ve"] = TsL("gia_ve"), ["tong_ve"] = TsL("tong_ve"), ["ve_toi_da"] = TsL("ve_toi_da"), ["luot_s"] = TsD("luot_s"),
            },
            ["phong_cho"] = TsL("phong_cho") == 1, ["con_lai"] = k.Con, ["da_ban"] = k.DaBan, ["toc_do"] = TsD("toc_do"), ["dung_luong"] = TsL("dung_luong"),
        };
        lock (khoaPc)
        {
            d["token"] = (long)token;
            d["dang_phuc_vu"] = dangPhucVu;
            d["so_cap"] = soCap;
            d["dang_cho"] = dangCho;
            d["da_cap_luot"] = daCapLuot;
            d["het_han"] = hetHan;
            d["het_ve"] = hetVe;
            d["dang_xu_ly"] = dangXuLy;
            d["xu_ly_max"] = xuLyMax;
            d["gioi_han"] = TsL("gioi_han_xu_ly");
            d["qua_tai"] = quaTai;
            d["so_don"] = soDon;
            d["ban_so"] = SoPc.DaBan;
            d["lich_su"] = new JsonArray([.. lichSu.Take(10).Select(x => x.DeepClone())]);
        }
        return d;
    }

    protected override string TomTat()
    {
        long con = Kho.Co("kho") ? ((KhoVe)Kho.Doc("kho")).Con : 0, don;
        lock (khoaPc) don = soDon;
        return $"Sổ: bán {SoPc.DaBan}/{TsL("tong_ve")} vé, {don} đơn; kho còn {con}";
    }
}
