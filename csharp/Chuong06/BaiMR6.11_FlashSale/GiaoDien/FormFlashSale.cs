using System.Drawing;
using System.Drawing.Drawing2D;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_11;

// Bài mở rộng MR6.11 – Trang flash sale (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// trang "giờ vàng" như sàn thương mại điện tử: dải FLASH SALE, đồng hồ đếm ngược tới giờ mở bán (chạy cục bộ theo
// mo_con_ms của cap_nhat), thẻ sản phẩm (ảnh vẽ bằng GDI+, nhãn giảm giá, giá sale cỡ lớn, giá gốc gạch ngang), thanh
// tồn kho "Đã bán x%" (cam, "SẮP CHÁY HÀNG" khi > 80 %, "HẾT HÀNG"), chọn số lượng 1–2, nút MUA NGAY bị khóa tới giờ
// mở bán, đơn hàng của bạn và "vừa mua" trực tiếp.
sealed class FormFlashSale(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    static readonly Color DoSale = GD.Hex("#e4002b"), CamSale = GD.Hex("#ff6a00");
    protected override string TieuDe => "Flash sale";
    protected override Size KichThuocKhung => new(600, 452);

    /// <summary>Ảnh sản phẩm: hộp sạc và đôi tai nghe không dây, tâm (cx, cy), tỉ lệ k (như ve_tai_nghe của bản Python).</summary>
    public static void VeTaiNghe(Graphics g, int w, int h, float cx, float cy, float k, bool nen = true)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        if (nen)
        {
            using (var b = new SolidBrush(GD.Hex("#fff1e6"))) g.FillRectangle(b, 0, 0, w, h);
            string[] mau = ["#ffe0cc", "#ffd1b3", "#ffc299"];
            for (int i = 0; i < 3; i++)
            {
                float r = (95 - i * 22) * k;
                using var b = new SolidBrush(GD.Hex(mau[i]));
                g.FillEllipse(b, cx - r, cy - r + 10 * k, 2 * r, 2 * r);
            }
        }
        float x0 = cx - 52 * k, x1 = cx + 52 * k, y0 = cy + 2 * k, y1 = cy + 62 * k, rr = 22 * k;   // hộp sạc bo góc
        GraphicsPath Hop(float d)
        {
            var p = new GraphicsPath();
            p.AddArc(x0 + d, y0 + d, 2 * rr, 2 * rr, 180, 90);
            p.AddArc(x1 + d - 2 * rr, y0 + d, 2 * rr, 2 * rr, 270, 90);
            p.AddArc(x1 + d - 2 * rr, y1 + d - 2 * rr, 2 * rr, 2 * rr, 0, 90);
            p.AddArc(x0 + d, y1 + d - 2 * rr, 2 * rr, 2 * rr, 90, 90);
            p.CloseFigure();
            return p;
        }
        using (var bong = Hop(3 * k))
        using (var b = new SolidBrush(GD.Hex("#c9b8a8"))) g.FillPath(b, bong);              // bóng
        using (var hop = Hop(0))
        using (var b = new SolidBrush(GD.Hex("#fafafa")))
        using (var p = new Pen(GD.Hex("#9a9a9a")))
        {
            g.FillPath(b, hop);
            g.DrawPath(p, hop);
        }
        using (var p = new Pen(GD.Hex("#b8b8b8"))) g.DrawLine(p, x0 + 4 * k, y0 + 22 * k, x1 - 4 * k, y0 + 22 * k);
        using (var b = new SolidBrush(GD.Hex("#2ecc71"))) g.FillEllipse(b, cx - 3 * k, y0 + 34 * k, 6 * k, 6 * k);
        using var vien = new Pen(GD.Hex("#9a9a9a"));
        foreach (int d in new[] { -1, 1 })                                                  // hai tai nghe
        {
            float hx = cx + d * 28 * k, hy = cy - 42 * k;
            PointF[] than = [new(hx - d * 4 * k, hy + 10 * k), new(hx + d * 6 * k, hy + 10 * k), new(hx + d * 12 * k, hy + 52 * k), new(hx + d * 2 * k, hy + 54 * k)];
            using (var b = new SolidBrush(GD.Hex("#f4f4f4"))) g.FillClosedCurve(b, than, FillMode.Alternate, 0.3f);
            g.DrawClosedCurve(vien, than, 0.3f, FillMode.Alternate);
            g.FillEllipse(Brushes.White, hx - 17 * k, hy - 15 * k, 34 * k, 32 * k);
            g.DrawEllipse(vien, hx - 17 * k, hy - 15 * k, 34 * k, 32 * k);
            using (var b = new SolidBrush(GD.Hex("#3a3a3a"))) g.FillEllipse(b, hx - 8 * k, hy - 6 * k, 16 * k, 14 * k);
            using (var b = new SolidBrush(GD.Hex("#7a7a7a"))) g.FillEllipse(b, hx - 4 * k, hy - 3 * k, 5 * k, 4 * k);
        }
    }

    /// <summary>Đồng hồ hh:mm:ss (làm tròn lên giây).</summary>
    static string DongHo(double giay)
    {
        int s = Math.Max(0, (int)(giay + 0.999));
        return $"{s / 3600:00}:{s / 60 % 60:00}:{s % 60:00}";
    }

    double? moLuc;                                       // thời điểm mở bán theo đồng hồ máy này
    readonly List<(string Ma, long Sl, long Tien, string Gio)> don = [];   // đơn của tôi
    public bool DangCho;
    public Label LblTrangThaiSale = null!, LblDongHo = null!, LblThongBao = null!;
    Label lblTen = null!, lblGia = null!, lblGoc = null!, lblGioiHan = null!;
    OVe anh = null!, thanhKho = null!;
    RadioButton[] chonSl = [];
    public Button BtnMua = null!;
    ListBox lsDon = null!, lsGan = null!;
    readonly HashSet<int> dongCuaToi = [];

    protected override void Ve(Panel khung)
    {
        var dai = new Panel { Location = new Point(0, 4), Size = new Size(600, 44), BackColor = DoSale };
        var t1 = new Label { Text = "FLASH SALE", Location = new Point(8, 4), AutoSize = true, ForeColor = Color.White,
                             Font = new Font(Control.DefaultFont.FontFamily, 20, FontStyle.Bold | FontStyle.Italic) };
        var t2 = new Label { Text = "GIỜ VÀNG\nGIÁ SỐC", Location = new Point(196, 6), Size = new Size(90, 34), ForeColor = GD.Hex("#ffe066"), Font = GD.Chu(9, true) };
        LblTrangThaiSale = new Label { Text = "Bắt đầu sau", Location = new Point(290, 13), Size = new Size(128, 20), ForeColor = Color.White,
                                       Font = GD.Chu(10, true), TextAlign = ContentAlignment.MiddleRight };
        LblDongHo = new Label { Text = "--:--:--", Location = new Point(422, 7), Size = new Size(172, 30), ForeColor = Color.White,
                                BackColor = GD.Hex("#1a1a1a"), Font = GD.ChuCoDinh(16, true), TextAlign = ContentAlignment.MiddleCenter };
        dai.Controls.AddRange([t1, t2, LblTrangThaiSale, LblDongHo]);

        var the = new Panel { Location = new Point(0, 52), Size = new Size(600, 224), BackColor = Color.White, BorderStyle = BorderStyle.FixedSingle };
        anh = new OVe(6, 6, 230, 210) { BackColor = Color.White, Ve = VeAnh };
        Label N(string t, int x, int y, int w, int h, Font f) => new() { Text = t, Location = new Point(x, y), Size = new Size(w, h), Font = f, BackColor = Color.White };
        lblTen = N("–", 244, 8, 348, 44, GD.Chu(12, true));
        lblGia = new Label { Text = "–", Location = new Point(244, 52), AutoSize = true, Font = GD.Chu(22, true), ForeColor = DoSale, BackColor = Color.White };
        lblGoc = new Label { Text = "", Location = new Point(400, 66), AutoSize = true, ForeColor = GD.Hex("#909090"), BackColor = Color.White,
                             Font = new Font(Control.DefaultFont.FontFamily, 11, FontStyle.Strikeout) };
        lblGioiHan = N("", 244, 96, 348, 18, GD.Chu(9));
        lblGioiHan.ForeColor = GD.Hex("#606060");
        thanhKho = new OVe(244, 118, 300, 24) { BackColor = Color.White, Ve = VeKho };
        var nhanSl = N("Số lượng:", 244, 152, 64, 20, GD.Thuong);
        chonSl = [.. new[] { 1, 2 }.Select((n, i) => new RadioButton { Text = $"{n}", Location = new Point(310 + i * 44, 150), Size = new Size(40, 22), Checked = n == 1, BackColor = Color.White })];
        BtnMua = new Button
        {
            Text = "CHỜ MỞ BÁN", Location = new Point(244, 178), Size = new Size(240, 38), Font = GD.Chu(14, true), BackColor = CamSale, ForeColor = Color.White,
            FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false,
        };
        BtnMua.FlatAppearance.BorderSize = 0;
        BtnMua.Click += (_, _) => Mua();
        the.Controls.AddRange([anh, lblTen, lblGia, lblGoc, lblGioiHan, thanhKho, nhanSl, .. chonSl, BtnMua]);

        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(0, 280), Size = new Size(600, 28), Font = GD.Chu(11, true), BackColor = GD.Hex("#e0e0e0"),
            TextAlign = ContentAlignment.MiddleCenter,
        };
        var g1 = GD.Nhom("Đơn hàng của bạn", 0, 312, 330, 138);
        lsDon = new ListBox { Location = new Point(8, 18), Size = new Size(314, 112), Font = GD.ChuCoDinh(9), IntegralHeight = false, SelectionMode = SelectionMode.None };
        GD.Them(g1, lsDon);
        var g2 = GD.Nhom("Vừa mua (trực tiếp)", 336, 312, 264, 138);
        lsGan = new ListBox
        {
            Location = new Point(8, 18), Size = new Size(248, 112), Font = GD.ChuCoDinh(9), IntegralHeight = false, SelectionMode = SelectionMode.None,
            DrawMode = DrawMode.OwnerDrawFixed,
        };
        lsGan.DrawItem += (_, e) =>                      // đơn của mình tô xanh
        {
            if (e.Index < 0) return;
            e.DrawBackground();
            using var b = new SolidBrush(dongCuaToi.Contains(e.Index) ? GD.Hex("#0040c0") : Color.Black);
            e.Graphics.DrawString(lsGan.Items[e.Index]?.ToString(), lsGan.Font, b, e.Bounds.Location);
        };
        GD.Them(g2, lsGan);
        GD.Them(khung, dai, the, LblThongBao, g1, g2);
        Nut();
    }

    // ------------------------------------------------------------ vẽ ảnh, thanh tồn kho
    void VeAnh(Graphics g)
    {
        VeTaiNghe(g, 230, 210, 115, 92, 1.0f);
        if (Tt?["sp"] is not JsonObject sp) return;
        long giam = (long)Math.Round(100 * (1 - Chung.SoD(sp, "gia_sale") / Math.Max(1, Chung.SoD(sp, "gia_goc"))));
        using (var b = new SolidBrush(DoSale)) g.FillEllipse(b, 168, 8, 54, 54);
        using (var p = new Pen(Color.White, 2)) g.DrawEllipse(p, 168, 8, 54, 54);
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        using (var f = GD.Chu(12, true)) g.DrawString($"-{giam}%", f, Brushes.White, 195, 35, sf);
        if (Chung.SoL(Tt, "ton") > 0) return;
        using (var b = new SolidBrush(GD.Hex("#404040"))) g.FillRectangle(b, 0, 170, 230, 35);
        using (var f = GD.Chu(14, true)) g.DrawString("HẾT HÀNG", f, Brushes.White, 115, 187, sf);
    }

    void VeKho(Graphics g)
    {
        if (Tt == null) return;
        long ton = Chung.SoL(Tt, "ton"), tonDau = Math.Max(1, Chung.SoL(Tt, "ton_dau"));
        double tiLe = Math.Min(1.0, (double)(tonDau - ton) / tonDau);
        using (var b = new SolidBrush(GD.Hex("#ffd6c2"))) g.FillRectangle(b, 0, 2, 300, 20);
        using (var b = new SolidBrush(tiLe >= 0.8 ? DoSale : CamSale)) g.FillRectangle(b, 0, 2, (float)(300 * tiLe), 20);
        string chu = ton <= 0 ? "HẾT HÀNG" : tiLe >= 0.8 ? $"SẮP CHÁY HÀNG – còn {ton}" : $"ĐÃ BÁN {Math.Round(tiLe * 100, MidpointRounding.ToEven)}%";
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        using var f = GD.Chu(10, true);
        using var bc = new SolidBrush(tiLe > 0.45 ? Color.White : GD.Hex("#a03000"));
        g.DrawString(chu, f, bc, 150, 12, sf);
    }

    // ------------------------------------------------------------ mua
    int SoLuong => chonSl[1].Checked ? 2 : 1;

    public void Mua(int? sl = null)
    {
        if (Kn == null || DangCho) return;
        if (sl is { } x) chonSl[x == 2 ? 1 : 0].Checked = true;
        int n = SoLuong;
        DangCho = true;
        (BtnMua.Text, BtnMua.Enabled) = ("ĐANG XỬ LÝ…", false);
        NhatKy.Ghi($"MUA NGAY {n} sản phẩm");
        GuiYc("mua", new JsonObject { ["so_luong"] = n });
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        DangCho = false;
        if (Chung.Dung(tin, "ok"))
        {
            string ma = Chung.Chuoi(tin, "ma_don")!;
            long sl = Chung.SoL(tin, "so_luong"), tien = Chung.SoL(tin, "thanh_tien");
            don.Add((ma, sl, tien, DateTime.Now.ToString("HH:mm:ss")));
            Bao($"Đặt hàng thành công! Đơn {ma} – {sl} sp", GD.Hex("#70d070"), Color.Black);
            NhatKy.Ghi($"  đơn {ma}, {Chung.Tien(tien)} ({Chung.F(rtt, "F1")} ms)");
            lsDon.Items.Clear();
            for (int i = don.Count - 1; i >= 0; i--) lsDon.Items.Add($"{don[i].Gio} {don[i].Ma} ×{don[i].Sl} {Chung.Tien(don[i].Tien),11}");
        }
        else
        {
            string? m = Chung.Chuoi(tin, "ma_loi");
            string text = m switch
            {
                "chua_mo" => "Chưa tới giờ mở bán!",
                "het_hang" => "Rất tiếc, sản phẩm đã HẾT HÀNG",
                "khong_du" => $"Chỉ còn {Chung.ChuoiPy(tin["ton"])} sản phẩm – giảm số lượng",
                "xung_dot" => "Hệ thống đang bận, vui lòng thử lại",
                _ => Chung.Chuoi(tin, "ly_do") ?? "",
            };
            Bao(text, GD.Hex(m == "het_hang" ? "#ff6060" : "#ffb347"), m == "het_hang" ? Color.White : Color.Black);
            NhatKy.Ghi($"  từ chối: {Chung.ChuoiPy(tin["ly_do"])}");
        }
        Nut();
    }

    void Bao(string text, Color nen, Color chu) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu);

    bool DaMo => moLuc is { } m && Chung.Now >= m;

    void Nut()
    {
        if (DangCho) return;
        if (Kn == null || Tt == null) (BtnMua.Text, BtnMua.Enabled, BtnMua.BackColor) = ("CHỜ MỞ BÁN", false, GD.Hex("#c8c8c8"));
        else if (Chung.SoL(Tt, "ton") <= 0) (BtnMua.Text, BtnMua.Enabled, BtnMua.BackColor) = ("HẾT HÀNG", false, GD.Hex("#909090"));
        else if (!DaMo) (BtnMua.Text, BtnMua.Enabled, BtnMua.BackColor) = ("CHỜ MỞ BÁN", false, GD.Hex("#f5b48a"));
        else (BtnMua.Text, BtnMua.Enabled, BtnMua.BackColor) = ("MUA NGAY", true, CamSale);
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        var sp = tt["sp"] as JsonObject;
        moLuc = Chung.Now + Chung.SoD(tt, "mo_con_ms") / 1000;
        lblTen.Text = Chung.Chuoi(sp, "ten");
        lblGia.Text = Chung.Tien(Chung.SoL(sp, "gia_sale"));
        lblGoc.Text = Chung.Tien(Chung.SoL(sp, "gia_goc"));
        lblGoc.Left = lblGia.Right + 8;
        lblGioiHan.Text = $"Mỗi đơn tối đa {Chung.SoL(sp, "toi_da")} sản phẩm · Số lượng có hạn: {Chung.SoL(tt, "ton_dau")}";
        anh.Invalidate();
        thanhKho.Invalidate();
        var gan = (tt["don_gan"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        var dong = gan.Select(x => $"{Chung.Chuoi(x, "gio")} {Chung.Chuoi(x, "nguoi"),-6} ×{Chung.SoL(x, "so_luong")}").ToList();
        if (!lsGan.Items.Cast<string>().SequenceEqual(dong))
        {
            dongCuaToi.Clear();
            for (int i = 0; i < gan.Count; i++)
                if (Chung.Chuoi(gan[i], "nguoi") == Ten) dongCuaToi.Add(i);
            lsGan.BeginUpdate();
            lsGan.Items.Clear();
            foreach (var d in dong) lsGan.Items.Add(d);
            lsGan.EndUpdate();
        }
        if (LblThongBao.Text == "Chưa kết nối") Bao("Đón giờ vàng – nút MUA NGAY mở khi hết đếm ngược", GD.Hex("#e0e0e0"), Color.Black);
        MoiNhip();
    }

    protected override void SuKien(string text)
    {
        base.SuKien(text);
        if (!text.StartsWith("Đặt lại")) return;         // đợt sale mới
        don.Clear();
        lsDon.Items.Clear();
        Bao("Đợt sale mới – chờ giờ mở bán", GD.Hex("#e0e0e0"), Color.Black);
    }

    protected override void MoiNhip()
    {
        if (Tt == null || moLuc is not { } m) return;
        double con = m - Chung.Now;
        if (con > 0)
        {
            LblTrangThaiSale.Text = "Bắt đầu sau";
            (LblDongHo.Text, LblDongHo.BackColor) = (DongHo(con), GD.Hex("#1a1a1a"));
        }
        else if (Chung.SoL(Tt, "ton") <= 0)
        {
            LblTrangThaiSale.Text = "Đã kết thúc";
            (LblDongHo.Text, LblDongHo.BackColor) = ("CHÁY HÀNG", GD.Hex("#404040"));
        }
        else
        {
            LblTrangThaiSale.Text = "";
            (LblDongHo.Text, LblDongHo.BackColor) = ("ĐANG DIỄN RA", GD.DoDam);
        }
        Nut();
    }

    protected override string DongDinhKy()
    {
        var tt = Tt!;
        double con = Math.Max(0.0, (moLuc ?? 0) - Chung.Now);
        return $"tồn {Chung.SoL(tt, "ton")}/{Chung.SoL(tt, "ton_dau")}, " + (con > 0 ? $"mở sau {Chung.F(con, "F0")} s" : "đang bán")
               + $"; bạn có {don.Count} đơn";
    }
}
