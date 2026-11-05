using System.Drawing;
using System.Drawing.Drawing2D;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Vòng quay may mắn (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// vòng quay bằng GDI+: bánh xe chia ô (mỗi món quà một ô màu, xen giữa là ô "May mắn lần sau"), kim chỉ ở đỉnh, đèn
// viền nhấp nháy khi quay, nút QUAY. Bấm QUAY: bánh xe quay tự do ngay, yêu cầu {"loai":"quay"} đi tới máy chủ; khi
// có kết quả, bánh xe giảm tốc (ease-out, 2,6 s) và dừng đúng ô kết quả (món trúng, hoặc một ô "may mắn lần sau").
// Bên phải: số quà còn lại, ảnh từng món với trạng thái còn / đã trao cho ai, danh sách người trúng trực tiếp (từ
// cap_nhat ≤ 10 lần/giây).

/// <summary>Ảnh 10 món quà vẽ đơn giản bằng GDI+ (như ve_qua của bản Python).</summary>
static class AnhQua
{
    public static readonly string[] MauQua = ["#e6194b", "#f58231", "#ffe119", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4", "#f032e6",
                                              "#9a6324", "#469990"];
    public static readonly string[] TenNgan = ["Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm",
                                               "Loa", "500k"];

    static RectangleF Hcn(float x0, float y0, float x1, float y1) => RectangleF.FromLTRB(x0, y0, x1, y1);

    /// <summary>Vẽ món quà i quanh tâm (cx, cy), nửa cạnh s. xam = true: đã trao (vẽ màu xám).</summary>
    public static void Ve(Graphics g, int i, float cx, float cy, float s, bool xam = false)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        var m = xam ? GD.Hex("#9a9a9a") : GD.Hex(MauQua[i]);
        var den = xam ? GD.Hex("#7a7a7a") : GD.Hex("#303030");
        using var bm = new SolidBrush(m);
        using var bd = new SolidBrush(den);
        using var pd = new Pen(den);
        using var pd2 = new Pen(den, 2);
        using var pd3 = new Pen(den, 3);
        using var pm3 = new Pen(m, 3) { LineJoin = LineJoin.Round };
        void O(Brush? b, Pen? p, float x0, float y0, float x1, float y1)
        {
            if (b != null) g.FillEllipse(b, Hcn(x0, y0, x1, y1));
            if (p != null) g.DrawEllipse(p, Hcn(x0, y0, x1, y1));
        }
        void Hn(Brush? b, Pen? p, float x0, float y0, float x1, float y1)
        {
            if (b != null) g.FillRectangle(b, Hcn(x0, y0, x1, y1));
            if (p != null) g.DrawRectangle(p, x0, y0, x1 - x0, y1 - y0);
        }
        switch (i)
        {
            case 0:                                      // xe đạp
                foreach (float dx in new[] { -0.55f, 0.55f }) O(null, pd2, cx + (dx - 0.4f) * s, cy - 0.1f * s, cx + (dx + 0.4f) * s, cy + 0.7f * s);
                g.DrawLines(pm3, new PointF[] { new(cx - 0.55f * s, cy + 0.3f * s), new(cx - 0.1f * s, cy - 0.35f * s), new(cx + 0.4f * s, cy - 0.35f * s), new(cx + 0.55f * s, cy + 0.3f * s) });
                g.DrawLines(pm3, new PointF[] { new(cx - 0.1f * s, cy - 0.35f * s), new(cx, cy + 0.3f * s), new(cx + 0.4f * s, cy - 0.35f * s) });
                g.DrawLine(pd3, cx - 0.25f * s, cy - 0.5f * s, cx + 0.05f * s, cy - 0.5f * s);
                g.DrawLine(pd3, cx + 0.35f * s, cy - 0.6f * s, cx + 0.55f * s, cy - 0.6f * s);
                break;
            case 1:                                      // điện thoại
                Hn(bd, null, cx - 0.42f * s, cy - 0.85f * s, cx + 0.42f * s, cy + 0.85f * s);
                Hn(bm, null, cx - 0.34f * s, cy - 0.68f * s, cx + 0.34f * s, cy + 0.6f * s);
                using (var b = new SolidBrush(GD.Hex("#d0d0d0"))) O(b, null, cx - 0.07f * s, cy + 0.66f * s, cx + 0.07f * s, cy + 0.8f * s);
                break;
            case 2:                                      // laptop
                Hn(bd, null, cx - 0.7f * s, cy - 0.65f * s, cx + 0.7f * s, cy + 0.3f * s);
                Hn(bm, null, cx - 0.6f * s, cy - 0.55f * s, cx + 0.6f * s, cy + 0.2f * s);
                PointF[] de = [new(cx - 0.7f * s, cy + 0.35f * s), new(cx + 0.7f * s, cy + 0.35f * s), new(cx + 0.9f * s, cy + 0.6f * s), new(cx - 0.9f * s, cy + 0.6f * s)];
                using (var b = new SolidBrush(GD.Hex("#b0b0b0"))) g.FillPolygon(b, de);
                g.DrawPolygon(pd, de);
                break;
            case 3:                                      // tai nghe
                g.DrawArc(pd3, Hcn(cx - 0.65f * s, cy - 0.75f * s, cx + 0.65f * s, cy + 0.55f * s), 180, 180);
                foreach (float dx in new[] { -0.65f, 0.65f }) Hn(bm, pd, cx + (dx - 0.18f) * s, cy - 0.1f * s, cx + (dx + 0.18f) * s, cy + 0.6f * s);
                break;
            case 4:                                      // đồng hồ
                using (var b = new SolidBrush(GD.Hex("#6b4226"))) Hn(b, null, cx - 0.25f * s, cy - 0.9f * s, cx + 0.25f * s, cy + 0.9f * s);
                using (var p = new Pen(m, 3)) O(Brushes.White, p, cx - 0.55f * s, cy - 0.55f * s, cx + 0.55f * s, cy + 0.55f * s);
                g.DrawLine(pd2, cx, cy, cx, cy - 0.38f * s);
                g.DrawLine(pd2, cx, cy, cx + 0.28f * s, cy + 0.1f * s);
                break;
            case 5:                                      // máy ảnh
                Hn(bd, null, cx - 0.3f * s, cy - 0.65f * s, cx + 0.15f * s, cy - 0.4f * s);
                Hn(bm, pd, cx - 0.85f * s, cy - 0.45f * s, cx + 0.85f * s, cy + 0.6f * s);
                O(bd, Pens.White, cx - 0.42f * s, cy - 0.33f * s, cx + 0.42f * s, cy + 0.5f * s);
                using (var b = new SolidBrush(GD.Hex("#5080c0"))) O(b, null, cx - 0.2f * s, cy - 0.12f * s, cx + 0.2f * s, cy + 0.28f * s);
                break;
            case 6:                                      // ba lô
                g.DrawArc(pd2, Hcn(cx - 0.4f * s, cy - 0.95f * s, cx + 0.4f * s, cy - 0.45f * s), 180, 180);
                Hn(bm, pd, cx - 0.6f * s, cy - 0.7f * s, cx + 0.6f * s, cy + 0.85f * s);
                Hn(bd, null, cx - 0.4f * s, cy + 0.15f * s, cx + 0.4f * s, cy + 0.65f * s);
                break;
            case 7:                                      // nồi cơm điện
                using (var b = new SolidBrush(GD.Hex("#e0e0e0"))) O(b, pd, cx - 0.8f * s, cy - 0.55f * s, cx + 0.8f * s, cy - 0.05f * s);
                Hn(bm, pd, cx - 0.8f * s, cy - 0.3f * s, cx + 0.8f * s, cy + 0.7f * s);
                Hn(bd, null, cx - 0.15f * s, cy - 0.75f * s, cx + 0.15f * s, cy - 0.5f * s);
                Hn(Brushes.White, null, cx - 0.25f * s, cy + 0.1f * s, cx + 0.25f * s, cy + 0.4f * s);
                break;
            case 8:                                      // loa
                Hn(bm, pd, cx - 0.55f * s, cy - 0.9f * s, cx + 0.55f * s, cy + 0.9f * s);
                O(bd, null, cx - 0.18f * s, cy - 0.72f * s, cx + 0.18f * s, cy - 0.36f * s);
                O(bd, Pens.White, cx - 0.42f * s, cy - 0.15f * s, cx + 0.42f * s, cy + 0.69f * s);
                break;
            default:                                     // phiếu mua hàng 500k
                Hn(bm, pd, cx - 0.9f * s, cy - 0.5f * s, cx + 0.9f * s, cy + 0.5f * s);
                using (var p = new Pen(Color.White) { DashPattern = [2, 2] }) g.DrawLine(p, cx + 0.4f * s, cy - 0.5f * s, cx + 0.4f * s, cy + 0.5f * s);
                using (var f = GD.Chu(Math.Max(5, (int)(s * 0.38)), true))
                using (var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center })
                    g.DrawString("500K", f, Brushes.White, cx - 0.25f * s, cy, sf);
                break;
        }
    }

    /// <summary>Các ô của bánh xe: mã quà hoặc null ("may mắn lần sau"); quà rải đều, tối thiểu 8 ô.</summary>
    public static int?[] BoTriO(int soQua)
    {
        int n = Math.Max(2 * soQua, 8);
        var o = new int?[n];
        for (int i = 0; i < soQua; i++) o[(int)Math.Round((double)i * n / soQua, MidpointRounding.ToEven)] = i;
        return o;
    }
}

sealed class FormVongQuay(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    const int R = 168;                                   // bán kính bánh xe
    protected override string TieuDe => "Vòng quay may mắn";
    protected override Size KichThuocKhung => new(690, 440);

    public string NhanHien = ten;                        // tên hiển thị trong kịch bản demo ("Bình (máy 2)")
    double goc = 90;                                     // góc xoay bánh xe (độ, ngược chiều kim đồng hồ)
    public string? Pha;                                  // null | "tu_do" | "dung"
    JsonObject? ttVe;                                    // trạng thái dùng để vẽ bánh xe
    JsonObject? choKq;                                   // kết quả chờ đến lúc bánh xe dừng
    (string Chu, Color Nen, Color Mau)? ketQua;          // dòng thông báo kết quả cuối
    int nhip;
    double tBam, tDung, goc0, gocDich;
    readonly System.Windows.Forms.Timer hoatHinh = new() { Interval = 30 };
    OVe banhXe = null!;
    public Button BtnQuay = null!;
    public Label LblThongBao = null!;
    Label lblLuat = null!, lblCon = null!, lblDaQuay = null!;
    readonly List<(OVe O, Label Nhan)> oQua = [];
    JsonArray? qua;
    string daVeQua = "";
    public ListBox Ls = null!;
    readonly HashSet<int> dongCuaToi = [];

    protected override void Ve(Panel khung)
    {
        banhXe = new OVe(0, 4, 2 * R + 40, 2 * R + 44) { BackColor = GD.Hex("#2b1055"), Ve = VeBanhXe };
        BtnQuay = new Button
        {
            Text = "QUAY", Location = new Point(0, 390), Size = new Size(140, 46), Font = GD.Chu(16, true), BackColor = GD.Hex("#d62828"),
            ForeColor = Color.White, FlatStyle = FlatStyle.Flat, UseVisualStyleBackColor = false, Enabled = false,
        };
        BtnQuay.Click += (_, _) => Quay();
        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(148, 390), Size = new Size(228, 46), Font = GD.Chu(11, true),
            BackColor = GD.Hex("#e0e0e0"), TextAlign = ContentAlignment.MiddleCenter,
        };
        var tieuDe = GD.Nhan("VÒNG QUAY MAY MẮN", 386, 4, 300, GD.Chu(15, true), 30);
        tieuDe.ForeColor = GD.Hex("#7a1fa2");
        lblLuat = GD.Nhan("Mỗi người 1 lượt", 386, 34, 300, GD.Chu(9), 18);
        lblCon = GD.Nhan("–", 386, 54, 300, GD.Chu(12, true), 22);
        lblCon.ForeColor = GD.DoDam;
        lblDaQuay = GD.Nhan("–", 386, 78, 300, GD.Chu(9), 18);
        var gQua = GD.Nhom("Quà tặng", 386, 98, 300, 204);
        for (int i = 0; i < Qua.TenQua.Length; i++)
        {
            int x = 8 + i % 2 * 146, y = 18 + i / 2 * 36, so = i;
            var o = new OVe(x, y, 36, 34) { BackColor = Color.White };
            o.Ve = gr => VeO(gr, so);
            var nh = GD.Nhan(Qua.TenQua[i], x + 40, y, 104, GD.Chu(8), 34);
            oQua.Add((o, nh));
            GD.Them(gQua, o, nh);
        }
        var gTrung = GD.Nhom("Người trúng thưởng (trực tiếp)", 386, 306, 300, 132);
        Ls = new ListBox
        {
            Location = new Point(8, 18), Size = new Size(284, 108), Font = GD.ChuCoDinh(9), DrawMode = DrawMode.OwnerDrawFixed,
            IntegralHeight = false, SelectionMode = SelectionMode.None,
        };
        Ls.DrawItem += (_, e) =>                         // tên mình tô xanh
        {
            if (e.Index < 0) return;
            e.DrawBackground();
            using var b = new SolidBrush(dongCuaToi.Contains(e.Index) ? GD.Hex("#0040c0") : Color.Black);
            e.Graphics.DrawString(Ls.Items[e.Index]?.ToString(), Ls.Font, b, e.Bounds.Location);
        };
        GD.Them(gTrung, Ls);
        GD.Them(khung, banhXe, BtnQuay, LblThongBao, tieuDe, lblLuat, lblCon, lblDaQuay, gQua, gTrung);
        hoatHinh.Tick += (_, _) => KhungHinh();
        FormClosing += (_, _) => hoatHinh.Stop();
    }

    void VeO(Graphics gr, int i)
    {
        if (qua == null || i >= qua.Count) return;
        bool con = Chung.Dung(qua[i] as JsonObject, "con");
        var o = oQua[i].O;
        if (!con)
            using (var b = new SolidBrush(GD.Hex("#e8e8e8"))) gr.FillRectangle(b, 0, 0, o.Width, o.Height);
        AnhQua.Ve(gr, i, o.Width / 2f, o.Height / 2f, 15, xam: !con);
        gr.DrawRectangle(Pens.Gray, 0, 0, o.Width - 1, o.Height - 1);
    }

    // ------------------------------------------------------------ bánh xe
    void VeBanhXe(Graphics g)
    {
        float cx = R + 20, cy = R + 26;
        var tt = ttVe;
        int soQua = tt != null ? (int)Chung.SoL(tt, "so_qua") : 10;
        var con = new Dictionary<int, bool>();
        foreach (var q in (tt?["qua"] as JsonArray ?? []).OfType<JsonObject>()) con[(int)Chung.SoL(q, "ma")] = Chung.Dung(q, "con");
        var o = AnhQua.BoTriO(soQua);
        float ext = 360f / o.Length;
        using (var b = new SolidBrush(GD.Hex("#f4b400"))) g.FillEllipse(b, cx - R - 14, cy - R - 14, 2 * R + 28, 2 * R + 28);
        using (var p = new Pen(GD.Hex("#a86b00"), 3)) g.DrawEllipse(p, cx - R - 14, cy - R - 14, 2 * R + 28, 2 * R + 28);
        using var vien = new Pen(Color.White, 2);
        using var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        for (int j = 0; j < o.Length; j++)
        {
            float batDau = (float)goc + j * ext;
            string mau, chu, mauChu;
            if (o[j] is not { } ma) (mau, chu, mauChu) = (j % 2 == 1 ? "#fff7e6" : "#ffe8f0", "May mắn\nlần sau", "#7a1fa2");
            else if (con.GetValueOrDefault(ma, true)) (mau, chu, mauChu) = (AnhQua.MauQua[ma], AnhQua.TenNgan[ma], ma is 2 or 4 ? "#202020" : "#ffffff");
            else (mau, chu, mauChu) = ("#bdbdbd", AnhQua.TenNgan[ma] + "\n(đã trao)", "#606060");
            using (var b = new SolidBrush(GD.Hex(mau))) g.FillPie(b, cx - R, cy - R, 2 * R, 2 * R, -batDau, -ext);   // GDI+: góc theo chiều kim đồng hồ
            g.DrawPie(vien, cx - R, cy - R, 2 * R, 2 * R, -batDau, -ext);
            double giua = (batDau + ext / 2) * Math.PI / 180;
            float r = R * 0.62f, x = cx + r * (float)Math.Cos(giua), y = cy - r * (float)Math.Sin(giua);
            float gocChu = ((batDau + ext / 2) % 360 + 360) % 360;
            if (gocChu is > 90 and < 270) gocChu -= 180;           // chữ ở nửa trái: lật cho dễ đọc
            var st = g.Save();
            g.TranslateTransform(x, y);
            g.RotateTransform(-gocChu);
            using (var f = GD.Chu(o[j] == null || o.Length > 12 ? 7 : 8, true))
            using (var b = new SolidBrush(GD.Hex(mauChu)))
                g.DrawString(chu, f, b, 0, 0, sf);
            g.Restore(st);
        }
        for (int k = 0; k < 24; k++)                     // đèn viền: nhấp nháy khi quay
        {
            double a = k * 15 * Math.PI / 180;
            float x = cx + (R + 7) * (float)Math.Cos(a), y = cy - (R + 7) * (float)Math.Sin(a);
            bool sang = Pha != null ? (k + nhip / 3) % 2 == 0 : k % 2 == 0;
            using var b = new SolidBrush(GD.Hex(sang ? "#fffde0" : "#c08000"));
            g.FillEllipse(b, x - 4, y - 4, 8, 8);
        }
        using (var b = new SolidBrush(GD.Hex("#d62828"))) g.FillEllipse(b, cx - 30, cy - 30, 60, 60);
        using (var p = new Pen(Color.White, 3)) g.DrawEllipse(p, cx - 30, cy - 30, 60, 60);
        using (var f = GD.Chu(8, true)) g.DrawString("MAY\nMẮN", f, Brushes.White, cx, cy, sf);
        PointF[] kim = [new(cx - 15, cy - R - 24), new(cx + 15, cy - R - 24), new(cx, cy - R + 12)];   // kim chỉ ở đỉnh
        using (var b = new SolidBrush(GD.Hex("#d62828"))) g.FillPolygon(b, kim);
        using (var p = new Pen(Color.White, 2)) g.DrawPolygon(p, kim);
    }

    public void Quay()
    {
        if (Kn == null || Pha != null) return;
        ttVe = Tt;
        (Pha, tBam, choKq) = ("tu_do", Chung.Now, null);
        (BtnQuay.Enabled, BtnQuay.Text) = (false, "ĐANG QUAY");
        Bao("Đang quay…", GD.Hex("#fff3c4"), Color.Black);
        NhatKy.Ghi("Bấm QUAY");
        GuiYc("quay");
        hoatHinh.Start();
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        string mo = Chung.Dung(tin, "ok") ? (Chung.Dung(tin, "trung") ? $"trúng {Chung.Chuoi(tin, "ten_qua")}" : "không trúng")
                                          : $"từ chối – {Chung.ChuoiPy(tin["ly_do"])}";
        NhatKy.Ghi($"  máy chủ trả lời sau {Chung.F(rtt, "F1")} ms: {mo}");
        choKq = tin;
        if (Pha == null) KetThucQuay();                  // (không xảy ra khi bấm nút; phòng khi gọi trực tiếp)
    }

    void BatDauDung()
    {
        var tin = choKq!;
        var o = AnhQua.BoTriO((int)Chung.SoL(ttVe, "so_qua", 10));
        int j;
        if (Chung.Dung(tin, "ok") && Chung.Dung(tin, "trung") && Chung.SoLN(tin["qua"]) is { } q && Array.IndexOf(o, (int?)q) >= 0)
            j = Array.IndexOf(o, (int?)q);
        else
        {
            var trong = Enumerable.Range(0, o.Length).Where(k => o[k] == null).ToList();
            j = trong[Random.Shared.Next(trong.Count)];
        }
        double ext = 360.0 / o.Length;
        double dich = 90 - (j + 0.5) * ext + (Random.Shared.NextDouble() * 0.6 - 0.3) * ext;
        double m = Math.Ceiling((dich - (goc - 540)) / 360);
        (goc0, gocDich, tDung, Pha) = (goc, dich - 360 * m, Chung.Now, "dung");
    }

    void KhungHinh()
    {
        if (Pha == null)
        {
            hoatHinh.Stop();
            return;
        }
        nhip++;
        if (Pha == "tu_do")
        {
            goc -= 20;                                   // quay theo chiều kim đồng hồ
            if (choKq != null && Chung.Now - tBam > 0.6) BatDauDung();
        }
        else
        {
            double t = Math.Min(1.0, (Chung.Now - tDung) / 2.6);
            goc = goc0 + (gocDich - goc0) * (1 - Math.Pow(1 - t, 3));   // giảm tốc (ease-out)
            if (t >= 1.0)
            {
                Pha = null;
                hoatHinh.Stop();
                banhXe.Invalidate();
                KetThucQuay();
                return;
            }
        }
        banhXe.Invalidate();
    }

    void KetThucQuay()
    {
        var tin = choKq!;
        bool ok = Chung.Dung(tin, "ok"), daQuay = Chung.Chuoi(tin, "ma_loi") == "da_quay";
        if (ok && Chung.Dung(tin, "trung")) ketQua = ("CHÚC MỪNG! Bạn trúng " + Chung.Chuoi(tin, "ten_qua"), GD.Hex("#ffd700"), Color.Black);
        else if (ok) ketQua = ("Chúc bạn may mắn lần sau", GD.Hex("#d0d0ff"), Color.Black);
        else if (daQuay) ketQua = ("Bạn đã quay rồi – mỗi người 1 lượt", GD.Hex("#ffb347"), Color.Black);
        else ketQua = ("Lỗi: " + (Chung.Chuoi(tin, "ly_do") ?? ""), GD.Hex("#ff6060"), Color.White);
        var (chu, nen, mau) = ketQua.Value;
        Bao(chu, nen, mau);
        NhatKy.Ghi("Kết quả: " + chu);
        (BtnQuay.Text, BtnQuay.Enabled) = ok || daQuay ? ("ĐÃ QUAY", false) : ("QUAY", true);
        if (Tt != null)
        {
            ttVe = Tt;
            banhXe.Invalidate();
        }
    }

    void Bao(string text, Color nen, Color chu) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu);

    public string ThongBao => LblThongBao.Text;

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        if (Pha == null)
        {
            bool moiDatLai = ttVe != null && Chung.SoL(tt, "da_quay") < Chung.SoL(ttVe, "da_quay");
            ttVe = tt;
            banhXe.Invalidate();
            if (moiDatLai || ketQua == null)             // đặt lại chương trình: được quay lại
            {
                ketQua = null;
                (BtnQuay.Text, BtnQuay.Enabled) = ("QUAY", true);
                Bao("Mời bạn bấm QUAY", GD.Hex("#e0e0e0"), Color.Black);
            }
        }
        long conLai = Chung.SoL(tt, "con_lai"), soQua = Chung.SoL(tt, "so_qua");
        lblLuat.Text = $"Mỗi người 1 lượt · xác suất trúng {Qua.ChuP(Chung.SoD(tt, "p"))} mỗi lượt";
        (lblCon.Text, lblCon.ForeColor) = ($"Còn {conLai}/{soQua} quà", conLai > 0 ? GD.DoDam : GD.Hex("#606060"));
        lblDaQuay.Text = $"Đã quay: {Chung.SoL(tt, "da_quay").ToString("#,0", Chung.Ic).Replace(",", ".")} lượt · {Chung.SoL(tt, "so_trung")} lượt trúng";
        var ds = tt["qua"] as JsonArray ?? [];
        string khoa = string.Join("|", ds.OfType<JsonObject>().Select(q => $"{Chung.SoL(q, "ma")}:{Chung.Dung(q, "con")}"));
        if (khoa != daVeQua)
        {
            (daVeQua, qua) = (khoa, ds);
            for (int i = 0; i < oQua.Count; i++)
            {
                oQua[i].O.Visible = i < soQua;           // chương trình ít quà hơn: ẩn ô thừa
                oQua[i].O.Invalidate();
            }
        }
        for (int i = 0; i < oQua.Count; i++)
        {
            var nh = oQua[i].Nhan;
            if (i < ds.Count && ds[i] is JsonObject q)
            {
                bool con = Chung.Dung(q, "con");
                nh.Text = Chung.Chuoi(q, "ten") + (con ? "\ncòn" : $"\n→ {Chung.Chuoi(q, "nguoi") ?? "?"}");
                nh.ForeColor = con ? GD.XanhDam : GD.Hex("#a00000");
            }
            else nh.Text = "";
        }
        var trung = (tt["nguoi_trung"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        var dong = trung.Select(x => $"{Chung.Chuoi(x, "gio")} {Chung.Chuoi(x, "nguoi"),-6} {Qua.TenQua[(int)Chung.SoL(x, "qua")]}").ToList();
        if (!Ls.Items.Cast<string>().SequenceEqual(dong))
        {
            dongCuaToi.Clear();
            for (int i = 0; i < trung.Count; i++)
                if (Chung.Chuoi(trung[i], "nguoi") == Ten) dongCuaToi.Add(i);
            Ls.BeginUpdate();
            Ls.Items.Clear();
            foreach (var d in dong) Ls.Items.Add(d);
            Ls.EndUpdate();
        }
    }

    protected override string DongDinhKy()
    {
        var tt = Tt!;
        string kq = ketQua?.Chu ?? "chưa quay";
        return $"còn {Chung.SoL(tt, "con_lai")}/{Chung.SoL(tt, "so_qua")} quà, đã quay {Chung.SoL(tt, "da_quay")} lượt; bạn: {kq}";
    }
}
