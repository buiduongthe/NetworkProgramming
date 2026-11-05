using System.Drawing;
using System.Drawing.Drawing2D;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_7;

// Bài mở rộng MR6.7 – Ứng dụng gọi xe (người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// ứng dụng gọi xe: bản đồ phố (GDI+: đường, công viên, hồ, tên phố), điểm đón (xanh) và điểm đến (đỏ) – bấm chuột
// trái/phải lên bản đồ để chọn ngã tư –, tuyến đường, các xe (màu theo xe, xe đã có khách mờ đi), giá cước dự kiến,
// nút ĐẶT XE, trạng thái "Đang tìm tài xế…" / "Đặt thành công – tài xế …" / "Hết chỗ" / "Tài xế … đã nhận khách
// khác!", thẻ tài xế và nút Hủy chuyến. Đặt được thì xe chạy hoạt hình tới điểm đón rồi tới điểm đến. Mọi số liệu
// về xe đến từ cap_nhat của máy chủ.

/// <summary>Đường đi trên lưới phố (dùng cho hoạt hình xe).</summary>
static class TuyenDuong
{
    public static double DoDaiDoan((double X, double Y) p, (double X, double Y) q) => Math.Abs(p.X - q.X) + Math.Abs(p.Y - q.Y);

    /// <summary>Đường đi trên lưới phố từ a tới b: đi ngang trước rồi đi dọc.</summary>
    public static List<(double X, double Y)> DuongDi((double X, double Y) a, (double X, double Y) b)
    {
        var ds = new List<(double X, double Y)> { a };
        if (a.X != b.X && a.Y != b.Y) ds.Add((b.X, a.Y));
        ds.Add(b);
        return ds;
    }

    public static double DoDai(List<(double X, double Y)> ds) => ds.Zip(ds.Skip(1)).Sum(x => DoDaiDoan(x.First, x.Second));

    /// <summary>Điểm cách đầu đường đi một đoạn s (đơn vị bản đồ) và hướng ("ngang"/"doc").</summary>
    public static ((double X, double Y) Diem, string Huong) DiemTren(List<(double X, double Y)> ds, double s)
    {
        for (int i = 0; i + 1 < ds.Count; i++)
        {
            var (p, q) = (ds[i], ds[i + 1]);
            double d = DoDaiDoan(p, q);
            if (s <= d || i + 1 == ds.Count - 1)
            {
                double t = d == 0 ? 0 : Math.Min(1.0, s / d);
                return ((p.X + (q.X - p.X) * t, p.Y + (q.Y - p.Y) * t), p.Y == q.Y ? "ngang" : "doc");
            }
            s -= d;
        }
        return (ds[^1], "ngang");
    }

    /// <summary>Phần đường đi từ khoảng cách s0 tới s1 (danh sách điểm).</summary>
    public static List<(double X, double Y)> Doan(List<(double X, double Y)> ds, double s0, double s1)
    {
        var kq = new List<(double X, double Y)> { DiemTren(ds, s0).Diem };
        double tich = 0;
        for (int i = 0; i + 1 < ds.Count; i++)
        {
            tich += DoDaiDoan(ds[i], ds[i + 1]);
            if (s0 < tich && tich < s1) kq.Add(ds[i + 1]);
        }
        kq.Add(DiemTren(ds, s1).Diem);
        return kq;
    }
}

/// <summary>Bản đồ phố (dùng cả ở ứng dụng gọi xe và bảng điều khiển máy chủ): nền vẽ một lần vào ảnh đệm, phần
/// động (tuyến, ghim, xe) vẽ lại mỗi lần Invalidate.</summary>
sealed class BanDo : Control
{
    public static readonly Dictionary<string, Color> MauXe = new()
    {
        ["trắng"] = GD.Hex("#f4f4f4"), ["đỏ"] = GD.Hex("#e53935"), ["xanh"] = GD.Hex("#1e88e5"), ["bạc"] = GD.Hex("#b0bec5"),
        ["vàng"] = GD.Hex("#fdd835"), ["đen"] = GD.Hex("#37474f"), ["cam"] = GD.Hex("#fb8c00"), ["tím"] = GD.Hex("#8e24aa"),
        ["xám"] = GD.Hex("#78909c"), ["hồng"] = GD.Hex("#f06292"),
    };
    public static readonly Color XanhGrab = GD.Hex("#00a651"), XanhTuyen = GD.Hex("#1a73e8"), DoPin = GD.Hex("#d93025");

    readonly bool nho;
    readonly List<Action<Graphics>> dong = [];
    Bitmap? nen;

    public BanDo(int x, int y, int w, int h, bool nho = false)
    {
        this.nho = nho;
        DoubleBuffered = true;
        (Location, Size) = (new Point(x, y), new Size(w, h));
    }

    PointF P(double x, double y) => new((float)(x * Width), (float)(y * Height));

    RectangleF O(double x0, double y0, double x1, double y1, double le = 0.022)
    {
        var a = P(x0 + le, y0 + le);
        var b = P(x1 - le, y1 - le);
        return RectangleF.FromLTRB(a.X, a.Y, b.X, b.Y);
    }

    static void Chu(Graphics g, string chu, float x, float y, Color mau, float co, bool dam = false, float goc = 0)
    {
        using var f = GD.Chu(co, dam);
        using var b = new SolidBrush(mau);
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        var st = g.Save();
        g.TranslateTransform(x, y);
        if (goc != 0) g.RotateTransform(goc);
        g.DrawString(chu, f, b, 0, 0, sf);
        g.Restore(st);
    }

    void VeNen(Graphics g)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.Clear(GD.Hex("#ebe7df"));
        double[] bien = [0.0, .. BanDoPho.Duong, 1.0];
        using (var nha = new SolidBrush(GD.Hex("#e2ddd2")))
        using (var nha2 = new SolidBrush(GD.Hex("#d6d0c2")))
            for (int i = 0; i + 1 < bien.Length; i++)                     // khu phố: nhà (ô nhỏ)
                for (int j = 0; j + 1 < bien.Length; j++)
                {
                    var r = O(bien[i], bien[j], bien[i + 1], bien[j + 1]);
                    if (r.Width < 6 || r.Height < 6) continue;
                    g.FillRectangle(nha, r);
                    if (!nho && (i + j) % 2 == 0)
                        g.FillRectangle(nha2, RectangleF.FromLTRB(r.Left + 4, r.Top + 4, (r.Left + r.Right) / 2 - 2, (r.Top + r.Bottom) / 2 - 2));
                }
        var cv = O(0.3, 0.7, 0.5, 0.9);                                   // công viên
        using (var b = new SolidBrush(GD.Hex("#bfe3a6"))) g.FillRectangle(b, cv);
        using (var b = new SolidBrush(GD.Hex("#7cbf5a")))
            for (int k = 0; k < 5; k++)
            {
                float x = cv.Left + cv.Width * (0.2f + 0.15f * k), y = cv.Top + cv.Height * (k % 2 == 1 ? 0.35f : 0.65f), r = nho ? 3 : 6;
                g.FillEllipse(b, x - r, y - r, 2 * r, 2 * r);
            }
        if (!nho) Chu(g, "Công viên", (cv.Left + cv.Right) / 2, cv.Bottom - 7, GD.Hex("#3d7a2a"), 7);
        var ho = O(0.7, 0.1, 0.9, 0.3);                                   // hồ
        using (var b = new SolidBrush(GD.Hex("#a9d3f2"))) g.FillEllipse(b, ho);
        using (var p = new Pen(GD.Hex("#8cc2ea"))) g.DrawEllipse(p, ho);
        if (!nho) Chu(g, "Hồ", (ho.Left + ho.Right) / 2, (ho.Top + ho.Bottom) / 2, GD.Hex("#2a6fa8"), 7);
        int rong = nho ? 7 : 12;
        foreach (double x in BanDoPho.Duong)                              // đường dọc và ngang
        {
            bool chinh = x == 0.5;
            using var p = new Pen(chinh ? GD.Hex("#fde293") : Color.White, rong + (chinh ? 2 : 0));
            g.DrawLine(p, P(x, 0), P(x, 1));
            g.DrawLine(p, P(0, x), P(1, x));
        }
        if (nho) return;
        var mauTen = GD.Hex("#7a7468");
        for (int i = 0; i < BanDoPho.Duong.Length; i++)                  // tên phố: giữa hai ngã tư
        {
            double d = BanDoPho.Duong[i];
            Chu(g, BanDoPho.TenDoc[i], (float)(d * Width + 10), (float)((i % 2 == 1 ? 0.4 : 0.6) * Height), mauTen, 7, goc: 90);
            Chu(g, BanDoPho.TenNgang[i], (float)((i % 2 == 1 ? 0.4 : 0.6) * Width), (float)(d * Height - 10), mauTen, 7);
        }
    }

    public void Xoa()
    {
        dong.Clear();
        Invalidate();
    }

    public void Tuyen(IList<(double X, double Y)> ds, Color? mau = null, float[]? net = null)
    {
        if (ds.Count < 2) return;
        var diem = ds.Select(p => P(p.X, p.Y)).ToArray();
        float rong = nho ? 3 : 5;
        var m = mau ?? XanhTuyen;
        dong.Add(g =>
        {
            if (net == null)
            {
                using var vien = new Pen(GD.Hex("#0b4fb3"), rong + 2) { StartCap = LineCap.Round, EndCap = LineCap.Round, LineJoin = LineJoin.Round };
                g.DrawLines(vien, diem);
            }
            using var p = new Pen(m, rong) { StartCap = LineCap.Round, EndCap = LineCap.Round, LineJoin = LineJoin.Round };
            if (net != null) p.DashPattern = net;
            g.DrawLines(p, diem);
        });
        Invalidate();
    }

    public void Ghim(double x, double y, Color mau, string chu)
    {
        var pt = P(x, y);
        float r = nho ? 5 : 9;
        dong.Add(g =>
        {
            using var b = new SolidBrush(mau);
            g.FillPolygon(b, [new PointF(pt.X - r * 0.8f, pt.Y - r * 1.4f), new PointF(pt.X + r * 0.8f, pt.Y - r * 1.4f), pt]);
            g.FillEllipse(b, pt.X - r, pt.Y - r * 2.6f, 2 * r, 2 * r);
            using (var vien = new Pen(Color.White, 2)) g.DrawEllipse(vien, pt.X - r, pt.Y - r * 2.6f, 2 * r, 2 * r);
            g.FillEllipse(Brushes.White, pt.X - r * 0.35f, pt.Y - r * 1.95f, r * 0.7f, r * 0.7f);
            if (chu != "" && !nho)
            {
                using var f = GD.Chu(8, true);
                g.DrawString(chu, f, b, pt.X + r + 3, pt.Y - r * 1.6f - 7);
            }
        });
        Invalidate();
    }

    public void Xe(double x, double y, string mau, string nhanXe = "", string huong = "ngang", bool mo = false, bool noiBat = false)
    {
        var pt = P(x, y);
        float dai = nho ? 14 : 24, ngan = nho ? 8 : 13;
        var (rx, ry) = huong == "ngang" ? (dai / 2, ngan / 2) : (ngan / 2, dai / 2);
        dong.Add(g =>
        {
            float px = pt.X, py = pt.Y;
            if (noiBat)
            {
                float r = dai * 0.85f;
                using var vong = new Pen(XanhGrab, 3);
                g.DrawEllipse(vong, px - r, py - r, 2 * r, 2 * r);
            }
            using (var bong = new SolidBrush(GD.Hex("#9a9488"))) g.FillRectangle(bong, px - rx + 1, py - ry + 2, 2 * rx, 2 * ry);
            using (var than = new SolidBrush(mo ? GD.Hex("#d0d0d0") : MauXe.GetValueOrDefault(mau, GD.Hex("#f4f4f4"))))
                g.FillRectangle(than, px - rx, py - ry, 2 * rx, 2 * ry);
            using (var vien = new Pen(mo ? GD.Hex("#555555") : GD.Hex("#202020"))) g.DrawRectangle(vien, px - rx, py - ry, 2 * rx, 2 * ry);
            using (var kinh = new SolidBrush(GD.Hex("#5c6f80")))
            {
                if (huong == "ngang")                                     // kính lái, kính sau
                {
                    g.FillRectangle(kinh, RectangleF.FromLTRB(px + rx * 0.25f, py - ry + 2, px + rx * 0.55f, py + ry - 2));
                    g.FillRectangle(kinh, RectangleF.FromLTRB(px - rx * 0.7f, py - ry + 2, px - rx * 0.45f, py + ry - 2));
                }
                else
                {
                    g.FillRectangle(kinh, RectangleF.FromLTRB(px - rx + 2, py + ry * 0.25f, px + rx - 2, py + ry * 0.55f));
                    g.FillRectangle(kinh, RectangleF.FromLTRB(px - rx + 2, py - ry * 0.7f, px + rx - 2, py - ry * 0.45f));
                }
            }
            if (nhanXe != "") Chu(g, nhanXe, px, py - Math.Max(rx, ry) - 6, mo ? GD.Hex("#707070") : GD.Hex("#202020"), nho ? 7 : 8, true);
        });
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        if (nen == null || nen.Size != Size)
        {
            nen?.Dispose();
            nen = new Bitmap(Math.Max(1, Width), Math.Max(1, Height));
            using var g = Graphics.FromImage(nen);
            VeNen(g);
        }
        e.Graphics.DrawImage(nen, 0, 0);
        e.Graphics.SmoothingMode = SmoothingMode.AntiAlias;
        foreach (var v in dong) v(e.Graphics);
        using var vien = new Pen(Color.Gray);
        e.Graphics.DrawRectangle(vien, 0, 0, Width - 1, Height - 1);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) nen?.Dispose();
        base.Dispose(disposing);
    }
}

sealed class FormGoiXe(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    const double TocDo = 0.28;                           // đơn vị bản đồ / giây (hoạt hình)
    const int Rong = 440, Cao = 340;
    protected override string TieuDe => "Gọi xe";
    protected override Size KichThuocKhung => new(Rong + 8 + 262, Cao + 26);

    (double X, double Y) don, den;
    public string Trang = "san_sang";                    // san_sang | dang_tim | da_dat | dang_huy | het_cho | mat_xe | xong
    long? xeToi;                                         // mã xe được báo đặt thành công
    public JsonObject? Chuyen;                           // thông tin chuyến từ kq
    long phienToi;                                       // số thứ tự lần ghi (kq) của chuyến mình đặt
    public (List<(double X, double Y)> Ds, double T0, double Dai, double Moc)? HoatHinh;   // đường đi, lúc bắt đầu, độ dài, mốc tới điểm đón
    string? pha;
    int soDat, soDuoc;
    double tTim;
    BanDo banDo = null!;
    Label lblDon = null!, lblDen = null!, lblCuoc = null!, lblXeTrong = null!, lblCuaToi = null!, lblTx2 = null!;
    public Label LblTt = null!, LblTx = null!;
    public Button BtnDat = null!, BtnHuy = null!;
    OVe avatar = null!;
    readonly System.Windows.Forms.Timer nhipHh = new() { Interval = 40 };

    protected override void Ve(Panel khung)
    {
        (don, den) = NguoiGoiXe.DiemMacDinh(Ten);
        banDo = new BanDo(0, 4, Rong, Cao);
        banDo.MouseDown += (_, e) => Chon(e, e.Button == MouseButtons.Right ? "den" : "don");
        var goiY = GD.Nhan("Chuột trái: chọn điểm đón · chuột phải: chọn điểm đến", 0, Cao + 6, Rong, GD.Chu(8));
        goiY.ForeColor = GD.Hex("#404040");
        var phai = new Panel { Location = new Point(Rong + 8, 4), Size = new Size(262, Cao + 18), BackColor = Color.White, BorderStyle = BorderStyle.FixedSingle };
        Label N(string t, int y, Font f, int h = 18) => new() { Text = t, Location = new Point(8, y), Size = new Size(244, h), Font = f, BackColor = Color.White, AutoEllipsis = true };
        var tieuDe = N("Bạn muốn đi đâu?", 6, GD.Chu(12, true), 24);
        lblDon = N("", 34, GD.Chu(9));
        lblDen = N("", 54, GD.Chu(9));
        lblCuoc = N("", 76, GD.Chu(10, true), 20);
        BtnDat = new Button
        {
            Text = "ĐẶT XE", Location = new Point(8, 100), Size = new Size(244, 36), BackColor = BanDo.XanhGrab, ForeColor = Color.White,
            Font = GD.Chu(12, true), FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false,
        };
        BtnDat.FlatAppearance.BorderSize = 0;
        BtnDat.Click += (_, _) => DatXe();
        LblTt = new Label
        {
            Text = "Chưa kết nối", Location = new Point(8, 142), Size = new Size(244, 46), Font = GD.Chu(10, true),
            BackColor = GD.Hex("#e8e8e8"), TextAlign = ContentAlignment.MiddleCenter,
        };
        var the = new Panel { Location = new Point(8, 194), Size = new Size(244, 56), BackColor = GD.Hex("#f5f7f6"), BorderStyle = BorderStyle.Fixed3D };
        avatar = new OVe(4, 4, 44, 44) { BackColor = GD.Hex("#f5f7f6") };
        LblTx = new Label { Text = "Tài xế: –", Location = new Point(54, 4), Size = new Size(184, 20), Font = GD.Chu(10, true), BackColor = the.BackColor };
        lblTx2 = new Label { Text = "", Location = new Point(54, 24), Size = new Size(184, 28), Font = GD.Chu(8), BackColor = the.BackColor };
        the.Controls.AddRange([avatar, LblTx, lblTx2]);
        BtnHuy = GD.Nut("Hủy chuyến", GD.Hex("#f0f0f0"), 8, 256, 130, 26);
        BtnHuy.Font = GD.Thuong;
        BtnHuy.Click += (_, _) => Huy();
        lblXeTrong = N("", 290, GD.Chu(9));
        lblCuaToi = N("", 312, GD.Chu(8));
        lblCuaToi.ForeColor = GD.Hex("#505050");
        phai.Controls.AddRange([tieuDe, lblDon, lblDen, lblCuoc, BtnDat, LblTt, the, BtnHuy, lblXeTrong, lblCuaToi]);
        GD.Them(khung, banDo, goiY, phai);
        nhipHh.Tick += (_, _) => NhipHoatHinh();
        FormClosing += (_, _) => nhipHh.Stop();
        VeDiem();
        BatNut();
        VeDong();
    }

    // ------------------------------------------------------------ chọn điểm
    void Chon(MouseEventArgs e, string loai)
    {
        if (Trang is "dang_tim" or "da_dat") return;
        double x = (double)e.X / Rong, y = (double)e.Y / Cao;
        var p = BanDoPho.NgaTu.MinBy(q => (q.X - x) * (q.X - x) + (q.Y - y) * (q.Y - y));
        if (loai == "don" && p != den) don = p;
        else if (loai == "den" && p != don) den = p;
        VeDiem();
        VeDong();
    }

    void VeDiem()
    {
        (lblDon.Text, lblDon.ForeColor) = ($"● Đón:  {BanDoPho.TenDiem(don)}", BanDo.XanhGrab);
        (lblDen.Text, lblDen.ForeColor) = ($"● Đến:  {BanDoPho.TenDiem(den)}", BanDo.DoPin);
        lblCuoc.Text = $"{Chung.F(BanDoPho.Km(don, den), "F1")} km · cước dự kiến {Chung.Tien(BanDoPho.Cuoc(don, den))}";
    }

    void BatNut()
    {
        bool coTheDat = Kn != null && Trang is "san_sang" or "het_cho" or "mat_xe" or "xong";
        BtnDat.Enabled = coTheDat;
        BtnDat.BackColor = coTheDat ? BanDo.XanhGrab : GD.Hex("#7fcfa5");
        BtnHuy.Enabled = Trang == "da_dat";
        BtnHuy.Text = HoatHinh != null && DaToi() ? "Hoàn tất chuyến" : "Hủy chuyến";
    }

    void Bao(string text, Color nen, Color? chu = null) => (LblTt.Text, LblTt.BackColor, LblTt.ForeColor) = (text, nen, chu ?? Color.Black);

    // ------------------------------------------------------------ đặt / hủy
    public void DatXe()
    {
        if (Kn == null || Trang is "dang_tim" or "da_dat") return;
        Trang = "dang_tim";
        soDat++;
        tTim = Chung.Now;
        Bao("Đang tìm tài xế…", GD.Hex("#fff3c4"));
        NhatKy.Ghi($"Đặt xe: {BanDoPho.TenDiem(don)} -> {BanDoPho.TenDiem(den)}");
        BatNut();
        GuiYc("dat_xe", new JsonObject { ["don"] = BanDoPho.Json(don), ["den"] = BanDoPho.Json(den) });
    }

    void Huy()
    {
        if (Trang != "da_dat") return;
        Trang = "dang_huy";
        BatNut();
        GuiYc("huy");
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        if (loai == "dat_xe")
        {
            if (Chung.Dung(tin, "ok"))
            {
                soDuoc++;
                (Trang, xeToi, Chuyen, phienToi) = ("da_dat", Chung.SoL(tin, "xe"), tin, Chung.SoL(tin, "phien"));
                Bao($"Đặt thành công – tài xế {Chung.Chuoi(tin, "tai_xe")}", GD.Hex("#c8f0d2"), GD.XanhDam);
                NhatKy.Ghi($"  ĐẶT THÀNH CÔNG: {Chung.Chuoi(tin, "tai_xe")} {Chung.Chuoi(tin, "bien_so")}, đến sau {Chung.SoL(tin, "phut")} phút"
                           + $" ({Chung.F(rtt, "F1")} ms)");
                TheTaiXe(tin);
                var goc = XeSo(xeToi.Value) is { } xe ? BanDoPho.DocDiem(xe["vi_tri"]) : don;
                var ds = TuyenDuong.DuongDi(goc, don);
                double moc = TuyenDuong.DoDai(ds);
                ds = [.. ds, .. TuyenDuong.DuongDi(don, den).Skip(1)];
                HoatHinh = (ds, Chung.Now, TuyenDuong.DoDai(ds), moc);
                KiemXe(Tt);
            }
            else if (Chung.Chuoi(tin, "ma_loi") == "het_cho")
            {
                Trang = "het_cho";
                Bao("Hết chỗ – không còn xe trống", GD.Hex("#ffd0d0"), GD.DoDam);
                NhatKy.Ghi($"  HẾT CHỖ ({Chung.F(rtt, "F1")} ms)");
            }
            else
            {
                Trang = "het_cho";
                Bao($"Không đặt được: {Chung.Chuoi(tin, "ly_do")}", GD.Hex("#ffd0d0"), GD.DoDam);
                NhatKy.Ghi($"  từ chối: {Chung.Chuoi(tin, "ly_do")}");
            }
        }
        else
        {
            if (Chung.Dung(tin, "ok"))
            {
                bool xong = HoatHinh != null && DaToi();
                NhatKy.Ghi(xong ? "Hoàn tất chuyến, cảm ơn bạn!" : $"Đã hủy chuyến ({Chung.Chuoi(tin, "tai_xe")})");
                Bao(xong ? "Hoàn tất chuyến – cảm ơn bạn!" : "Đã hủy chuyến", GD.Hex("#e8e8e8"));
                Trang = "xong";
            }
            else
            {
                NhatKy.Ghi($"  hủy bị từ chối: {Chung.Chuoi(tin, "ly_do")}");
                Trang = "san_sang";
            }
            (xeToi, Chuyen, HoatHinh) = (null, null, null);
            TheTaiXe(null);
        }
        lblCuaToi.Text = $"Bạn: bấm đặt {soDat} lần, được báo thành công {soDuoc}";
        BatNut();
        VeDong();
    }

    void TheTaiXe(JsonObject? tin)
    {
        if (tin == null)
        {
            avatar.Ve = null;
            avatar.Invalidate();
            LblTx.Text = "Tài xế: –";
            lblTx2.Text = "";
            return;
        }
        string taiXe = Chung.Chuoi(tin, "tai_xe") ?? "?";
        avatar.Ve = g =>
        {
            using var b = new SolidBrush(BanDo.XanhGrab);
            g.FillEllipse(b, 0, 0, 36, 36);
            using var f = GD.Chu(16, true);
            using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
            g.DrawString(taiXe[..1], f, Brushes.White, new RectangleF(0, 0, 36, 36), sf);
        };
        avatar.Invalidate();
        LblTx.Text = $"{taiXe}  ★ 4.9";
        lblTx2.Text = $"{Chung.Chuoi(tin, "bien_so")} · xe {Chung.Chuoi(tin, "mau")}\nđến sau {Chung.SoL(tin, "phut")} phút · {Chung.Tien(Chung.SoL(tin, "cuoc"))}";
    }

    JsonObject? XeSo(long ma) => (Tt?["xe"] as JsonArray ?? []).OfType<JsonObject>().FirstOrDefault(x => Chung.SoL(x, "ma") == ma);

    /// <summary>Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq). Xe mình được báo đặt thành
    /// công mà lại ghi khách khác: bị "cướp" chỗ (tranh chấp); xe trống: máy chủ đặt lại.</summary>
    void KiemXe(JsonObject? tt)
    {
        if (Trang != "da_dat" || xeToi == null || tt == null || Chung.SoL(tt, "phien") < phienToi) return;
        var xe = (tt["xe"] as JsonArray ?? []).OfType<JsonObject>().FirstOrDefault(x => Chung.SoL(x, "ma") == xeToi);
        string? khach = Chung.Chuoi(xe, "khach");
        if (xe != null && khach == Ten) return;
        if (xe != null && khach != null)
        {
            Trang = "mat_xe";
            HoatHinh = null;
            Bao($"Tài xế {Chung.Chuoi(xe, "tai_xe")} đã nhận khách khác ({khach})!", GD.Hex("#ff6060"), Color.White);
            NhatKy.Ghi($"Tài xế {Chung.Chuoi(xe, "tai_xe")} đã nhận khách khác: {khach}");
        }
        else
        {
            Trang = "san_sang";
            (xeToi, Chuyen, HoatHinh) = (null, null, null);
            TheTaiXe(null);
            Bao("Chuyến đã kết thúc (máy chủ đặt lại)", GD.Hex("#e8e8e8"));
        }
        BatNut();
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        lblXeTrong.Text = $"Xe trống quanh bạn: {Chung.SoL(tt, "trong")}/{Chung.SoL(tt, "so_xe")}";
        if (Trang != "dang_tim" && LblTt.Text == "Chưa kết nối") Bao("Chọn điểm đón và bấm ĐẶT XE", GD.Hex("#e8e8e8"));
        KiemXe(tt);
        BatNut();
        VeDong();
    }

    public bool DaToi() => HoatHinh is { } h && (Chung.Now - h.T0) * TocDo >= h.Dai;

    void VeDong()
    {
        banDo.Xoa();
        if (HoatHinh is { } h)
        {
            double s = (Chung.Now - h.T0) * TocDo;
            if (s < h.Moc)                                // xe đang tới điểm đón: tuyến nét đứt
            {
                banDo.Tuyen(TuyenDuong.Doan(h.Ds, s, h.Moc), GD.Hex("#5f8fd8"), [4, 3]);
                banDo.Tuyen(TuyenDuong.DuongDi(don, den));
            }
            else banDo.Tuyen(TuyenDuong.Doan(h.Ds, s, h.Dai));
        }
        else banDo.Tuyen(TuyenDuong.DuongDi(don, den), GD.Hex("#8fb4ec"));
        banDo.Ghim(don.X, don.Y, BanDo.XanhGrab, "Đón");
        banDo.Ghim(den.X, den.Y, BanDo.DoPin, "Đến");
        foreach (var xe in (Tt?["xe"] as JsonArray ?? []).OfType<JsonObject>())
        {
            if (Chung.SoL(xe, "ma") == xeToi && HoatHinh != null) continue;
            var p = BanDoPho.DocDiem(xe["vi_tri"]);
            banDo.Xe(p.X, p.Y, Chung.Chuoi(xe, "mau") ?? "", Chung.Chuoi(xe, "tai_xe") ?? "", mo: !Chung.LaNull(xe["khach"]));
        }
        if (HoatHinh is { } hh && Chuyen != null)
        {
            var (diem, huong) = TuyenDuong.DiemTren(hh.Ds, Math.Min(hh.Dai, (Chung.Now - hh.T0) * TocDo));
            banDo.Xe(diem.X, diem.Y, Chung.Chuoi(Chuyen, "mau") ?? "", Chung.Chuoi(Chuyen, "tai_xe") ?? "", huong, noiBat: true);
        }
    }

    protected override void MoiNhip()
    {
        if (Trang == "dang_tim")
            Bao("Đang tìm tài xế" + new string('.', 1 + (int)((Chung.Now - tTim) * 5) % 3), GD.Hex("#fff3c4"));
    }

    void NhipHoatHinh()
    {
        if (HoatHinh is { } h)
        {
            double s = (Chung.Now - h.T0) * TocDo;
            string moi = s < h.Moc ? "toi_don" : s < h.Dai ? "di" : "den";
            if (moi != pha)
            {
                pha = moi;
                if (moi == "di")
                {
                    Bao($"Tài xế {Chung.Chuoi(Chuyen, "tai_xe")} đã đón bạn – đang đi", GD.Hex("#c8f0d2"), GD.XanhDam);
                    NhatKy.Ghi("Tài xế đã tới điểm đón");
                }
                else if (moi == "den")
                {
                    Bao("Đã tới nơi – bấm Hoàn tất chuyến", GD.Hex("#c8f0d2"), GD.XanhDam);
                    NhatKy.Ghi($"Đã tới {BanDoPho.TenDiem(den)}");
                    BatNut();
                }
            }
            VeDong();
        }
        else pha = null;
    }

    protected override void DaKetNoi(KetNoiKhach kn)
    {
        base.DaKetNoi(kn);
        BatNut();
        nhipHh.Start();
    }

    protected override void MatKetNoi(KetNoiKhach? kn, string lyDo = "Mất kết nối máy chủ")
    {
        base.MatKetNoi(kn, lyDo);
        if (Kn != null) return;
        Trang = "san_sang";
        (xeToi, Chuyen, HoatHinh) = (null, null, null);
        BatNut();
    }

    protected override string DongDinhKy()
    {
        var tt = Tt!;
        var xe = xeToi is { } m ? XeSo(m) : null;
        return $"{Chung.SoL(tt, "trong")}/{Chung.SoL(tt, "so_xe")} xe trống; trạng thái {Trang}"
               + (xe != null ? $", xe {Chung.SoL(xe, "ma")} {Chung.Chuoi(xe, "tai_xe")}" : "") + $"; bạn đặt {soDat}, được {soDuoc}";
    }
}
