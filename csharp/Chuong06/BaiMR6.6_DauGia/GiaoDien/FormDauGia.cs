using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_6;

// Bài mở rộng MR6.6 – Phòng đấu giá (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// phòng đấu giá: ảnh món hàng (vẽ bằng GDI+), giá hiện tại cỡ lớn, người dẫn đầu, đồng hồ đếm ngược (đỏ khi còn
// < 10 s, báo GIA HẠN), các nút "+1/+2/+5 bước", ô nhập giá, lịch sử trả giá, thông báo "Bạn bị trả giá cao hơn!" /
// "BẠN THẮNG!". Mọi số liệu đến từ cap_nhat của máy chủ (≤ 10 lần/giây); đồng hồ đếm ngược chạy cục bộ từ
// con_lai_ms của lần cap_nhat mới nhất.

/// <summary>Vẽ "Bình gốm hoa lam" (dùng ở phòng đấu giá và bảng điều khiển máy chủ).</summary>
static class BinhGom
{
    // nửa bề rộng của bình theo chiều cao (đơn vị trước khi nhân tỉ lệ): (y, nửa rộng)
    static readonly (float Y, float R)[] Dang =
        [(0, 24), (6, 22), (14, 15), (38, 16), (52, 34), (70, 50), (92, 55), (118, 47), (142, 32), (156, 22), (162, 25), (170, 25)];

    static float NuaRong(float y)
    {
        for (int i = 0; i + 1 < Dang.Length; i++)
        {
            var (y0, r0) = Dang[i];
            var (y1, r1) = Dang[i + 1];
            if (y0 <= y && y <= y1) return r0 + (r1 - r0) * (y - y0) / (y1 - y0);
        }
        return 0;
    }

    /// <summary>Tâm ngang cx, đỉnh miệng ở y = tren, tỉ lệ k; nen = true vẽ thêm nền tối, quầng đèn và bệ gỗ.</summary>
    public static void Ve(Graphics g, float w, float h, float cx, float tren, float k = 1f, bool nen = true)
    {
        var lam = GD.Hex("#1f4fa0");
        if (nen)
        {
            using (var b = new SolidBrush(GD.Hex("#23232f"))) g.FillRectangle(b, 0, 0, w, h);
            string[] quang = ["#2c2c3c", "#363648", "#404056", "#4a4a62"];
            for (int i = 0; i < quang.Length; i++)
            {
                float r = (1f - i * 0.2f) * w * 0.5f;
                using var b = new SolidBrush(GD.Hex(quang[i]));
                g.FillEllipse(b, cx - r, tren + 80 * k - r * 0.9f, 2 * r, 1.8f * r);
            }
            float day = tren + 170 * k;
            PointF[] be = [new(cx - 70 * k, day), new(cx + 70 * k, day), new(cx + 82 * k, day + 22 * k), new(cx - 82 * k, day + 22 * k)];
            using (var b = new SolidBrush(GD.Hex("#6b4226"))) g.FillPolygon(b, be);
            using (var p = new Pen(GD.Hex("#3e2412"))) g.DrawPolygon(p, be);
            using (var b = new SolidBrush(GD.Hex("#4e2f1a"))) g.FillRectangle(b, cx - 82 * k, day + 22 * k, 164 * k, h);
            using (var b = new SolidBrush(GD.Hex("#1a1a1a"))) g.FillEllipse(b, cx - 48 * k, day - 5 * k, 96 * k, 11 * k);
        }
        var vo = Dang.Select(d => new PointF(cx - d.R * k, tren + d.Y * k))
                     .Concat(Dang.Reverse().Select(d => new PointF(cx + d.R * k, tren + d.Y * k))).ToArray();
        using (var b = new SolidBrush(GD.Hex("#f5f2e8"))) g.FillClosedCurve(b, vo, System.Drawing.Drawing2D.FillMode.Alternate, 0.3f);
        using (var p = new Pen(GD.Hex("#8c96a8"))) g.DrawClosedCurve(p, vo, 0.3f, System.Drawing.Drawing2D.FillMode.Alternate);
        using (var b = new SolidBrush(GD.Hex("#dcd8cc"))) g.FillEllipse(b, cx - 24 * k, tren - 4 * k, 48 * k, 9 * k);
        using (var p = new Pen(GD.Hex("#8c96a8"))) g.DrawEllipse(p, cx - 24 * k, tren - 4 * k, 48 * k, 9 * k);
        using var but = new Pen(lam);
        foreach (float y in new float[] { 10, 36, 54, 60, 138, 150 })           // đường viền men lam quanh thân
        {
            float r = NuaRong(y) * k - 1;
            using var p = new Pen(lam, Math.Max(1, (int)((y is 54 or 138 ? 2 : 1) * k)));
            g.DrawLine(p, cx - r, tren + y * k, cx + r, tren + y * k);
        }
        for (int i = -3; i <= 3; i++)                                           // cánh sen quanh cổ
        {
            float x = cx + i * 9 * k;
            g.DrawArc(but, x - 5 * k, tren + 18 * k, 10 * k, 20 * k, 180, 180);
        }
        float tamY = tren + 96 * k;                                             // bông hoa giữa thân
        using (var canh = new SolidBrush(GD.Hex("#5b82c4")))
            for (int i = 0; i < 8; i++)
            {
                double goc = i * Math.PI / 4;
                float x = cx + 15 * k * (float)Math.Cos(goc), y = tamY + 15 * k * (float)Math.Sin(goc);
                g.FillEllipse(canh, x - 7 * k, y - 7 * k, 14 * k, 14 * k);
                g.DrawEllipse(but, x - 7 * k, y - 7 * k, 14 * k, 14 * k);
            }
        using var bLam = new SolidBrush(lam);
        g.FillEllipse(bLam, cx - 8 * k, tamY - 8 * k, 16 * k, 16 * k);
        using (var day2 = new Pen(lam, 2))
            foreach (int d in new[] { -1, 1 })                                  // dây lá cuốn hai bên
            {
                PointF[] la = [new(cx + d * 24 * k, tamY), new(cx + d * 34 * k, tamY - 16 * k), new(cx + d * 44 * k, tamY - 4 * k),
                               new(cx + d * 40 * k, tamY + 14 * k), new(cx + d * 30 * k, tamY + 22 * k)];
                g.DrawCurve(day2, la, 0.5f);
                g.FillEllipse(bLam, cx + d * 40 * k - 4 * k, tamY - 12 * k, 8 * k, 8 * k);
            }
        for (int i = -4; i <= 4; i++)                                           // cánh sen chân bình
        {
            float x = cx + i * 8 * k;
            g.DrawLines(but, [new PointF(x - 4 * k, tren + 150 * k), new PointF(x, tren + 140 * k), new PointF(x + 4 * k, tren + 150 * k)]);
        }
    }

    /// <summary>Đếm ngược dạng mm:ss (dưới 10 s thêm phần mười).</summary>
    public static string DongHo(double giay)
    {
        if (giay < 10) return "00:" + giay.ToString("00.0", CultureInfo.InvariantCulture);
        int s = (int)giay;
        return $"{s / 60:00}:{s % 60:00}";
    }
}

sealed class FormDauGia(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    protected override string TieuDe => "Phòng đấu giá";
    protected override Size KichThuocKhung => new(584, 400);

    public double? HetHan;                               // thời điểm (đồng hồ đơn điệu) hết phiên theo cap_nhat
    long soLanCu;
    bool danDau;
    int soTra, soNhan;
    double hanBaoGiaHan;
    Label lblTenMon = null!, lblGiaKd = null!, lblCuaToi = null!, lblGia = null!, lblDanDau = null!;
    public Label LblDongHo = null!, LblGiaHan = null!, LblThongBao = null!;
    public readonly List<(int N, Button Nut)> NutBuoc = [];
    TextBox txtGia = null!;
    Button btnTra = null!;
    ListBox ls = null!;
    readonly List<(string Chu, bool CuaToi)> dongLs = [];

    protected override void Ve(Panel khung)
    {
        var anh = new OVe(0, 4, 230, 240) { Ve = g => BinhGom.Ve(g, 230, 240, 115, 26) };
        lblTenMon = GD.Nhan("–", 0, 250, 236, GD.Chu(11, true), 22);
        lblGiaKd = GD.Nhan("", 0, 274, 236, GD.Chu(9));
        lblCuaToi = GD.Nhan("Bạn: chưa trả giá", 0, 294, 236, GD.Chu(9));
        const int x = 246, w = 334;
        var tieuDe = GD.Nhan("GIÁ HIỆN TẠI", x, 4, 200, GD.Chu(9, true));
        tieuDe.ForeColor = GD.Hex("#404040");
        lblGia = GD.Nhan("–", x, 20, w, GD.Chu(24, true), 44);
        lblGia.ForeColor = GD.Hex("#8b0000");
        lblDanDau = GD.Nhan("Dẫn đầu: –", x, 66, w, GD.Chu(10), 20);
        LblDongHo = GD.Nhan("–", x + 62, 88, 150, GD.ChuCoDinh(20, true), 36);
        LblGiaHan = GD.Nhan("", x + 214, 98, 110, GD.Chu(10, true), 20);
        LblGiaHan.ForeColor = GD.DoDam;
        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(x, 128), Size = new Size(w, 30), Font = GD.Chu(11, true), BackColor = GD.Hex("#e0e0e0"),
            TextAlign = ContentAlignment.MiddleCenter,
        };
        GD.Them(khung, anh, lblTenMon, lblGiaKd, lblCuaToi, tieuDe, lblGia, lblDanDau, GD.Nhan("Còn lại ", x, 98, 60, GD.Chu(10)),
                LblDongHo, LblGiaHan, LblThongBao);
        int bx = x;
        foreach (int n in new[] { 1, 2, 5 })
        {
            var b = GD.Nut($"+{n} bước", n == 1 ? GD.Cam : GD.XanhDuong, bx, 164, 108, 42);
            b.Click += (_, _) => Tang(n);
            NutBuoc.Add((n, b));
            GD.Them(khung, b);
            bx += 112;
        }
        txtGia = GD.O("", x + 84, 216, 110);
        btnTra = GD.Nut("Trả giá", GD.Hex("#ff8c00"), x + 200, 213, 80, 26);
        btnTra.Click += (_, _) => TraONhap();
        var gLs = GD.Nhom("Lịch sử trả giá (mới nhất trên cùng)", x, 246, w, 150);
        ls = new ListBox
        {
            Location = new Point(8, 20), Size = new Size(w - 16, 122), Font = GD.ChuCoDinh(9), DrawMode = DrawMode.OwnerDrawFixed,
            IntegralHeight = false,
        };
        ls.DrawItem += (_, e) =>                         // giá của mình tô xanh
        {
            if (e.Index < 0 || e.Index >= dongLs.Count) return;
            e.DrawBackground();
            var (chu, cuaToi) = dongLs[e.Index];
            TextRenderer.DrawText(e.Graphics, chu, ls.Font, e.Bounds, cuaToi ? GD.Hex("#0040c0") : Color.Black, TextFormatFlags.Left);
        };
        GD.Them(gLs, ls);
        GD.Them(khung, GD.Nhan("Giá của bạn:", x, 219), txtGia, btnTra, gLs);
        BatNut(false);
    }

    // ------------------------------------------------------------ trả giá
    void BatNut(bool bat)
    {
        foreach (var (_, b) in NutBuoc) b.Enabled = bat;
        btnTra.Enabled = bat;
    }

    public void Tang(int n)
    {
        if (Tt != null) Tra(Chung.SoL(Tt, "gia") + n * Chung.SoL(Tt["mon"] as JsonObject, "buoc_gia"));
    }

    void TraONhap()
    {
        string s = txtGia.Text.Replace(".", "").Replace(" ", "").Replace("đ", "");
        if (!long.TryParse(s, NumberStyles.Integer, CultureInfo.InvariantCulture, out long gia))
        {
            NhatKy.Ghi("Giá phải là số nguyên (đồng)");
            return;
        }
        Tra(gia);
    }

    void Tra(long gia)
    {
        soTra++;
        NhatKy.Ghi($"Trả {Chung.Tien(gia)}…");
        GuiYc("tra_gia", new JsonObject { ["gia"] = gia });
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        long gia = Chung.SoL(truong, "gia");
        if (Chung.Dung(tin, "ok"))
        {
            soNhan++;
            NhatKy.Ghi($"  chấp nhận {Chung.Tien(gia)} ({Chung.F(rtt, "F1")} ms)" + (Chung.Dung(tin, "gia_han") ? " – phiên được GIA HẠN" : ""));
            Bao("Bạn đang dẫn đầu", GD.Hex("#70d070"), Color.Black);
            danDau = true;                               // cap_nhat sau đó báo nếu bị vượt (hoặc bị ghi đè)
        }
        else
        {
            NhatKy.Ghi($"  từ chối {Chung.Tien(gia)}: {Chung.Chuoi(tin, "ly_do")}");
            Bao("Bị từ chối: " + (Chung.Chuoi(tin, "ma_loi") == "gia_thap" ? "giá đã thay đổi" : Chung.Chuoi(tin, "ly_do") ?? ""),
                GD.Hex("#ffb347"), Color.Black);
        }
        lblCuaToi.Text = $"Bạn: trả {soTra} lần, được nhận {soNhan}";
    }

    void Bao(string text, Color nen, Color chu) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu);

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        var mon = tt["mon"] as JsonObject;
        long gia = Chung.SoL(tt, "gia"), buoc = Chung.SoL(mon, "buoc_gia"), soLan = Chung.SoL(tt, "so_lan");
        bool ketThuc = Chung.Dung(tt, "ket_thuc");
        lblTenMon.Text = Chung.Chuoi(mon, "ten") ?? "";
        lblGiaKd.Text = $"Khởi điểm {Chung.Tien(Chung.SoL(mon, "gia_khoi_diem"))} · bước {Chung.Tien(buoc)}";
        lblGia.Text = Chung.Tien(gia);
        string? ai = Chung.Chuoi(tt, "nguoi_dan_dau");
        lblDanDau.Text = $"Dẫn đầu: {ai ?? "–"}  ({soLan} lượt trả)";
        foreach (var (n, b) in NutBuoc) b.Text = $"+{n} bước\n{Chung.Tien(gia + n * buoc)}";
        double? moi = Chung.SoD(tt["con_lai_ms"]) is { } c ? Chung.Now + c / 1000 : null;
        if (moi != null && HetHan != null && moi > HetHan + 0.5 && !ketThuc && soLan > soLanCu)   // hạn lùi ra sau vì có giá mới
        {
            hanBaoGiaHan = Chung.Now + 3.0;
            NhatKy.Ghi($"GIA HẠN: còn {Chung.F(Chung.SoD(tt, "con_lai_ms") / 1000, "F1")} s");
        }
        (HetHan, soLanCu) = (moi, soLan);
        dongLs.Clear();
        foreach (var x in (tt["lich_su"] as JsonArray ?? []).OfType<JsonObject>())
        {
            string nguoi = Chung.Chuoi(x, "nguoi") ?? "";
            dongLs.Add(($"{Chung.Chuoi(x, "gio")} {nguoi,-6} {Chung.Tien(Chung.SoL(x, "gia")),14}" + (Chung.Dung(x, "gia_han") ? " gia hạn" : ""),
                        nguoi == Ten));
        }
        ls.BeginUpdate();
        ls.Items.Clear();
        foreach (var d in dongLs) ls.Items.Add(d.Chu);
        ls.EndUpdate();
        bool cuaToi = ai == Ten;
        if (ketThuc)
        {
            BatNut(false);
            if (cuaToi) Bao($"BẠN THẮNG! {Chung.Tien(gia)}", GD.Hex("#ffd700"), Color.Black);
            else Bao($"Kết thúc – {ai ?? "không ai"} thắng", GD.Hex("#909090"), Color.White);
        }
        else
        {
            BatNut(true);
            if (danDau && !cuaToi)
            {
                Bao($"Bạn bị trả giá cao hơn! ({ai})", GD.Hex("#ff6060"), Color.White);
                NhatKy.Ghi($"Bạn bị trả giá cao hơn: {ai} {Chung.Tien(gia)}");
            }
            else if (cuaToi) Bao("Bạn đang dẫn đầu", GD.Hex("#70d070"), Color.Black);
            else if (soLan == 0 || LblThongBao.Text == "Chưa kết nối") Bao("Phiên đang mở – mời trả giá", GD.Hex("#e0e0e0"), Color.Black);
        }
        danDau = cuaToi && !ketThuc;
    }

    protected override void MoiNhip()
    {
        if (Tt == null) return;
        if (Chung.Dung(Tt, "ket_thuc")) (LblDongHo.Text, LblDongHo.ForeColor) = ("00:00", GD.Hex("#606060"));
        else if (HetHan == null) (LblDongHo.Text, LblDongHo.ForeColor) = ("∞", Color.Black);
        else
        {
            double con = Math.Max(0, HetHan.Value - Chung.Now);
            (LblDongHo.Text, LblDongHo.ForeColor) = (BinhGom.DongHo(con), con < 10 ? GD.DoDam : GD.XanhDam);
        }
        LblGiaHan.Text = Chung.Now < hanBaoGiaHan ? "GIA HẠN!" : "";
    }

    protected override string DongDinhKy()
    {
        var tt = Tt!;
        string con = HetHan is { } h ? $"{Chung.F(Math.Max(0, h - Chung.Now), "F1")} s" : "∞";
        return $"giá {Chung.Tien(Chung.SoL(tt, "gia"))}, dẫn đầu {Chung.Chuoi(tt, "nguoi_dan_dau") ?? "–"}, còn {con},"
               + $" {(Chung.Dung(tt, "ket_thuc") ? "KẾT THÚC" : "đang đấu")}; bạn trả {soTra}, nhận {soNhan}";
    }
}
