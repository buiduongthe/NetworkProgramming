using System.Drawing;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_8;

// Bài mở rộng MR6.8 – Ứng dụng đặt vé xem phim (người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// ứng dụng đặt vé: áp phích phim (GDI+), sơ đồ phòng chiếu 8 × 12 ghế với MÀN HÌNH, lối đi, nhãn hàng A–H; màu ghế:
// trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn. Bấm ghế để chọn, "GIỮ CHỖ" gửi giu_cho, đồng
// hồ giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả ghế". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/
// giây) khi người khác giữ/mua ghế; ghế đang chọn bị người khác giữ thì tự bỏ chọn và báo.

/// <summary>Sơ đồ ghế (Control tự vẽ); dùng ở ứng dụng đặt vé (ô = 30 px) và bảng điều khiển máy chủ (ô nhỏ).</summary>
sealed class SoDoGhe : Control
{
    public static readonly Color Trong = GD.Hex("#ffffff"), Chon = GD.Hex("#b9f6ca"), KhacGiu = GD.Hex("#ffa726"), DaBan = GD.Hex("#9e9e9e"),
                                 ToiGiu = GD.Hex("#2e7d32"), VeToi = GD.Hex("#1565c0");
    public static readonly Color NenRap = GD.Hex("#1c1c24");

    readonly string hang;
    readonly float o, le, tren;
    readonly bool chu;
    public readonly float RongSoDo, CaoSoDo;
    readonly Dictionary<string, RectangleF> viTri = [];
    Dictionary<string, Color> mauGhe = [];
    Dictionary<string, Color> vien = [];
    public Action<Graphics>? VeThem;                     // vẽ thêm (chú giải màu) sau sơ đồ

    public SoDoGhe(int x, int y, int w, int h, string hang, int cot, float o = 30, float khe = 4, float le = 24, float tren = 46,
                   int[]? loiDi = null, bool chu = true)
    {
        loiDi ??= [2, 10];
        (this.hang, this.o, this.le, this.tren, this.chu) = (hang, o, le, tren, chu);
        RongSoDo = le * 2 + cot * (o + khe) + loiDi.Length * o * 0.5f;
        CaoSoDo = tren + hang.Length * (o * 0.86f + khe) + 4;
        for (int i = 0; i < hang.Length; i++)
        {
            float yy = tren + i * (o * 0.86f + khe);
            for (int c = 1; c <= cot; c++)
            {
                float xx = le + (c - 1) * (o + khe) + loiDi.Count(a => c > a) * o * 0.5f;
                viTri[$"{hang[i]}{c}"] = new RectangleF(xx, yy, o, o * 0.86f);
            }
        }
        DoubleBuffered = true;
        (Location, Size, BackColor) = (new Point(x, y), new Size(w, h), NenRap);
    }

    /// <summary>mauGhe: {ghế: màu}; vien: {ghế: màu viền} (ghế nóng ở máy chủ).</summary>
    public void Ve(Dictionary<string, Color> mau, Dictionary<string, Color>? vienMoi = null)
    {
        (mauGhe, vien) = (mau, vienMoi ?? []);
        Invalidate();
    }

    public string? GheTai(float x, float y) => viTri.FirstOrDefault(v => v.Value.Contains(x, y)).Key;

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        g.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.AntiAlias;
        float w = RongSoDo, lech = tren * 0.55f;
        using (var p = new Pen(GD.Hex("#e8e8f0"), chu ? 4 : 2))
            g.DrawCurve(p, [new PointF(le, lech), new PointF(w * 0.5f, lech * 0.35f), new PointF(w - le, lech)]);
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        if (chu)
        {
            using var f = GD.Chu(8, true);
            using var b = new SolidBrush(GD.Hex("#b0b0c0"));
            g.DrawString("MÀN HÌNH", f, b, w / 2, lech + 10, sf);
            using var f2 = GD.Chu(9, true);
            using var b2 = new SolidBrush(GD.Hex("#c0c0d0"));
            foreach (char r in hang)
            {
                var o1 = viTri[$"{r}1"];
                float yy = (o1.Top + o1.Bottom) / 2;
                g.DrawString(r.ToString(), f2, b2, le / 2, yy, sf);
                g.DrawString(r.ToString(), f2, b2, w - le / 2, yy, sf);
            }
        }
        using var so = GD.Chu(7);
        foreach (var (ma, rc) in viTri)
        {
            var mau = mauGhe.GetValueOrDefault(ma, Trong);
            using (var b = new SolidBrush(mau)) g.FillRectangle(b, rc);
            bool coVien = vien.TryGetValue(ma, out var mv);
            using (var p = new Pen(coVien ? mv : GD.Hex("#606070"), coVien ? 2 : 1)) g.DrawRectangle(p, rc.X, rc.Y, rc.Width, rc.Height);
            if (!chu) continue;
            using (var b = new SolidBrush(GD.Hex("#707080")))       // tay vịn: vạch tối ở chân ghế
                g.FillRectangle(b, RectangleF.FromLTRB(rc.Left + 2, rc.Bottom - 4, rc.Right - 2, rc.Bottom - 1));
            bool toi = mau == ToiGiu || mau == VeToi || mau == DaBan;
            using (var b = new SolidBrush(toi ? Color.White : GD.Hex("#303040")))
                g.DrawString(ma[1..], so, b, (rc.Left + rc.Right) / 2, (rc.Top + rc.Bottom) / 2 - 2, sf);
            if (mau == DaBan)
                using (var p = new Pen(GD.Hex("#e0e0e0"))) g.DrawLine(p, rc.Left + 4, rc.Top + 3, rc.Right - 4, rc.Bottom - 6);
        }
        VeThem?.Invoke(g);
    }
}

sealed class FormDatVe(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    static readonly (Color Mau, string Chu)[] ChuGiai =
        [(SoDoGhe.Trong, "Trống"), (SoDoGhe.Chon, "Đang chọn"), (SoDoGhe.KhacGiu, "Người khác giữ"), (SoDoGhe.DaBan, "Đã bán"),
         (SoDoGhe.ToiGiu, "Bạn giữ"), (SoDoGhe.VeToi, "Vé của bạn")];
    static readonly Color DoRap = GD.Hex("#e71a0f");
    protected override string TieuDe => "Đặt vé xem phim";
    protected override Size KichThuocKhung => new(486 + 8 + 266, 364);

    public readonly HashSet<string> DangChon = [];       // ghế đang chọn (chưa giữ)
    public HashSet<string> Giu = [];                     // ghế mình đang giữ (theo kq)
    public readonly HashSet<string> VeCuaToi = [];       // vé của mình
    long phienGiu, phienVe;                              // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    double? han;                                         // thời điểm (đồng hồ đơn điệu) hết hạn giữ chỗ
    public int SoGiu;
    int soVe;
    string phimCu = "";
    SoDoGhe soDo = null!;
    OVe apPhich = null!;
    JsonObject? ttPhim;
    Label lblGhe = null!, lblTien = null!, lblVe = null!;
    public Label LblDongHo = null!, LblThongBao = null!;
    public Button BtnGiu = null!, BtnTt = null!, BtnNha = null!;

    /// <summary>Đồng hồ giữ chỗ mm:ss (làm tròn lên giây).</summary>
    static string DongHo(double giay)
    {
        int s = Math.Max(0, (int)(giay + 0.999));
        return $"{s / 60:00}:{s % 60:00}";
    }

    protected override void Ve(Panel khung)
    {
        soDo = new SoDoGhe(0, 4, 486, 322, PhongChieu.Hang, PhongChieu.Cot);
        soDo.VeThem = g =>                               // chú giải màu
        {
            float y = soDo.CaoSoDo + 14, x = 10;
            using var f = GD.Chu(8);
            using var b = new SolidBrush(GD.Hex("#d0d0e0"));
            using var p = new Pen(GD.Hex("#606070"));
            foreach (var (mau, chu) in ChuGiai)
            {
                using (var bm = new SolidBrush(mau)) g.FillRectangle(bm, x, y, 14, 12);
                g.DrawRectangle(p, x, y, 14, 12);
                g.DrawString(chu, f, b, x + 18, y - 1);
                x += 22 + chu.Length * 6;
            }
        };
        soDo.MouseDown += (_, e) =>
        {
            if (soDo.GheTai(e.X, e.Y) is { } g) Bam(g);
        };
        var phai = new Panel { Location = new Point(494, 4), Size = new Size(266, 356), BackColor = Color.White, BorderStyle = BorderStyle.FixedSingle };
        apPhich = new OVe(6, 6, 250, 96) { BackColor = GD.Hex("#2b1640"), Ve = VePhim };
        Label N(string t, int y, Font f, int h = 18) => new() { Text = t, Location = new Point(8, y), Size = new Size(248, h), Font = f, BackColor = Color.White, AutoEllipsis = true };
        lblGhe = N("Ghế đã chọn: –", 106, GD.Chu(10, true), 20);
        lblTien = N("Tạm tính: 0 đ", 126, GD.Chu(9));
        Button NutLon(string t, Color mau, int y) => new()
        {
            Text = t, Location = new Point(8, y), Size = new Size(248, 32), BackColor = mau, ForeColor = Color.White, Font = GD.Chu(11, true),
            FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false,
        };
        BtnGiu = NutLon("GIỮ CHỖ", GD.Hex("#ff8f00"), 146);
        BtnGiu.Click += (_, _) => GiuCho();
        var nhanCon = N("Giữ chỗ còn", 190, GD.Chu(9));
        nhanCon.Width = 80;
        LblDongHo = new Label { Text = "--:--", Location = new Point(90, 182), Size = new Size(120, 30), Font = GD.ChuCoDinh(18, true), ForeColor = GD.Hex("#909090"), BackColor = Color.White };
        BtnTt = NutLon("THANH TOÁN", DoRap, 214);
        BtnTt.Click += (_, _) => ThanhToan();
        BtnNha = GD.Nut("Nhả ghế", GD.Hex("#f0f0f0"), 8, 250, 90, 26);
        BtnNha.Font = GD.Thuong;
        BtnNha.Click += (_, _) => Nha();
        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(8, 280), Size = new Size(248, 36), Font = GD.Chu(9, true), BackColor = GD.Hex("#e8e8e8"),
            TextAlign = ContentAlignment.MiddleCenter,
        };
        lblVe = N("Vé của bạn: –", 318, GD.Chu(9), 34);
        lblVe.ForeColor = SoDoGhe.VeToi;
        foreach (var b in new[] { BtnGiu, BtnTt }) b.FlatAppearance.BorderSize = 0;
        phai.Controls.AddRange([apPhich, lblGhe, lblTien, BtnGiu, nhanCon, LblDongHo, BtnTt, BtnNha, LblThongBao, lblVe]);
        GD.Them(khung, soDo, phai);
        VeGhe();
        BatNut();
    }

    // ------------------------------------------------------------ áp phích, sơ đồ
    void VePhim(Graphics g)
    {
        for (int i = 0; i < 12; i++)                     // nền chuyển màu tím -> xanh
        {
            using var b = new SolidBrush(Color.FromArgb(43 + i * 2, 22 + i * 6, 64 + i * 9));
            g.FillRectangle(b, 0, i * 8, 250, 8);
        }
        using (var b = new SolidBrush(GD.Hex("#4fc3f7"))) g.FillEllipse(b, 196, 8, 46, 46);
        using (var p = new Pen(GD.Hex("#b3e5fc"), 2)) g.DrawEllipse(p, 196, 8, 46, 46);
        using (var b = new SolidBrush(GD.Hex("#43a047"))) g.FillPie(b, 196, 8, 46, 46, 20, 140);
        for (int k = 0; k < 14; k++) g.FillEllipse(Brushes.White, k * 37 % 170 + 6, k * 23 % 60 + 6, 2, 2);
        if (ttPhim is not { } tt) return;
        using var f1 = GD.Chu(12, true);
        using var f2 = GD.Chu(8);
        using var f3 = GD.Chu(8, true);
        using var sf = new StringFormat { LineAlignment = StringAlignment.Center };
        g.DrawString((Chung.Chuoi(tt, "phim") ?? "").ToUpperInvariant(), f1, Brushes.White, 10, 40, sf);
        using (var b = new SolidBrush(GD.Hex("#e0d8f0"))) g.DrawString($"2D Phụ đề · Suất {Chung.Chuoi(tt, "suat")} · Phòng 3", f2, b, 10, 66, sf);
        using (var b = new SolidBrush(GD.Hex("#ffd54f")))
            g.DrawString($"{Chung.Tien(Chung.SoL(tt, "gia_ve"))}/ghế · giữ chỗ {Chung.G(Chung.SoD(tt, "ttl_s"))} s", f3, b, 10, 82, sf);
    }

    void VeGhe()
    {
        var mau = new Dictionary<string, Color>();
        if (Tt is { } tt)
        {
            string hang = Chung.Chuoi(tt, "hang") ?? "";
            var chu = tt["chu"] as JsonObject;
            var dong = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
            for (int i = 0; i < Math.Min(hang.Length, dong.Count); i++)
                for (int c = 0; c < dong[i].Length; c++)
                {
                    string g = $"{hang[i]}{c + 1}";
                    bool cuaToi = Chung.Chuoi(chu, g) == Ten;
                    if (dong[i][c] == 'b') mau[g] = cuaToi ? SoDoGhe.VeToi : SoDoGhe.DaBan;
                    else if (dong[i][c] == 'g') mau[g] = cuaToi ? SoDoGhe.ToiGiu : SoDoGhe.KhacGiu;
                }
        }
        foreach (var g in DangChon) mau.TryAdd(g, SoDoGhe.Chon);
        soDo.Ve(mau);
    }

    /// <summary>"trong" | "giu" (người khác) | "ban" | "toi_giu" | "ve" theo cap_nhat mới nhất.</summary>
    string TrangGhe(string g)
    {
        if (Tt is not { } tt) return "trong";
        string hang = Chung.Chuoi(tt, "hang") ?? "";
        int r = hang.IndexOf(g[0]), c = int.Parse(g[1..]);
        var dong = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        char x = r >= 0 && r < dong.Count && c - 1 < dong[r].Length ? dong[r][c - 1] : '.';
        bool cuaToi = Chung.Chuoi(tt["chu"] as JsonObject, g) == Ten;
        return x switch { 'g' => cuaToi ? "toi_giu" : "giu", 'b' => cuaToi ? "ve" : "ban", _ => "trong" };
    }

    /// <summary>Bấm một ghế (người dùng hoặc kịch bản demo).</summary>
    public void Bam(string g)
    {
        if (Kn == null) return;
        string trang = TrangGhe(g);
        if (DangChon.Contains(g)) DangChon.Remove(g);
        else if (trang == "trong" && !Giu.Contains(g))
        {
            if (DangChon.Count + Giu.Count >= 8)
            {
                Bao("Tối đa 8 ghế mỗi lần", GD.Hex("#ffe0b2"));
                return;
            }
            DangChon.Add(g);
        }
        else if (trang == "giu") Bao($"Ghế {g} đang có người giữ", GD.Hex("#ffe0b2"));
        else if (trang == "ban") Bao($"Ghế {g} đã bán", GD.Hex("#e0e0e0"));
        CapNhatGio();
    }

    static string Ds(IEnumerable<string> ds) => string.Join(", ", ds.Order(StringComparer.Ordinal));

    void CapNhatGio()
    {
        long gia = Tt != null ? Chung.SoL(Tt, "gia_ve") : 0;
        if (Giu.Count > 0)
        {
            lblGhe.Text = $"Đang giữ: {Ds(Giu)}";
            lblTien.Text = $"Tổng: {Chung.Tien(gia * Giu.Count)}";
            BtnTt.Text = $"THANH TOÁN {Chung.Tien(gia * Giu.Count)}";
        }
        else
        {
            lblGhe.Text = $"Ghế đã chọn: {(DangChon.Count > 0 ? Ds(DangChon) : "–")}";
            lblTien.Text = $"Tạm tính: {Chung.Tien(gia * DangChon.Count)}";
            BtnTt.Text = "THANH TOÁN";
        }
        BatNut();
        VeGhe();
    }

    void BatNut()
    {
        bool co = Kn != null, conHan = han is { } h && Chung.Now < h;
        bool giuDuoc = co && DangChon.Count > 0 && Giu.Count == 0, ttDuoc = co && Giu.Count > 0 && conHan;
        (BtnGiu.Enabled, BtnGiu.BackColor) = (giuDuoc, giuDuoc ? GD.Hex("#ff8f00") : GD.Hex("#ffcc80"));
        (BtnTt.Enabled, BtnTt.BackColor) = (ttDuoc, ttDuoc ? DoRap : GD.Hex("#ef9a9a"));
        BtnNha.Enabled = co && Giu.Count > 0;
    }

    void Bao(string text, Color nen, Color? chu = null) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu ?? Color.Black);

    // ------------------------------------------------------------ giữ / thanh toán / nhả
    void GiuCho()
    {
        if (DangChon.Count == 0 || Giu.Count > 0) return;
        var ghe = DangChon.Order(StringComparer.Ordinal).ToList();
        NhatKy.Ghi($"Giữ chỗ {string.Join(", ", ghe)}…");
        Bao("Đang giữ chỗ…", GD.Hex("#fff3c4"));
        BtnGiu.Enabled = false;
        GuiYc("giu_cho", new JsonObject { ["ghe"] = PhongChieu.Json(ghe) });
    }

    void ThanhToan()
    {
        if (Giu.Count == 0) return;
        NhatKy.Ghi($"Thanh toán {Ds(Giu)}…");
        GuiYc("thanh_toan", new JsonObject { ["ghe"] = PhongChieu.Json(Giu.Order(StringComparer.Ordinal)) });
    }

    void Nha()
    {
        if (Giu.Count > 0) GuiYc("nha", new JsonObject { ["ghe"] = PhongChieu.Json(Giu.Order(StringComparer.Ordinal)) });
    }

    static List<string> DsJson(JsonNode? n) => [.. (n as JsonArray ?? []).Select(Chung.ChuoiPy)];

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        var ghe = DsJson(truong["ghe"]);
        bool ok = Chung.Dung(tin, "ok");
        if (loai == "giu_cho")
        {
            if (ok)
            {
                SoGiu++;
                (Giu, phienGiu) = ([.. DsJson(tin["ghe"])], Chung.SoL(tin, "phien"));
                DangChon.ExceptWith(Giu);
                double hetMs = Chung.SoD(tin, "het_han_ms");
                han = Chung.Now + hetMs / 1000;
                Bao($"Đã giữ {string.Join(", ", ghe)} – thanh toán trong {Chung.F(hetMs / 1000, "F0")} s", GD.Hex("#c8f0d2"), GD.XanhDam);
                NhatKy.Ghi($"  ĐÃ GIỮ {string.Join(", ", ghe)} ({Chung.F(rtt, "F1")} ms)");
                KiemGhe(Tt);
            }
            else
            {
                DangChon.ExceptWith(tin["ghe_ban"] is JsonArray ? DsJson(tin["ghe_ban"]) : ghe);
                Bao($"Không giữ được: {Chung.Chuoi(tin, "ly_do")}", GD.Hex("#ffd0d0"), GD.DoDam);
                NhatKy.Ghi($"  không giữ được: {Chung.Chuoi(tin, "ly_do")}");
            }
        }
        else if (loai == "thanh_toan")
        {
            if (ok)
            {
                soVe += ghe.Count;
                VeCuaToi.UnionWith(ghe);
                phienVe = Chung.SoL(tin, "phien");
                Giu.ExceptWith(ghe);
                han = null;
                Bao($"Đặt vé thành công! {Chung.Tien(Chung.SoL(tin, "tien"))}", GD.Hex("#c8f0d2"), GD.XanhDam);
                NhatKy.Ghi($"  ĐẶT VÉ THÀNH CÔNG: {string.Join(", ", DsJson(tin["ma_ve"]))} ({Chung.F(rtt, "F1")} ms)");
            }
            else
            {
                Bao($"Thanh toán bị từ chối: {Chung.Chuoi(tin, "ly_do")}", GD.Hex("#ff6060"), Color.White);
                NhatKy.Ghi($"  thanh toán bị từ chối: {Chung.Chuoi(tin, "ly_do")}");
            }
        }
        else
        {
            if (ok)
            {
                var da = DsJson(tin["ghe"]);
                Giu.ExceptWith(da);
                if (Giu.Count == 0) han = null;
                Bao($"Đã nhả {string.Join(", ", da)}", GD.Hex("#e8e8e8"));
                NhatKy.Ghi($"  đã nhả {string.Join(", ", da)}");
            }
            else NhatKy.Ghi($"  nhả bị từ chối: {Chung.Chuoi(tin, "ly_do")}");
        }
        VeLaiVe();
        CapNhatGio();
    }

    void VeLaiVe() =>
        lblVe.Text = $"Vé của bạn: {(VeCuaToi.Count > 0 ? Ds(VeCuaToi) : "–")}\nBạn: giữ được {SoGiu} lần, mua {soVe} vé";

    /// <summary>Ghế mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; ghế đang chọn bị người khác giữ.
    /// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).</summary>
    void KiemGhe(JsonObject? tt)
    {
        if (tt == null) return;
        var chu = tt["chu"] as JsonObject;
        long phien = Chung.SoL(tt, "phien");
        if (phien >= phienGiu)
        {
            foreach (var g in Giu.Order(StringComparer.Ordinal).ToList())
            {
                string? ai = Chung.Chuoi(chu, g);
                if (ai == Ten) continue;
                Giu.Remove(g);
                if (ai != null)
                {
                    Bao($"Ghế {g} đã bị {ai} lấy mất!", GD.Hex("#ff6060"), Color.White);
                    NhatKy.Ghi($"Ghế {g} đã bị {ai} lấy mất!");
                }
                else
                {
                    Bao("Hết thời gian giữ chỗ – ghế đã được nhả", GD.Hex("#ffd0d0"), GD.DoDam);
                    NhatKy.Ghi($"Hết thời gian giữ chỗ: {g}");
                }
            }
            if (Giu.Count == 0) han = null;
        }
        if (phien >= phienVe)
            foreach (var g in VeCuaToi.Order(StringComparer.Ordinal).ToList())
            {
                string? ai = Chung.Chuoi(chu, g);
                if (ai == Ten) continue;
                VeCuaToi.Remove(g);
                Bao($"Vé {g} bị ghi cho {ai ?? "không ai"}!", GD.Hex("#ff6060"), Color.White);
                NhatKy.Ghi($"Vé {g} bị ghi cho {ai ?? "không ai"}!");
            }
        var biLay = DangChon.Where(g => TrangGhe(g) != "trong").Order(StringComparer.Ordinal).ToList();
        DangChon.ExceptWith(biLay);
        if (biLay.Count > 0)
        {
            Bao($"Ghế {string.Join(", ", biLay)} vừa có người giữ", GD.Hex("#ffe0b2"));
            NhatKy.Ghi($"Ghế {string.Join(", ", biLay)} vừa có người giữ");
        }
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        string phim = $"{Chung.Chuoi(tt, "phim")}|{Chung.SoD(tt, "ttl_s")}|{Chung.SoL(tt, "gia_ve")}|{Chung.Chuoi(tt, "suat")}";
        if (phim != phimCu)
        {
            (ttPhim, phimCu) = (tt, phim);
            apPhich.Invalidate();
        }
        if (LblThongBao.Text == "Chưa kết nối") Bao("Chọn ghế rồi bấm GIỮ CHỖ", GD.Hex("#e8e8e8"));
        var chu = tt["chu"] as JsonObject ?? [];
        var dem = tt["dem"] as JsonObject;
        if (!chu.Any(x => !Chung.LaNull(x.Value)) && (VeCuaToi.Count > 0 || Giu.Count > 0) && Chung.SoL(dem, "ban") == 0 && Chung.SoL(dem, "giu") == 0)
            VeCuaToi.Clear();                            // máy chủ đặt lại: rạp trống
        var conLai = tt["con_lai_ms"] as JsonObject;
        var cuaToi = Giu.Where(g => Chung.Chuoi(chu, g) == Ten && conLai?[g] is not null).Select(g => Chung.SoD(conLai, g)).ToList();
        if (cuaToi.Count > 0) han = Chung.Now + cuaToi.Min() / 1000;
        KiemGhe(tt);
        VeLaiVe();
        CapNhatGio();
    }

    protected override void MoiNhip()
    {
        if (han is not { } h)
        {
            (LblDongHo.Text, LblDongHo.ForeColor) = ("--:--", GD.Hex("#909090"));
            return;
        }
        double con = h - Chung.Now;
        (LblDongHo.Text, LblDongHo.ForeColor) = (DongHo(con), con < 10 ? GD.DoDam : GD.XanhDam);
        if (con <= 0 && BtnTt.Enabled)
        {
            BatNut();
            Bao("Hết thời gian giữ chỗ", GD.Hex("#ffd0d0"), GD.DoDam);
        }
    }

    protected override string DongDinhKy()
    {
        var d = Tt!["dem"] as JsonObject;
        string con = han is { } h ? $"{Chung.F(Math.Max(0, h - Chung.Now), "F0")} s" : "–";
        return $"trống {Chung.SoL(d, "trong")}, giữ {Chung.SoL(d, "giu")}, bán {Chung.SoL(d, "ban")}; bạn giữ"
               + $" {(Giu.Count > 0 ? Ds(Giu) : "–")} (còn {con}), vé {(VeCuaToi.Count > 0 ? Ds(VeCuaToi) : "–")}";
    }
}
