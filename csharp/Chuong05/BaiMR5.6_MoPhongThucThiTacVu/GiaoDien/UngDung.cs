using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR5_6;

// Cửa sổ WinForms của MR5.6 (tương đương ung_dung.py của bản Python).
// Chọn chế độ (đơn luồng/đa luồng × đồng bộ/bất đồng bộ) hoặc "So sánh 4 chế độ"; biểu đồ Gantt chạy theo
// thời gian: mỗi hàng là một luồng (luồng hệ điều hành, vòng lặp sự kiện hay luồng thợ), màu là tác vụ đang
// dùng CPU; ô nhạt có gạch chéo: luồng bị chặn chờ I/O; ô viền đứt: sẵn sàng nhưng chờ lõi CPU; ô chấm: chờ
// join. Hàng "I/O chờ" (bất đồng bộ) vẽ các lần chờ I/O không giữ luồng nào. Nút "Chen ngang U ngay" đưa việc
// khẩn U vào đúng thời điểm đang xem và mô phỏng lại (lịch sử trước đó không đổi vì bộ lập lịch tất định).
//
// Toàn bộ chạy trên luồng giao diện: mô phỏng rất nhanh (vài mili giây), hoạt hình dùng
// System.Windows.Forms.Timer 30 ms (thông điệp WM_TIMER đi qua chính vòng lặp thông điệp) – không có luồng
// nền nào nên không cần Invoke/BeginInvoke hay khóa.

/// <summary>Vùng vẽ Gantt: bật bộ đệm đôi để hoạt hình không nhấp nháy.</summary>
class VungVe : Panel
{
    public VungVe()
    {
        DoubleBuffered = true;
        ResizeRedraw = true;
        BackColor = Color.White;
        BorderStyle = BorderStyle.FixedSingle;
    }
}

class UngDung : Form
{
    static Color Hex(string s) => ColorTranslator.FromHtml(s);
    static readonly Dictionary<string, Color> Mau = new()
    {
        ["T1"] = Hex("#2457c5"), ["T2"] = Hex("#e07b00"), ["T3"] = Hex("#1b9e5a"), ["U"] = Hex("#d1344b"),
    };
    static readonly Dictionary<string, Color> Nhat = new()
    {
        ["T1"] = Hex("#c9d8f6"), ["T2"] = Hex("#fbdcb8"), ["T3"] = Hex("#c3ead3"), ["U"] = Hex("#f6c5cc"),
    };
    static readonly Color Luoi = Hex("#e4e7ec"), Luoi10 = Hex("#c9ced8"), Chu = Hex("#1d2330"),
                          ChuMo = Hex("#667085"), NenLan = Hex("#f6f7f9"), NenIo = Hex("#f6fbf3"),
                          CotIo = Hex("#7fb069"), NenCho = Hex("#e9ecef"), VienCho = Hex("#adb5bd"),
                          VachNay = Hex("#f2b705");

    public List<(string M, KetQua Kq)> KetQua = [];      // các chế độ đang hiển thị
    public double T;                                     // thời điểm đang xem (nhịp, số thực để hoạt hình mượt)
    public bool DangChay;
    public int SoLanVe;                                  // số lần vẽ lại vùng Gantt (kịch bản demo dùng)
    double? last;
    bool dangDatThanh;                                   // đang tự đặt thanh kéo (không coi là người dùng kéo)
    readonly Stopwatch sw = Stopwatch.StartNew();
    string nkCu = "";

    // ---- điều khiển (kịch bản demo dùng trực tiếp) ----
    public readonly Dictionary<string, Button> Nut = [];
    public readonly Button BtnSoSanh = new() { Text = "So sánh 4 chế độ", Dock = DockStyle.Fill };
    public readonly Button BtnXoa = new() { Text = "Xóa", Dock = DockStyle.Fill };
    public readonly Button BtnChen = new() { Text = "⚡ Chen ngang U ngay", Dock = DockStyle.Fill };
    public readonly Button BtnChay = new() { Text = "⏸ Dừng", Dock = DockStyle.Fill };
    public readonly TextBox TxtT1 = new() { Text = "4", Width = 50 };
    public readonly TextBox TxtT2 = new() { Text = "3", Width = 50 };
    public readonly TextBox TxtT3 = new() { Text = "2", Width = 50 };
    public readonly NumericUpDown SpinLoi = new() { Minimum = 1, Maximum = 4, Value = 2, Width = 50 };
    public readonly TextBox TxtLat = new() { Text = "0.5", Width = 50 };
    public readonly ComboBox CboTocDo = new() { DropDownStyle = ComboBoxStyle.DropDownList, Width = 70 };
    public readonly CheckBox ChkPhuThuoc = new() { Text = "T3 chờ T1, T2 (join/await)", Checked = true, AutoSize = true };
    public readonly CheckBox ChkChia = new() { Text = "Chia CPU cuối T1 trên 2 luồng", Checked = true, AutoSize = true };
    public readonly CheckBox ChkKhan = new() { Text = "Việc khẩn U lúc (s)", Checked = true, AutoSize = true };
    public readonly TextBox TxtTKhan = new() { Text = "0.3", Width = 50 };
    readonly Label lblGio = new() { AutoSize = true, Font = new Font("Segoe UI", 12, FontStyle.Bold), Anchor = AnchorStyles.Right };
    readonly VungVe cv = new() { Dock = DockStyle.Fill };
    public readonly TrackBar Thanh = new() { Minimum = 0, Maximum = 1000, TickStyle = TickStyle.None, Dock = DockStyle.Bottom };
    readonly ListView bang = new() { View = View.Details, FullRowSelect = true, GridLines = true, Dock = DockStyle.Bottom, Height = 110, HeaderStyle = ColumnHeaderStyle.Nonclickable };
    readonly TextBox nk = new() { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical, WordWrap = true, Dock = DockStyle.Fill, Font = new Font("Consolas", 9), BackColor = Color.White };
    readonly System.Windows.Forms.Timer timer = new() { Interval = 30 };

    public UngDung()
    {
        Text = "Mô phỏng thực thi tác vụ – MR5.6";
        ClientSize = new Size(1280, 860);
        StartPosition = FormStartPosition.CenterScreen;
        Font = new Font("Segoe UI", 9);
        TaoGiaoDien();
        timer.Tick += (_, _) => NhipDongHo();
        timer.Start();
        Ve();
    }

    // ------------------------------------------------------------------ giao diện
    void TaoGiaoDien()
    {
        // Hàng trên: "Điều khiển" | "Thông số" | đồng hồ t = … / … s
        var dk = new GroupBox { Text = "Điều khiển", AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink, Padding = new Padding(6) };
        var luoiDk = new TableLayoutPanel { ColumnCount = 4, RowCount = 2, AutoSize = true, Dock = DockStyle.Fill };
        (string Ten, string M)[] nut = [("Đơn luồng – đồng bộ", "don_dong_bo"), ("Đa luồng – đồng bộ", "da_dong_bo"),
                                        ("Đơn luồng – bất đồng bộ", "don_bat_dong_bo"), ("Đa luồng – bất đồng bộ", "da_bat_dong_bo")];
        for (int i = 0; i < nut.Length; i++)
        {
            string m = nut[i].M;
            var b = new Button { Text = nut[i].Ten, Width = 170, Height = 30 };
            b.Click += (_, _) => Chon([m]);
            luoiDk.Controls.Add(b, i % 2, i / 2);
            Nut[m] = b;
        }
        BtnSoSanh.Click += (_, _) => Chon(LapLich.DsCheDo);
        BtnXoa.Click += (_, _) => Xoa();
        BtnChen.Click += (_, _) => ChenNgang();
        BtnChay.Click += (_, _) => ChayDung();
        foreach (var b in new[] { BtnSoSanh, BtnXoa, BtnChen, BtnChay }) { b.MinimumSize = new Size(150, 30); }
        luoiDk.Controls.Add(BtnSoSanh, 2, 0);
        luoiDk.Controls.Add(BtnXoa, 2, 1);
        luoiDk.Controls.Add(BtnChen, 3, 0);
        luoiDk.Controls.Add(BtnChay, 3, 1);
        dk.Controls.Add(luoiDk);

        var ts = new GroupBox { Text = "Thông số", AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink, Padding = new Padding(6) };
        var luoiTs = new TableLayoutPanel { ColumnCount = 6, RowCount = 3, AutoSize = true, Dock = DockStyle.Fill };
        Label Nhan(string s) => new() { Text = s, AutoSize = true, Anchor = AnchorStyles.Left, Margin = new Padding(8, 6, 3, 3) };
        luoiTs.Controls.Add(Nhan("T1 (s)"), 0, 0); luoiTs.Controls.Add(TxtT1, 1, 0);
        luoiTs.Controls.Add(Nhan("T2 (s)"), 0, 1); luoiTs.Controls.Add(TxtT2, 1, 1);
        luoiTs.Controls.Add(Nhan("T3 (s)"), 0, 2); luoiTs.Controls.Add(TxtT3, 1, 2);
        luoiTs.Controls.Add(Nhan("Số lõi CPU"), 2, 0); luoiTs.Controls.Add(SpinLoi, 3, 0);
        luoiTs.Controls.Add(Nhan("Lát thời gian (s)"), 2, 1); luoiTs.Controls.Add(TxtLat, 3, 1);
        luoiTs.Controls.Add(Nhan("Tốc độ"), 2, 2); luoiTs.Controls.Add(CboTocDo, 3, 2);
        CboTocDo.Items.AddRange(["× 0.25", "× 0.5", "× 1", "× 2", "× 4"]);
        CboTocDo.SelectedIndex = 2;
        foreach (var c in new[] { ChkPhuThuoc, ChkChia, ChkKhan }) c.Margin = new Padding(14, 4, 3, 3);
        luoiTs.Controls.Add(ChkPhuThuoc, 4, 0);
        luoiTs.Controls.Add(ChkChia, 4, 1);
        luoiTs.Controls.Add(ChkKhan, 4, 2);
        luoiTs.Controls.Add(TxtTKhan, 5, 2);
        ts.Controls.Add(luoiTs);

        var tren = new TableLayoutPanel { Dock = DockStyle.Top, AutoSize = true, ColumnCount = 3, RowCount = 1, Padding = new Padding(6) };
        tren.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        tren.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        tren.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        tren.Controls.Add(dk, 0, 0);
        tren.Controls.Add(ts, 1, 0);
        tren.Controls.Add(lblGio, 2, 0);

        // Giữa: Gantt + thanh kéo + bảng chỉ số (trái), nhật ký sự kiện (phải)
        var trai = new Panel { Dock = DockStyle.Fill };
        cv.Paint += (_, e) => VeGantt(e.Graphics);
        Thanh.ValueChanged += (_, _) => Keo(Thanh.Value);
        foreach (var c in new[] { "Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát" })
            bang.Columns.Add(c, c == "Chế độ" ? 150 : 80, c == "Chế độ" ? HorizontalAlignment.Left : HorizontalAlignment.Center);
        trai.Controls.Add(cv);                     // thêm vùng Fill trước, các vùng Bottom sau (thứ tự xếp của Dock)
        trai.Controls.Add(Thanh);
        trai.Controls.Add(bang);
        var phai = new GroupBox { Text = "Nhật ký sự kiện", Dock = DockStyle.Right, Width = 390, Padding = new Padding(4) };
        phai.Controls.Add(nk);
        var giua = new Panel { Dock = DockStyle.Fill, Padding = new Padding(6, 0, 6, 6) };
        giua.Controls.Add(trai);
        giua.Controls.Add(phai);

        Controls.Add(giua);
        Controls.Add(tren);
    }

    // ------------------------------------------------------------------ thao tác
    static double So(string s, double mac) =>
        double.TryParse(s.Trim().Replace(",", "."), NumberStyles.Float, CultureInfo.InvariantCulture, out double x) ? x : mac;

    public ThamSo DocThamSo() =>
        new(Math.Max(0.4, So(TxtT1.Text, 4)), Math.Max(0.4, So(TxtT2.Text, 3)), Math.Max(0.4, So(TxtT3.Text, 2)),
            (int)SpinLoi.Value, Math.Max(0.1, So(TxtLat.Text, 0.5)),
            ChkPhuThuoc.Checked, ChkChia.Checked, ChkKhan.Checked, Math.Max(0.0, So(TxtTKhan.Text, 0.3)));

    public void Chon(IEnumerable<string> dsCheDo)
    {
        var ts = DocThamSo();
        KetQua = dsCheDo.Select(m => (m, LapLich.Chay(m, ts))).ToList();
        T = 0; DangChay = true; last = null;
        BtnChay.Text = "⏸ Dừng";
        CapNhatBang();
        Ve();
    }

    void Xoa()
    {
        KetQua = []; T = 0; DangChay = false;
        bang.Items.Clear();
        Ve();
    }

    void ChayDung()
    {
        DangChay = !DangChay;
        last = null;
        BtnChay.Text = DangChay ? "⏸ Dừng" : "▶ Chạy";
    }

    /// <summary>Đặt việc khẩn U đến ngay thời điểm đang xem rồi mô phỏng lại; giữ nguyên thời điểm xem.</summary>
    public void ChenNgang()
    {
        if (KetQua.Count == 0) return;
        ChkKhan.Checked = true;
        TxtTKhan.Text = LapLich.Giay((int)T);
        double t = T;
        var ds = KetQua.Select(k => k.M).ToList();
        var ts = DocThamSo();
        KetQua = ds.Select(m => (m, LapLich.Chay(m, ts))).ToList();
        T = t;
        CapNhatBang();
        Ve();
    }

    void Keo(int giaTri)
    {
        int D = DoDai();
        if (D > 0 && !DangChay && !dangDatThanh)
        {
            T = giaTri / 1000.0 * D;
            Ve();
        }
    }

    public int DoDai() => KetQua.Count == 0 ? 0 : KetQua.Max(k => k.Kq.Tong);

    void CapNhatBang()
    {
        bang.Items.Clear();
        foreach (var (m, kq) in KetQua)
        {
            var s = LapLich.TinhChiSo(kq);
            string u = s.UCho >= 0 ? LapLich.Giay(s.UCho) + " s" : "–";
            bang.Items.Add(new ListViewItem(new[] { LapLich.TenNgan[m], LapLich.Giay(s.Tong) + " s", s.Luong.ToString(),
                                                    $"{s.Ban} %", u, s.Chen.ToString(), s.Lat.ToString() }));
        }
    }

    void NhipDongHo()
    {
        double now = sw.Elapsed.TotalSeconds;
        if (DangChay && KetQua.Count > 0)
        {
            if (last != null)
            {
                double toc = double.Parse(((string)CboTocDo.SelectedItem!).Split(' ')[1], CultureInfo.InvariantCulture);
                T += (now - last.Value) * 10 * toc;          // × 1: 1 giây mô phỏng mỗi giây thật
                if (T >= DoDai())
                {
                    T = DoDai();
                    DangChay = false;
                    BtnChay.Text = "▶ Chạy";
                }
            }
            last = now;
            Ve();
        }
    }

    // ------------------------------------------------------------------ vẽ
    public void Ve()
    {
        int D = DoDai();
        int t = Math.Min((int)T, D);
        lblGio.Text = D > 0 ? $"t = {LapLich.Giay(t)} / {LapLich.Giay(D)} s" : "Chọn một chế độ";
        if (D > 0)
        {
            dangDatThanh = true;
            Thanh.Value = Math.Clamp((int)(1000 * T / D), 0, 1000);
            dangDatThanh = false;
        }
        cv.Invalidate();
        VeNhatKy(t);
    }

    void VeGantt(Graphics g)
    {
        SoLanVe++;
        g.SmoothingMode = SmoothingMode.None;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
        int W = Math.Max(200, cv.ClientSize.Width), H = Math.Max(200, cv.ClientSize.Height);
        int D = DoDai();
        int t = Math.Min((int)T, D);
        g.Clear(Color.White);
        using var fChu = new Font("Segoe UI", 9);
        if (KetQua.Count == 0)
        {
            using var f14 = new Font("Segoe UI", 14);
            VeChu(g, "Chọn một chế độ để bắt đầu mô phỏng", f14, ChuMo, W / 2f, H / 2f, ContentAlignment.MiddleCenter);
            return;
        }
        float trai = 110, phai = W - 20;
        float X(double n) => (float)(trai + (phai - trai) * n / Math.Max(1, D));
        double soHang = KetQua.Sum(k => k.Kq.Lan.Count + (k.Kq.Io.Count > 0 ? 1 : 0) + 1.2);
        float h = (float)Math.Min(48, (H - 50) / soHang);
        // vị trí đáy của các làn (để vẽ lưới phía sau trước)
        float yCuoi = 8;
        foreach (var (_, kq) in KetQua)
            yCuoi += h * 0.9f + h * kq.Lan.Count + (kq.Io.Count > 0 ? h : 0) + h * 0.3f;
        // trục thời gian (lưới nằm dưới các khối)
        using (var pLuoi = new Pen(Luoi)) using (var pLuoi10 = new Pen(Luoi10))
            for (int gg = 0; gg <= D; gg += 5)
            {
                float x = X(gg);
                g.DrawLine(gg % 10 != 0 ? pLuoi : pLuoi10, x, 4, x, yCuoi);
                if (gg % 10 == 0)
                    VeChu(g, $"{gg / 10} s", fChu, ChuMo, x, yCuoi + 10, ContentAlignment.MiddleCenter);
            }
        float y = 8;
        using var fDam = new Font("Segoe UI", 10, FontStyle.Bold);
        foreach (var (m, kq) in KetQua)
        {
            VeChu(g, LapLich.CheDo[m], fDam, Chu, 8, y + h * 0.4f, ContentAlignment.MiddleLeft);
            y += h * 0.9f;
            foreach (var (ten, o) in kq.Lan)
            {
                VeLan(g, ten, o, y, h, X, t, fChu);
                y += h;
            }
            if (kq.Io.Count > 0)
            {
                VeIo(g, kq, y, h, X, t, fChu);
                y += h;
            }
            y += h * 0.3f;
        }
        // đường "bây giờ"
        using (var pNay = new Pen(VachNay, 2))
            g.DrawLine(pNay, X(T), 2, X(T), y);
        VeChuGiai(g, y + 26, W, fChu);
    }

    static void VeChu(Graphics g, string s, Font f, Color c, float x, float y, ContentAlignment canh)
    {
        var kt = g.MeasureString(s, f);
        float x0 = canh switch
        {
            ContentAlignment.MiddleLeft => x,
            ContentAlignment.MiddleRight => x - kt.Width,
            _ => x - kt.Width / 2,
        };
        using var b = new SolidBrush(c);
        g.DrawString(s, f, b, x0, y - kt.Height / 2);
    }

    static RectangleF HinhChuNhat(float x0, float y0, float x1, float y1) => new(x0, y0, Math.Max(0, x1 - x0), Math.Max(0, y1 - y0));

    void VeLan(Graphics g, string ten, List<O> o, float y, float h, Func<double, float> X, int t, Font fChu)
    {
        VeChu(g, ten, fChu, Chu, 104, y + h / 2, ContentAlignment.MiddleRight);
        using (var nen = new SolidBrush(NenLan))
            g.FillRectangle(nen, HinhChuNhat(X(0), y + 2, X(o.Count), y + h - 2));
        using var f9 = new Font("Segoe UI", 9, FontStyle.Bold);
        using var f8 = new Font("Segoe UI", 8);
        int j = 0;
        while (j < Math.Min(o.Count, t + 1))                    // gộp các nhịp liên tiếp cùng trạng thái thành một khối
        {
            int k = j;
            while (k + 1 < o.Count && k + 1 <= t && o[k + 1] == o[j])
                k++;
            var (tt, nhan) = (o[j].Tt, o[j].Nhan);
            float x0 = X(j), x1 = k >= t ? X(Math.Min(k + 1, T)) : X(k + 1);
            if (tt == LapLich.CPU)
            {
                using var b = new SolidBrush(Mau[nhan]);
                g.FillRectangle(b, HinhChuNhat(x0, y + 3, x1, y + h - 3));
                if (x1 - x0 > 18)
                    VeChu(g, nhan, f9, Color.White, (x0 + x1) / 2, y + h / 2, ContentAlignment.MiddleCenter);
            }
            else if (tt == LapLich.IO)
            {
                var r = HinhChuNhat(x0, y + 6, x1, y + h - 6);
                using var b = new HatchBrush(HatchStyle.WideUpwardDiagonal, Color.FromArgb(110, Mau[nhan]), Nhat[nhan]);
                g.FillRectangle(b, r);
                using var p = new Pen(Mau[nhan]);
                g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
                if (x1 - x0 > 40)
                    VeChu(g, $"{nhan} chờ I/O", f8, Chu, (x0 + x1) / 2, y + h / 2, ContentAlignment.MiddleCenter);
            }
            else if (tt == LapLich.SAN_SANG)
            {
                var r = HinhChuNhat(x0, y + 5, x1, y + h - 5);
                using var p = new Pen(Mau[nhan]) { DashPattern = [3, 2] };
                g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
            }
            else if (tt == LapLich.CHO)
            {
                var r = HinhChuNhat(x0, y + 8, x1, y + h - 8);
                using var b = new SolidBrush(NenCho);
                g.FillRectangle(b, r);
                using var p = new Pen(VienCho) { DashPattern = [1, 2] };
                g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
                if (x1 - x0 > 50)
                    VeChu(g, "chờ join", f8, ChuMo, (x0 + x1) / 2, y + h / 2, ContentAlignment.MiddleCenter);
            }
            j = k + 1;
        }
    }

    static void VeIo(Graphics g, KetQua kq, float y, float h, Func<double, float> X, int t, Font fChu)
    {
        VeChu(g, "I/O chờ", fChu, Chu, 104, y + h / 2, ContentAlignment.MiddleRight);
        using (var nen = new SolidBrush(NenIo))
            g.FillRectangle(nen, HinhChuNhat(X(0), y + 2, X(kq.Tong), y + h - 2));
        int nMax = kq.Io.Count > 0 ? kq.Io.Max() : 1;
        using var b = new SolidBrush(CotIo);
        for (int j = 0; j < Math.Min(kq.Io.Count, t + 1); j++)
        {
            int n = kq.Io[j];
            if (n != 0)
            {
                float hh = (h - 6) * n / Math.Max(1, nMax);
                g.FillRectangle(b, HinhChuNhat(X(j), y + h - 3 - hh, X(j + 1), y + h - 3));
            }
        }
    }

    /// <summary>Chú giải: mỗi mục một ô mẫu + chữ; vị trí tính theo độ rộng chữ thật để không chồng nhau.</summary>
    void VeChuGiai(Graphics g, float y, int W, Font f)
    {
        var muc = new List<(Action<RectangleF> Mau, string Chu)>();
        foreach (var tv in new[] { "T1", "T2", "T3", "U" })
            muc.Add((r => { using var b = new SolidBrush(Mau[tv]); g.FillRectangle(b, r); }, $"{tv} dùng CPU"));
        muc.Add((r =>
        {
            using var b = new HatchBrush(HatchStyle.WideUpwardDiagonal, Color.FromArgb(110, Mau["T1"]), Nhat["T1"]);
            g.FillRectangle(b, r);
            using var p = new Pen(Mau["T1"]);
            g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
        }, "bị chặn chờ I/O"));
        muc.Add((r => { using var p = new Pen(Mau["T1"]) { DashPattern = [3, 2] }; g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height); },
                 "sẵn sàng, chờ lõi"));
        muc.Add((r =>
        {
            using var b = new SolidBrush(NenCho);
            g.FillRectangle(b, r);
            using var p = new Pen(VienCho) { DashPattern = [1, 2] };
            g.DrawRectangle(p, r.X, r.Y, r.Width, r.Height);
        }, "chờ join"));
        muc.Add((r => { using var b = new SolidBrush(CotIo); g.FillRectangle(b, r); }, "I/O bất đồng bộ (không giữ luồng)"));
        float x = 20;
        foreach (var (veMau, chu) in muc)
        {
            float rong = g.MeasureString(chu, f).Width;
            if (x + 23 + rong > W - 10)                    // hết chỗ: xuống hàng
            {
                x = 20; y += 20;
            }
            veMau(new RectangleF(x, y - 6, 18, 12));
            VeChu(g, chu, f, Chu, x + 23, y, ContentAlignment.MiddleLeft);
            x += 23 + rong + 16;
        }
    }

    void VeNhatKy(int t)
    {
        var sb = new System.Text.StringBuilder();
        foreach (var (m, kq) in KetQua)
        {
            if (KetQua.Count > 1)
                sb.Append($"— {LapLich.TenNgan[m]} —\r\n");
            foreach (var (tn, text) in kq.NhatKy)
                if (tn <= t)
                    sb.Append($"{LapLich.Phai(LapLich.Giay(tn), 4)} s  {text}\r\n");
        }
        string moi = sb.ToString();
        if (moi == nkCu) return;                           // chỉ cập nhật khi có dòng mới: không nhấp nháy
        nkCu = moi;
        nk.Text = moi;
        nk.SelectionStart = nk.TextLength;
        nk.ScrollToCaret();
    }
}
