using System.Drawing;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_14;

// Bài mở rộng MR6.14 – Ứng dụng đặt vé xe khách theo chặng (người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// ứng dụng đặt vé: băng rôn nhà xe, chọn "Đi từ" – "Đến" (thanh tuyến Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang tô đoạn
// đã chọn, giá từng chặng), sơ đồ xe (giường nằm 2 tầng hoặc ghế ngồi 50 chỗ, có vô lăng ở đầu xe). Mỗi chỗ tô màu
// theo ĐOẠN đang chọn: trống / đang chọn / người khác giữ / đã bán / bạn giữ / vé của bạn, và có dải nhỏ ở chân chỗ,
// mỗi chặng một vạch, cho thấy chỗ đó bận ở chặng nào. Bấm chỗ để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm
// ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả chỗ". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây).

/// <summary>Màu và hàm đọc trạng thái chỗ theo đoạn – dùng chung cho ứng dụng đặt vé và bảng điều khiển máy chủ.</summary>
static class XeGui
{
    public static readonly Color Trong = GD.Hex("#ffffff"), Chon = GD.Hex("#b9f6ca"), KhacGiu = GD.Hex("#ffa726"), DaBan = GD.Hex("#9e9e9e"),
                                 ToiGiu = GD.Hex("#2e7d32"), VeToi = GD.Hex("#1565c0");
    public static readonly Color MauXe = GD.Hex("#0b5d3b"), NenXe = GD.Hex("#e9efeb"), Vien = GD.Hex("#607d74"), ChuToi = GD.Hex("#263238");
    public static readonly Dictionary<char, Color> MauVach = new()
    {
        ['.'] = GD.Hex("#dfe6e2"), ['g'] = KhacGiu, ['b'] = DaBan, ['G'] = ToiGiu, ['V'] = VeToi,
    };

    static List<string> Ds(JsonNode? n) => [.. (n as JsonArray ?? []).Select(Chung.ChuoiPy)];

    /// <summary>Chuỗi SoChang ký tự của chỗ g trong so_do ('.' trống, 'g' giữ, 'b' bán – từng chặng).</summary>
    public static string DongCua(JsonObject tt, string g)
    {
        int i = Ds(tt["ghe"]).IndexOf(g);
        var sd = Ds(tt["so_do"]);
        return i < 0 || i >= sd.Count ? new string('.', XeKhach.SoChang) : sd[i];
    }

    /// <summary>Trạng thái chỗ g cho đoạn [tu, den): "trong" | "giu" | "ban" | "toi_giu" | "ve".</summary>
    public static string TrangCho(JsonObject tt, string g, string? ten, int tu, int den)
    {
        string dong = DongCua(tt, g), kq = "trong";
        for (int k = tu; k < den && k < dong.Length; k++)
        {
            char x = dong[k];
            bool toi = ten != null && NguoiDatVeXe.ChuO(tt, $"{g}#{k}") == ten;
            if (x == 'b' && !toi) return "ban";
            if (x == 'g' && !toi) kq = "giu";
            else if (x == 'g' && kq == "trong") kq = "toi_giu";
            else if (x == 'b' && kq is "trong" or "toi_giu") kq = "ve";
        }
        return kq;
    }

    /// <summary>Một ký tự mỗi chặng: . g b (người khác) G V (của mình).</summary>
    public static string Vach(JsonObject tt, string g, string? ten)
    {
        var b = DongCua(tt, g).ToCharArray();
        for (int k = 0; k < b.Length; k++)
            if (ten != null && NguoiDatVeXe.ChuO(tt, $"{g}#{k}") == ten) b[k] = b[k] switch { 'g' => 'G', 'b' => 'V', var c => c };
        return new string(b);
    }

    public static void ChuGiua(Graphics g, string s, Font f, Color mau, float x, float y)
    {
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        using var b = new SolidBrush(mau);
        g.DrawString(s, f, b, x, y, sf);
    }
}

/// <summary>Vẽ sơ đồ xe; dùng ở ứng dụng đặt vé (k = 1) và bảng điều khiển máy chủ (thu nhỏ, k &lt; 1).</summary>
sealed class SoDoXe
{
    public readonly string LoaiXe;
    readonly float k;
    readonly bool chu;
    public readonly float Rong, Cao;
    readonly Dictionary<string, RectangleF> viTri = [];
    readonly List<(float X0, float X1, string Ten)> tang = [];

    public SoDoXe(string loaiXe, float k, bool chu)
    {
        (LoaiXe, this.k, this.chu) = (loaiXe, k, chu);
        float w, h, khe;
        if (loaiXe == "giuong")
        {
            (w, h, khe) = (46 * k, 30 * k, 6 * k);
            float rongTang = 3 * w + 2 * khe;
            tang.Add((12 * k, 12 * k + rongTang, "TẦNG DƯỚI"));
            tang.Add((12 * k + rongTang + 26 * k, 12 * k + 2 * rongTang + 26 * k, "TẦNG TRÊN"));
            Rong = 24 * k + 2 * rongTang + 26 * k;
            Cao = 40 * k + 7 * (h + khe) + 4 * k;
        }
        else
        {
            (w, h, khe) = (34 * k, 26 * k, 5 * k);
            tang.Add((12 * k, 12 * k + 6 * (w + khe) - khe, ""));
            Rong = 24 * k + 6 * (w + khe) - khe;
            Cao = 40 * k + 12 * (h + khe) + 4 * k;
        }
        foreach (var x in XeKhach.BoTri(loaiXe))
        {
            float px = tang[loaiXe == "giuong" ? x.Tang : 0].X0 + x.Cot * (w + khe);
            float py = 40 * k + x.Hang * (h + khe);
            viTri[x.Ma] = new RectangleF(px, py, w, h);
        }
    }

    /// <summary>Thân xe (mỗi tầng một khung), tên tầng, vô lăng ở đầu xe.</summary>
    public void VeNen(Graphics g)
    {
        using var nen = new SolidBrush(GD.Hex("#f7faf8"));
        using var vien = new Pen(XeGui.MauXe, Math.Max(1, 2 * k));
        using var f = GD.Chu(7, true);
        foreach (var (x0, x1, ten) in tang)
        {
            var than = new RectangleF(x0 - 6 * k, 8 * k, x1 - x0 + 12 * k, Cao - 8 * k);
            g.FillRectangle(nen, than);
            g.DrawRectangle(vien, than.X, than.Y, than.Width, than.Height);
            if (chu && ten != "") XeGui.ChuGiua(g, ten, f, XeGui.MauXe, (x0 + x1) / 2, 20 * k);
        }
        if (chu || k >= 0.5f)                            // vô lăng ở đầu xe (bên trái, tầng dưới)
        {
            float r = 9 * k, cx = tang[0].X0 + r + 2 * k, cy = 22 * k;
            using var p = new Pen(XeGui.ChuToi, 1);
            g.DrawEllipse(p, cx - r, cy - r, 2 * r, 2 * r);
            g.DrawLine(p, cx - r, cy, cx + r, cy);
        }
    }

    /// <summary>Tô mọi chỗ theo đoạn [tu, den) và vẽ dải vạch theo chặng; vien: {chỗ: màu viền} (ghế nóng).</summary>
    public void Ve(Graphics g, JsonObject? tt, string? ten, int tu, int den, ISet<string>? chon, Dictionary<string, Color>? vien)
    {
        HashSet<string> ds = tt == null ? [] : (tt["ghe"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToHashSet();
        using var f = GD.Chu(7);
        using var butVien = new Pen(XeGui.Vien, 1);
        using var butToi = new Pen(XeGui.ChuToi, 1);
        float caoVach = Math.Max(2, 5 * k);
        foreach (var (ma, r) in viTri)
        {
            string trang = "trong", vc = new('.', XeKhach.SoChang);
            if (tt != null && ds.Contains(ma))
            {
                trang = XeGui.TrangCho(tt, ma, ten, tu, den);
                vc = XeGui.Vach(tt, ma, ten);
            }
            var mau = trang switch
            {
                "giu" => XeGui.KhacGiu, "ban" => XeGui.DaBan, "toi_giu" => XeGui.ToiGiu, "ve" => XeGui.VeToi,
                _ => chon != null && chon.Contains(ma) ? XeGui.Chon : XeGui.Trong,
            };
            using (var b = new SolidBrush(mau)) g.FillRectangle(b, r);
            if (vien != null && vien.TryGetValue(ma, out var mv))
                using (var p = new Pen(mv, 2)) g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
            else g.DrawRectangle(butVien, r.X, r.Y, r.Width, r.Height);
            for (int i = 0; i < vc.Length; i++)          // dải vạch: mỗi chặng một ô ở chân chỗ
            {
                float a = r.Left + 2 + (r.Width - 4) * i / vc.Length, b2 = r.Left + 2 + (r.Width - 4) * (i + 1) / vc.Length - 1;
                var v = new RectangleF(a, r.Bottom - caoVach - 1, b2 - a, caoVach);
                using (var b = new SolidBrush(XeGui.MauVach.GetValueOrDefault(vc[i], XeGui.Trong))) g.FillRectangle(b, v);
                if (tu <= i && i < den) g.DrawRectangle(butToi, v.X, v.Y, v.Width, v.Height);
            }
            if (chu)
            {
                bool toi = mau == XeGui.ToiGiu || mau == XeGui.VeToi || mau == XeGui.DaBan;
                XeGui.ChuGiua(g, ma, f, toi ? Color.White : XeGui.ChuToi, (r.Left + r.Right) / 2, (r.Top + r.Bottom) / 2 - 3 * k);
            }
        }
    }

    public string? ChoTai(float x, float y) => viTri.FirstOrDefault(v => v.Value.Contains(x, y)).Key;
}

sealed record VeXe(string Ghe, int Tu, int Den, string Ma);

sealed class FormDatVeXe(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    static readonly (Color Mau, string Chu)[] ChuGiai =
        [(XeGui.Trong, "Trống"), (XeGui.Chon, "Đang chọn"), (XeGui.KhacGiu, "Người khác giữ"), (XeGui.DaBan, "Đã bán"),
         (XeGui.ToiGiu, "Bạn giữ"), (XeGui.VeToi, "Vé của bạn")];
    protected override string TieuDe => "Đặt vé xe khách";
    protected override Size KichThuocKhung => new(384 + 266, 484);

    public readonly SortedSet<string> DangChon = new(StringComparer.Ordinal);   // chỗ đang chọn (chưa giữ)
    public SortedSet<string> Giu = new(StringComparer.Ordinal);                 // chỗ mình đang giữ (theo kq) cho đoạn doanGiu
    (int Tu, int Den) doanGiu;
    public List<VeXe> VeCuaToi = [];                     // vé của mình
    long phienGiu, phienVe;                              // số thứ tự lần ghi (kq) của lần giữ chỗ / thanh toán gần nhất
    double? han;                                         // thời điểm (đồng hồ đơn điệu) hết hạn giữ chỗ
    public int SoGiu;
    int soVe;
    public int Tu, Den = XeKhach.SoChang;
    string bangRonCu = "";
    bool dangDatDoan;
    SoDoXe? soDo;
    OVe cv = null!, cvXe = null!, cvTuyen = null!;
    ComboBox chonTu = null!, chonDen = null!;
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
        cv = new OVe(0, 4, 376, 480) { BackColor = XeGui.NenXe, Ve = VeCho };
        cv.MouseDown += (_, e) =>
        {
            if (soDo?.ChoTai(e.X, e.Y) is { } g) Bam(g);
        };
        var phai = new Panel { Location = new Point(384, 4), Size = new Size(266, 456), BackColor = Color.White, BorderStyle = BorderStyle.FixedSingle };
        cvXe = new OVe(6, 6, 250, 62) { BackColor = XeGui.MauXe, Ve = VeBangRon };
        Label N(string t, int x, int y, int w, Font f, int h = 18) =>
            new() { Text = t, Location = new Point(x, y), Size = new Size(w, h), Font = f, BackColor = Color.White, AutoEllipsis = true };
        ComboBox Chon(string[] ds, int y)
        {
            var c = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(60, y), Size = new Size(150, 24), Font = GD.Chu(9) };
            c.Items.AddRange(ds);
            return c;
        }
        chonTu = Chon(XeKhach.Tram[..XeKhach.SoChang], 74);
        chonDen = Chon(XeKhach.Tram[1..], 102);
        chonTu.SelectedIndex = 0;
        chonDen.SelectedIndex = XeKhach.SoChang - 1;
        chonTu.SelectedIndexChanged += (_, _) => DoiDoan();
        chonDen.SelectedIndexChanged += (_, _) => DoiDoan();
        cvTuyen = new OVe(6, 130, 250, 40) { BackColor = Color.White, Ve = VeTuyen };
        lblGhe = N("Chỗ đã chọn: –", 8, 172, 248, GD.Chu(10, true), 20);
        lblTien = N(" ", 8, 192, 248, GD.Chu(9), 32);
        lblTien.AutoEllipsis = false;
        Button NutLon(string t, Color mau, int y) => new()
        {
            Text = t, Location = new Point(8, y), Size = new Size(248, 32), BackColor = mau, ForeColor = Color.White, Font = GD.Chu(11, true),
            FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false,
        };
        BtnGiu = NutLon("GIỮ CHỖ", GD.Hex("#ff8f00"), 226);
        BtnGiu.Click += (_, _) => GiuCho();
        var nhanCon = N("Giữ chỗ còn", 8, 270, 80, GD.Chu(9));
        LblDongHo = new Label { Text = "--:--", Location = new Point(90, 262), Size = new Size(120, 30), Font = GD.ChuCoDinh(18, true), ForeColor = GD.Hex("#909090"), BackColor = Color.White };
        BtnTt = NutLon("THANH TOÁN", XeGui.MauXe, 294);
        BtnTt.Click += (_, _) => ThanhToan();
        BtnNha = GD.Nut("Nhả chỗ", GD.Hex("#f0f0f0"), 8, 330, 90, 26);
        BtnNha.Font = GD.Thuong;
        BtnNha.Click += (_, _) => Nha();
        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(8, 360), Size = new Size(248, 40), Font = GD.Chu(9, true), BackColor = GD.Hex("#e8e8e8"),
            TextAlign = ContentAlignment.MiddleCenter,
        };
        lblVe = N("Vé của bạn: –", 8, 404, 248, GD.Chu(9), 46);
        lblVe.AutoEllipsis = false;
        lblVe.ForeColor = XeGui.VeToi;
        foreach (var b in new[] { BtnGiu, BtnTt }) b.FlatAppearance.BorderSize = 0;
        phai.Controls.AddRange([cvXe, N("Đi từ", 8, 77, 50, GD.Chu(9)), chonTu, N("Đến", 8, 105, 50, GD.Chu(9)), chonDen, cvTuyen, lblGhe,
                                lblTien, BtnGiu, nhanCon, LblDongHo, BtnTt, BtnNha, LblThongBao, lblVe]);
        GD.Them(khung, cv, phai);
        VeLaiVe();
        CapNhatGio();
    }

    // ------------------------------------------------------------ băng rôn, tuyến, sơ đồ
    void VeBangRon(Graphics g)
    {
        using (var b = new SolidBrush(GD.Hex("#0f7a4f"))) g.FillRectangle(b, 0, 44, 250, 18);
        if (Tt is not { } tt) return;
        using var f1 = GD.Chu(11, true);
        using var f2 = GD.Chu(8);
        using var f3 = GD.Chu(8, true);
        using (var b = new SolidBrush(Color.White)) g.DrawString((Chung.Chuoi(tt, "nha_xe") ?? "").ToUpperInvariant(), f1, b, 8, 3);
        using (var b = new SolidBrush(GD.Hex("#d7f0e2"))) g.DrawString($"Chuyến {Chung.Chuoi(tt, "chuyen")} · {Chung.Chuoi(tt, "ten_xe")}", f2, b, 8, 26);
        using (var b = new SolidBrush(GD.Hex("#ffe082"))) g.DrawString($"Giữ chỗ {Chung.G(Chung.SoD(tt, "ttl_s"))} s · giá theo chặng", f3, b, 8, 46);
    }

    void VeTuyen(Graphics g)
    {
        int n = XeKhach.Tram.Length;
        var x = Enumerable.Range(0, n).Select(i => 16 + i * (218f / (n - 1))).ToArray();
        using (var p = new Pen(GD.Hex("#cfd8dc"), 6)) g.DrawLine(p, x[0], 12, x[n - 1], 12);
        using (var p = new Pen(XeGui.MauXe, 6)) g.DrawLine(p, x[Tu], 12, x[Den], 12);
        var gia = Tt?["gia_chang"] as JsonArray ?? [];
        using var vienTram = new Pen(XeGui.MauXe, 2);
        using var nenTram = new SolidBrush(XeGui.MauXe);
        using var f1 = GD.Chu(7, true);
        using var f2 = GD.Chu(6);
        for (int i = 0; i < n; i++)
        {
            var c = new RectangleF(x[i] - 6, 6, 12, 12);
            g.FillEllipse(Tu <= i && i <= Den ? nenTram : Brushes.White, c);
            g.DrawEllipse(vienTram, c);
            XeGui.ChuGiua(g, XeKhach.MaTram[i], f1, XeGui.ChuToi, x[i], 31);
            if (i < gia.Count && i < n - 1)
                XeGui.ChuGiua(g, $"{(Chung.SoLN(gia[i]) ?? 0) / 1000}k", f2, XeGui.Vien, (x[i] + x[i + 1]) / 2, 31);
        }
    }

    void VeCho(Graphics g)
    {
        if (Tt is not { } tt) return;
        string loai = Chung.Chuoi(tt, "loai_xe") ?? "giuong";
        if (soDo == null || soDo.LoaiXe != loai) soDo = new SoDoXe(loai, 1, true);
        soDo.VeNen(g);
        soDo.Ve(g, tt, Ten, Tu, Den, DangChon, null);
        float y = soDo.Cao + 10, x = 8;
        using var f = GD.Chu(8);
        using var bChu = new SolidBrush(XeGui.ChuToi);
        using var p = new Pen(XeGui.Vien);
        for (int i = 0; i < ChuGiai.Length; i++)         // chú giải màu, 2 hàng
        {
            if (i == 3) (x, y) = (8, y + 16);
            var (mau, chu) = ChuGiai[i];
            using (var b = new SolidBrush(mau)) g.FillRectangle(b, x, y, 14, 11);
            g.DrawRectangle(p, x, y, 14, 11);
            g.DrawString(chu, f, bChu, x + 18, y - 2);
            x += 30 + g.MeasureString(chu, f).Width;
        }
        var chang = Enumerable.Range(0, XeKhach.SoChang).Select(k => $"{XeKhach.MaTram[k]}-{XeKhach.MaTram[k + 1]}");
        using var f2 = GD.Chu(7);
        using var b2 = new SolidBrush(GD.Hex("#455a64"));
        g.DrawString($"Vạch ở chân chỗ: từng chặng {string.Join(" | ", chang)}", f2, b2, 8, y + 20);
    }

    // ------------------------------------------------------------ chọn đoạn, chọn chỗ
    /// <summary>Chọn đoạn đường (người dùng hoặc kịch bản demo): chỉ số trạm.</summary>
    public void DatDoan(int tu, int den)
    {
        if (Giu.Count > 0) return;                       // đang giữ chỗ: không đổi đoạn
        dangDatDoan = true;
        chonTu.SelectedIndex = Math.Clamp(tu, 0, XeKhach.SoChang - 1);
        chonDen.SelectedIndex = Math.Clamp(den - 1, 0, XeKhach.SoChang - 1);
        dangDatDoan = false;
        DoiDoan();
    }

    void DoiDoan()
    {
        if (dangDatDoan || BtnGiu == null) return;      // đang dựng cửa sổ / đang đặt hai ô
        int t = chonTu.SelectedIndex, d = chonDen.SelectedIndex + 1;
        if (d <= t)                                      // "Đến" phải sau "Đi từ"
        {
            d = t + 1;
            dangDatDoan = true;
            chonDen.SelectedIndex = d - 1;
            dangDatDoan = false;
        }
        (Tu, Den) = (t, d);
        if (Tt is { } tt)
        {
            var bo = DangChon.Where(g => XeGui.TrangCho(tt, g, Ten, Tu, Den) != "trong").ToList();
            DangChon.ExceptWith(bo);
            if (bo.Count > 0) Bao($"Chỗ {string.Join(", ", bo)} không trống cả đoạn {XeKhach.TenDoan(Tu, Den)}", GD.Hex("#ffe0b2"));
        }
        cvTuyen.Invalidate();
        CapNhatGio();
    }

    /// <summary>Bấm một chỗ (người dùng hoặc kịch bản demo).</summary>
    public void Bam(string g)
    {
        if (Kn == null || Tt is not { } tt) return;
        string trang = XeGui.TrangCho(tt, g, Ten, Tu, Den);
        if (DangChon.Contains(g)) DangChon.Remove(g);
        else if (trang == "trong" && !Giu.Contains(g))
        {
            if (DangChon.Count + Giu.Count >= XeKhach.ToiDaGhe)
            {
                Bao($"Tối đa {XeKhach.ToiDaGhe} chỗ mỗi lần", GD.Hex("#ffe0b2"));
                return;
            }
            DangChon.Add(g);
        }
        else if (trang == "giu") Bao($"Chỗ {g} đang có người giữ trên đoạn này", GD.Hex("#ffe0b2"));
        else if (trang == "ban") Bao($"Chỗ {g} đã bán trên đoạn này", GD.Hex("#e0e0e0"));
        CapNhatGio();
    }

    long Gia(int tu, int den, int soCho)
    {
        var g = Tt?["gia_chang"] as JsonArray ?? [];
        long t = 0;
        for (int k = tu; k < den && k < g.Count; k++) t += Chung.SoLN(g[k]) ?? 0;
        return t * soCho;
    }

    void CapNhatGio()
    {
        if (Giu.Count > 0)
        {
            long g = Gia(doanGiu.Tu, doanGiu.Den, Giu.Count);
            lblGhe.Text = $"Đang giữ: {string.Join(", ", Giu)}";
            lblTien.Text = $"{XeKhach.TenDoan(doanGiu.Tu, doanGiu.Den)} · tổng {Chung.Tien(g)}";
            BtnTt.Text = $"THANH TOÁN {Chung.Tien(g)}";
        }
        else
        {
            lblGhe.Text = $"Chỗ đã chọn: {(DangChon.Count > 0 ? string.Join(", ", DangChon) : "–")}";
            lblTien.Text = $"{XeKhach.TenDoan(Tu, Den)} · tạm tính {Chung.Tien(Gia(Tu, Den, DangChon.Count))}";
            BtnTt.Text = "THANH TOÁN";
        }
        BatNut();
        cv.Invalidate();
    }

    void BatNut()
    {
        bool co = Kn != null, conHan = han is { } h && Chung.Now < h;
        bool giuDuoc = co && DangChon.Count > 0 && Giu.Count == 0, ttDuoc = co && Giu.Count > 0 && conHan;
        (BtnGiu.Enabled, BtnGiu.BackColor) = (giuDuoc, giuDuoc ? GD.Hex("#ff8f00") : GD.Hex("#ffcc80"));
        (BtnTt.Enabled, BtnTt.BackColor) = (ttDuoc, ttDuoc ? XeGui.MauXe : GD.Hex("#8fb8a3"));
        BtnNha.Enabled = co && Giu.Count > 0;
        chonTu.Enabled = chonDen.Enabled = Giu.Count == 0;   // đang giữ chỗ: không đổi đoạn
    }

    void Bao(string text, Color nen, Color? chu = null) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu ?? Color.Black);

    // ------------------------------------------------------------ giữ / thanh toán / nhả
    static JsonObject Truong(IEnumerable<string> ghe, int tu, int den) => new() { ["ghe"] = XeKhach.Json(ghe), ["tu"] = tu, ["den"] = den };

    void GiuCho()
    {
        if (DangChon.Count == 0 || Giu.Count > 0) return;
        GuiYc("giu_cho", Truong(DangChon, Tu, Den));    // gửi trước, vẽ sau
        NhatKy.Ghi($"Giữ chỗ {string.Join(", ", DangChon)} {XeKhach.TenDoan(Tu, Den)}…");
        Bao("Đang giữ chỗ…", GD.Hex("#fff3c4"));
        BtnGiu.Enabled = false;
    }

    void ThanhToan()
    {
        if (Giu.Count == 0) return;
        GuiYc("thanh_toan", Truong(Giu, doanGiu.Tu, doanGiu.Den));
        NhatKy.Ghi($"Thanh toán {string.Join(", ", Giu)}…");
    }

    void Nha()
    {
        if (Giu.Count > 0) GuiYc("nha", Truong(Giu, doanGiu.Tu, doanGiu.Den));
    }

    static List<string> DsJson(JsonNode? n) => [.. (n as JsonArray ?? []).Select(Chung.ChuoiPy)];

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        var ghe = DsJson(truong["ghe"]);
        int t = (int)Chung.SoL(truong, "tu"), d = (int)Chung.SoL(truong, "den");
        string noi = string.Join(", ", ghe);
        bool ok = Chung.Dung(tin, "ok");
        if (loai == "giu_cho")
        {
            if (ok)
            {
                SoGiu++;
                Giu = new SortedSet<string>(DsJson(tin["ghe"]), StringComparer.Ordinal);
                (doanGiu, phienGiu) = ((t, d), Chung.SoL(tin, "phien"));
                DangChon.ExceptWith(Giu);
                double hetMs = Chung.SoD(tin, "het_han_ms");
                han = Chung.Now + hetMs / 1000;
                Bao($"Đã giữ {noi} – thanh toán trong {Chung.F(hetMs / 1000, "F0")} s", GD.Hex("#c8f0d2"), GD.XanhDam);
                NhatKy.Ghi($"  ĐÃ GIỮ {noi} ({Chung.F(rtt, "F1")} ms)");
                KiemCho(Tt);
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
                var ma = DsJson(tin["ma_ve"]);
                for (int i = 0; i < ghe.Count; i++) VeCuaToi.Add(new VeXe(ghe[i], t, d, i < ma.Count ? ma[i] : ""));
                phienVe = Chung.SoL(tin, "phien");
                Giu.Clear();
                han = null;
                Bao($"Đặt vé thành công! {Chung.Tien(Chung.SoL(tin, "tien"))}", GD.Hex("#c8f0d2"), GD.XanhDam);
                NhatKy.Ghi($"  ĐẶT VÉ THÀNH CÔNG: {string.Join(", ", ma)} ({Chung.F(rtt, "F1")} ms)");
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

    void VeLaiVe()
    {
        var ds = VeCuaToi.Select(v => $"{v.Ghe} {XeKhach.MaTram[v.Tu]}→{XeKhach.MaTram[v.Den]}").ToList();
        lblVe.Text = $"Vé của bạn: {(ds.Count > 0 ? string.Join(", ", ds) : "–")}\nBạn: giữ được {SoGiu} lần, mua {soVe} vé";
    }

    /// <summary>Chỗ mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; chỗ đang chọn bị người khác giữ. Chỉ
    /// tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).</summary>
    void KiemCho(JsonObject? tt)
    {
        if (tt == null) return;
        long phien = Chung.SoL(tt, "phien");
        if (phien >= phienGiu && Giu.Count > 0)
        {
            foreach (var g in Giu.ToList())
            {
                bool tatCa = true;
                string? ai = null;
                for (int k = doanGiu.Tu; k < doanGiu.Den; k++)
                {
                    string? a = NguoiDatVeXe.ChuO(tt, $"{g}#{k}");
                    if (a == Ten) continue;
                    tatCa = false;
                    ai ??= a;
                }
                if (tatCa) continue;
                Giu.Remove(g);
                if (ai != null)
                {
                    Bao($"Chỗ {g} đã bị {ai} lấy mất!", GD.Hex("#ff6060"), Color.White);
                    NhatKy.Ghi($"Chỗ {g} đã bị {ai} lấy mất!");
                }
                else
                {
                    Bao("Hết thời gian giữ chỗ – chỗ đã được nhả", GD.Hex("#ffd0d0"), GD.DoDam);
                    NhatKy.Ghi($"Hết thời gian giữ chỗ: {g}");
                }
            }
            if (Giu.Count == 0) han = null;
        }
        if (phien >= phienVe)
        {
            var con = new List<VeXe>();
            foreach (var v in VeCuaToi)
            {
                string? ai = Ten;
                for (int k = v.Tu; k < v.Den; k++)
                    if (NguoiDatVeXe.ChuO(tt, $"{v.Ghe}#{k}") is var a && a != Ten)
                    {
                        ai = a;
                        break;
                    }
                if (ai == Ten)
                {
                    con.Add(v);
                    continue;
                }
                Bao($"Vé {v.Ghe} bị ghi cho {ai ?? "không ai"}!", GD.Hex("#ff6060"), Color.White);
                NhatKy.Ghi($"Vé {v.Ghe} bị ghi cho {ai ?? "không ai"}!");
            }
            VeCuaToi = con;
        }
        var biLay = DangChon.Where(g => XeGui.TrangCho(tt, g, Ten, Tu, Den) != "trong").ToList();
        DangChon.ExceptWith(biLay);
        if (biLay.Count > 0)
        {
            Bao($"Chỗ {string.Join(", ", biLay)} vừa có người giữ", GD.Hex("#ffe0b2"));
            NhatKy.Ghi($"Chỗ {string.Join(", ", biLay)} vừa có người giữ");
        }
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        string br = $"{Chung.Chuoi(tt, "nha_xe")}|{Chung.Chuoi(tt, "chuyen")}|{Chung.Chuoi(tt, "ten_xe")}|{Chung.SoD(tt, "ttl_s")}";
        if (br != bangRonCu)
        {
            bangRonCu = br;
            cvXe.Invalidate();
            cvTuyen.Invalidate();
        }
        if (LblThongBao.Text == "Chưa kết nối") Bao("Chọn đoạn đường, chọn chỗ rồi bấm GIỮ CHỖ", GD.Hex("#e8e8e8"));
        var chu = tt["chu"] as JsonObject ?? [];
        var dem = tt["dem"] as JsonObject;
        if (chu.Count == 0 && (VeCuaToi.Count > 0 || Giu.Count > 0) && Chung.SoL(dem, "ban") == 0 && Chung.SoL(dem, "giu") == 0)
            VeCuaToi.Clear();                            // máy chủ đặt lại: xe trống
        var ghe = DsJson(tt["ghe"]).ToHashSet();
        DangChon.RemoveWhere(g => !ghe.Contains(g));     // đổi loại xe: bỏ chỗ không còn
        if (Giu.Count > 0)
        {
            var conLai = tt["con_lai_ms"] as JsonObject;
            double? min = null;
            foreach (var g in Giu)
                for (int k = doanGiu.Tu; k < doanGiu.Den; k++)
                {
                    string o = $"{g}#{k}";
                    if (Chung.Chuoi(chu, o) == Ten && conLai?[o] is not null)
                    {
                        double x = Chung.SoD(conLai, o);
                        min = min is { } m ? Math.Min(m, x) : x;
                    }
                }
            if (min is { } mn) han = Chung.Now + mn / 1000;
        }
        KiemCho(tt);
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
        return $"chỗ-chặng trống {Chung.SoL(d, "trong")}, giữ {Chung.SoL(d, "giu")}, bán {Chung.SoL(d, "ban")}; bạn giữ"
               + $" {(Giu.Count > 0 ? string.Join(", ", Giu) : "–")} (còn {con}), vé {VeCuaToi.Count}";
    }
}
