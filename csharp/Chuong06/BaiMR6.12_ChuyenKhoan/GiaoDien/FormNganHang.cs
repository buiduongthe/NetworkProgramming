using System.Drawing;
using System.Drawing.Drawing2D;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Ngân hàng số (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// ứng dụng ngân hàng: thẻ tài khoản (GDI+: nền chuyển màu, chip, số tài khoản, chủ tài khoản, số dư cỡ lớn), chọn tài
// khoản của tôi, lịch sử giao dịch (tiền ra đỏ "−", tiền vào xanh "+"), khung chuyển tiền (người nhận, số tiền, nút
// nhanh 100K / 500K / 1 triệu / 2 triệu, nội dung), thông báo kết quả (kể cả "khóa chết: … đã hủy giao dịch"), dòng
// "+ Nhận … từ …" khi có tiền đến (từ cap_nhat), cảnh báo khi tổng tiền hệ thống lệch.
sealed class FormNganHang(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    const string TenNganHang = "NGÂN HÀNG SỐ LTM";
    static readonly (long Tien, string Chu)[] Nhanh = [(100_000, "100K"), (500_000, "500K"), (1_000_000, "1 triệu"), (2_000_000, "2 triệu")];
    protected override string TieuDe => "Ngân hàng số";
    protected override Size KichThuocKhung => new(724, 420);

    /// <summary>Vẽ thẻ tài khoản: nền chuyển màu, góc bo, chip, số tài khoản, chủ tài khoản, số dư.</summary>
    static void VeThe(Graphics g, int w, int h, string ma, string chu, long soDu)
    {
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.Clear(GD.Nen);
        const int r = 16;
        using var vien = new GraphicsPath();
        vien.AddArc(0, 0, 2 * r, 2 * r, 180, 90);
        vien.AddArc(w - 2 * r - 1, 0, 2 * r, 2 * r, 270, 90);
        vien.AddArc(w - 2 * r - 1, h - 2 * r - 1, 2 * r, 2 * r, 0, 90);
        vien.AddArc(0, h - 2 * r - 1, 2 * r, 2 * r, 90, 90);
        vien.CloseFigure();
        using (var nen = new LinearGradientBrush(new Rectangle(0, 0, w, h), GD.Hex("#0b3d91"), GD.Hex("#1e88e5"), LinearGradientMode.Vertical))
            g.FillPath(nen, vien);
        var cu = g.Clip;
        g.SetClip(vien);
        using (var p = new Pen(GD.Hex("#5c9ce6"), 2)) g.DrawEllipse(p, w - 150, -60, 210, 210);   // họa tiết
        using (var p = new Pen(GD.Hex("#4a8ad8"), 1)) g.DrawEllipse(p, w - 110, -30, 200, 200);
        g.Clip = cu;
        using var trai = new StringFormat { LineAlignment = StringAlignment.Center };
        using var phai = new StringFormat { LineAlignment = StringAlignment.Center, Alignment = StringAlignment.Far };
        using var nhat = new SolidBrush(GD.Hex("#cfe3ff"));
        using (var f = GD.Chu(11, true)) g.DrawString(TenNganHang, f, Brushes.White, 18, 20, trai);
        using (var f = GD.Chu(8, true)) g.DrawString("THANH TOÁN", f, nhat, w - 18, 20, phai);
        using (var b = new SolidBrush(GD.Hex("#e8c766"))) g.FillRectangle(b, 20, 44, 42, 30);   // chip
        using (var p = new Pen(GD.Hex("#a8862e")))
        {
            g.DrawRectangle(p, 20, 44, 42, 30);
            foreach (int y in new[] { 54, 64 }) g.DrawLine(p, 20, y, 62, y);
            g.DrawLine(p, 41, 44, 41, 74);
        }
        int[] banKinh = [8, 14, 20];                     // biểu tượng không tiếp xúc
        for (int i = 0; i < banKinh.Length; i++)
        {
            int rr = banKinh[i];
            using var p = new Pen(Color.White, i == 0 ? 2 : 1);
            g.DrawArc(p, 70 - rr, 59 - rr, 2 * rr, 2 * rr, -40, 80);
        }
        using (var f = GD.Chu(8)) g.DrawString("Số tài khoản", f, nhat, 110, 50, trai);
        using (var f = GD.ChuCoDinh(12, true)) g.DrawString($"{ma}  ••••  6125", f, Brushes.White, 110, 68, trai);
        using (var f = GD.Chu(9)) g.DrawString("Số dư khả dụng", f, nhat, 20, 98, trai);
        using (var f = GD.Chu(22, true)) g.DrawString(Chung.Tien(soDu), f, Brushes.White, 20, 126, trai);
        using (var f = GD.Chu(9, true))
        {
            g.DrawString($"CHỦ TÀI KHOẢN: {chu.ToUpperInvariant()}", f, Brushes.White, 20, h - 22, trai);
            g.DrawString("VND", f, nhat, w - 18, h - 22, phai);
        }
    }

    string? tk;                                          // mã tài khoản của tôi
    HashSet<long> daThay = [];                           // ma_gd đã đưa vào lịch sử
    int soChuyen, soThanhCong, soNhan;
    public int SoKq;
    long soGdCu;
    (string Ma, string Chu, long SoDu)? theCu;
    OVe the = null!;
    ComboBox chonTk = null!, chonDen = null!;
    ListBox ls = null!;
    readonly List<Color> mauLs = [];
    TextBox oTien = null!, oNoiDung = null!;
    public Button BtnChuyen = null!;
    public Label LblThongBao = null!, LblDen = null!;
    Label lblDem = null!, lblHeThong = null!;

    protected override void Ve(Panel khung)
    {
        the = new OVe(0, 4, 340, 190) { Ve = g => VeThe(g, 340, 190, theCu?.Ma ?? "TK??", theCu?.Chu ?? "–", theCu?.SoDu ?? 0) };
        chonTk = new ComboBox { Location = new Point(112, 200), Width = 180, DropDownStyle = ComboBoxStyle.DropDownList };
        chonTk.SelectionChangeCommitted += (_, _) => DoiTk();
        var gLs = GD.Nhom("Lịch sử giao dịch (mới nhất trên cùng)", 0, 228, 340, 190);
        ls = new ListBox
        {
            Location = new Point(8, 18), Size = new Size(324, 166), Font = GD.ChuCoDinh(9), IntegralHeight = false, SelectionMode = SelectionMode.None,
            DrawMode = DrawMode.OwnerDrawFixed, HorizontalScrollbar = true,
        };
        ls.DrawItem += (_, e) =>                         // tiền ra đỏ, tiền vào xanh
        {
            if (e.Index < 0) return;
            e.DrawBackground();
            using var b = new SolidBrush(e.Index < mauLs.Count ? mauLs[e.Index] : Color.Black);
            e.Graphics.DrawString(ls.Items[e.Index]?.ToString(), ls.Font, b, e.Bounds.Location);
        };
        GD.Them(gLs, ls);

        var gCt = GD.Nhom("Chuyển tiền", 348, 4, 376, 200);
        chonDen = new ComboBox { Location = new Point(104, 20), Width = 200, DropDownStyle = ComboBoxStyle.DropDownList };
        oTien = new TextBox { Text = "500.000", Location = new Point(104, 50), Width = 140, Font = GD.Chu(11, true), TextAlign = HorizontalAlignment.Right };
        var nut = Nhanh.Select((x, i) =>
        {
            var b = GD.Nut(x.Chu, GD.Hex("#e8eef8"), 8 + i * 66, 84, 62, 26);
            b.Click += (_, _) => DatTien(x.Tien);
            return (Control)b;
        }).ToArray();
        oNoiDung = new TextBox { Text = "Chuyển tiền", Location = new Point(104, 118), Width = 200 };
        BtnChuyen = GD.Nut("Chuyển tiền", GD.Hex("#ff8c00"), 104, 152, 170, 34);
        BtnChuyen.Font = GD.Chu(10, true);
        BtnChuyen.Enabled = false;
        BtnChuyen.Click += (_, _) => BamChuyen();
        GD.Them(gCt, [GD.Nhan("Đến tài khoản:", 8, 23), chonDen, GD.Nhan("Số tiền (đ):", 8, 54), oTien, .. nut, GD.Nhan("Nội dung:", 8, 121), oNoiDung, BtnChuyen]);
        oTien.Font = GD.Chu(11, true);
        BtnChuyen.Font = GD.Chu(10, true);

        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(348, 210), Size = new Size(376, 62), Font = GD.Chu(10, true), BackColor = GD.Hex("#e0e0e0"),
            TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(6, 0, 0, 0),
        };
        LblDen = new Label { Text = "", Location = new Point(348, 276), Size = new Size(376, 36), Font = GD.Chu(9, true), ForeColor = GD.Hex("#0050a0"),
                             Padding = new Padding(6, 2, 0, 0) };
        lblDem = GD.Nhan("Đã chuyển 0 lần · thành công 0 · nhận 0", 348, 316, 376, GD.Chu(8), 18);
        lblHeThong = GD.Nhan("", 348, 338, 376, GD.Chu(8), 40);
        lblHeThong.ForeColor = GD.Hex("#404040");
        GD.Them(khung, the, GD.Nhan("Tài khoản của tôi:", 0, 203), chonTk, gLs, gCt, LblThongBao, LblDen, lblDem, lblHeThong);
    }

    // ------------------------------------------------------------ tiện ích
    IEnumerable<JsonObject> TaiKhoan() => (Tt?["tai_khoan"] as JsonArray ?? []).OfType<JsonObject>();

    string Chu(string? ma) => Chung.Chuoi(TaiKhoan().FirstOrDefault(t => Chung.Chuoi(t, "ma") == ma), "chu") ?? "?";

    long SoDu() => Chung.SoL(TaiKhoan().FirstOrDefault(t => Chung.Chuoi(t, "ma") == tk), "so_du");

    /// <summary>Điền ô số tiền như bấm nút nhanh: 500000 -> "500.000".</summary>
    public void DatTien(long x) => oTien.Text = Chung.Tien(x)[..^2];

    void Bao(string text, string nen, Color? chu = null) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, GD.Hex(nen), chu ?? Color.Black);

    public string ThongBao => LblThongBao.Text;

    /// <summary>Thêm một giao dịch vào đầu lịch sử: tiền ra (đỏ, dấu −) hoặc tiền vào (xanh, dấu +).</summary>
    void ThemLs(string gio, string tu, string den, long soTien, string? noiDung, bool ra, string ghiChu = "")
    {
        string doi = ra ? den : tu;
        string dong = $"{gio} {(ra ? "−" : "+")}{Chung.Tien(soTien),13} {(ra ? "tới" : "từ ")} {doi} {Chu(doi)}"
                      + (string.IsNullOrEmpty(noiDung) ? "" : $" · {noiDung}") + ghiChu;
        ls.Items.Insert(0, dong);
        mauLs.Insert(0, ra ? GD.DoDam : GD.XanhDam);
        while (ls.Items.Count > 100)
        {
            ls.Items.RemoveAt(100);
            mauLs.RemoveAt(100);
        }
    }

    void XoaLs()
    {
        ls.Items.Clear();
        mauLs.Clear();
    }

    // ------------------------------------------------------------ chọn tài khoản
    void DoiTk()
    {
        string ma = (chonTk.SelectedItem as string ?? "").Split(' ')[0];
        if (ma == tk || ma == "") return;
        tk = ma;
        XoaLs();
        daThay = [.. (Tt?["lich_su"] as JsonArray ?? []).OfType<JsonObject>().Select(g => Chung.SoL(g, "ma"))];
        DienDen();
        VeLaiThe();
        NhatKy.Ghi($"Đăng nhập {ma} – {Chu(ma)}, số dư {Chung.Tien(SoDu())}");
    }

    void DienDen()
    {
        var ds = TaiKhoan().Where(t => Chung.Chuoi(t, "ma") != tk).Select(t => $"{Chung.Chuoi(t, "ma")} – {Chung.Chuoi(t, "chu")}").ToList();
        string? chon = chonDen.SelectedItem as string;
        if (!chonDen.Items.Cast<string>().SequenceEqual(ds))
        {
            chonDen.Items.Clear();
            foreach (var d in ds) chonDen.Items.Add(d);
        }
        if (chon != null && ds.Contains(chon)) chonDen.SelectedItem = chon;
        else if (ds.Count > 0) chonDen.SelectedIndex = 0;
    }

    /// <summary>Chọn tài khoản đích (kịch bản demo) – như người dùng chọn trong danh sách.</summary>
    public void ChonNguoiNhan(string ma) => chonDen.SelectedItem = $"{ma} – {Chu(ma)}";

    void VeLaiThe()
    {
        var moi = (tk ?? "TK??", Chu(tk), SoDu());
        if (theCu == moi) return;                        // chỉ vẽ lại khi số dư/tài khoản đổi
        theCu = moi;
        the.Invalidate();
    }

    // ------------------------------------------------------------ chuyển tiền
    void BamChuyen()
    {
        string den = (chonDen.SelectedItem as string ?? "").Split(' ')[0];
        if (!long.TryParse(oTien.Text.Replace(".", "").Replace(" ", "").Replace("đ", ""), System.Globalization.NumberStyles.Integer, Chung.Ic, out long x))
        {
            Bao("Số tiền không hợp lệ", "#ffb347");
            return;
        }
        soChuyen++;
        NhatKy.Ghi($"Chuyển {Chung.Tien(x)} {tk}→{den}…");
        Bao($"Đang xử lý: {Chung.Tien(x)} tới {den}…", "#fff3c4");
        string nd = oNoiDung.Text.Trim();
        if (nd.Length > 40) nd = nd[..40];
        GuiYc("chuyen", new JsonObject { ["tu"] = tk, ["den"] = den, ["so_tien"] = x, ["noi_dung"] = nd });
    }

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        long x = Chung.SoL(truong, "so_tien");
        string den = Chung.Chuoi(truong, "den") ?? "";
        SoKq++;
        if (Chung.Dung(tin, "ok"))
        {
            soThanhCong++;
            daThay.Add(Chung.SoL(tin, "ma_gd"));
            if (Chung.Chuoi(truong, "tu") == tk)
                ThemLs(DateTime.Now.ToString("HH:mm:ss"), Chung.Chuoi(truong, "tu")!, den, x, Chung.Chuoi(truong, "noi_dung"), true);
            NhatKy.Ghi($"  thành công ({Chung.F(rtt, "F1")} ms), mã GD {Chung.SoL(tin, "ma_gd")}");
            Bao($"Chuyển thành công {Chung.Tien(x)} tới {den} – {Chu(den)}\nSố dư mới: {Chung.Tien(Chung.SoL(tin, "so_du"))}", "#70d070");
        }
        else
        {
            string lyDo = Chung.Chuoi(tin, "ly_do") ?? "";
            NhatKy.Ghi($"  thất bại ({Chung.F(rtt, "F0")} ms): {lyDo}");
            string? ma = Chung.Chuoi(tin, "ma_loi");
            if (ma == "khoa_cho_lau") Bao(lyDo.Length > 0 ? char.ToUpper(lyDo[0]) + lyDo[1..] : lyDo, "#ff6060", Color.White);
            else if (ma == "khong_du") Bao($"Không đủ số dư để chuyển {Chung.Tien(x)}", "#ffb347");
            else Bao($"Giao dịch thất bại: {lyDo}", "#ffb347");
        }
        lblDem.Text = $"Đã chuyển {soChuyen} lần · thành công {soThanhCong} · nhận {soNhan}";
    }

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        var ds = TaiKhoan().Select(t => $"{Chung.Chuoi(t, "ma")} – {Chung.Chuoi(t, "chu")}").ToList();
        if (!chonTk.Items.Cast<string>().SequenceEqual(ds))
        {
            chonTk.Items.Clear();
            foreach (var d in ds) chonTk.Items.Add(d);
        }
        long soGd = Chung.SoL(tt, "so_gd");
        if (soGd < soGdCu)                               // máy chủ vừa đặt lại: số dư về ban đầu
        {
            XoaLs();
            (LblDen.Text, LblDen.BackColor) = ("", GD.Nen);
            Bao("Máy chủ đặt lại: số dư về ban đầu", "#e0e0e0");
            NhatKy.Ghi("Máy chủ đặt lại: số dư về ban đầu");
        }
        soGdCu = soGd;
        var lichSu = (tt["lich_su"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        if (tk == null || !TaiKhoan().Any(t => Chung.Chuoi(t, "ma") == tk))
        {
            tk = Chung.Chuoi(TaiKhoan().FirstOrDefault(t => Chung.Chuoi(t, "chu") == Ten) ?? TaiKhoan().First(), "ma");
            daThay = [.. lichSu.Select(g => Chung.SoL(g, "ma"))];
            XoaLs();
            NhatKy.Ghi($"Đăng nhập {tk} – {Chu(tk)}, số dư {Chung.Tien(SoDu())}");
            Bao("Xin chào! Chọn người nhận và số tiền để chuyển.", "#e0e0e0");
        }
        chonTk.SelectedItem = $"{tk} – {Chu(tk)}";
        DienDen();
        VeLaiThe();
        BtnChuyen.Enabled = true;
        for (int i = lichSu.Count - 1; i >= 0; i--)      // giao dịch mới liên quan tới tài khoản của tôi
        {
            var g = lichSu[i];
            if (!daThay.Add(Chung.SoL(g, "ma"))) continue;
            string tu = Chung.Chuoi(g, "tu") ?? "", den = Chung.Chuoi(g, "den") ?? "", gio = Chung.Chuoi(g, "gio") ?? "";
            long x = Chung.SoL(g, "so_tien");
            if (den == tk)
            {
                soNhan++;
                ThemLs(gio, tu, den, x, Chung.Chuoi(g, "noi_dung"), false);
                NhatKy.Ghi($"+ Nhận {Chung.Tien(x)} từ {tu} ({Chu(tu)})");
                (LblDen.Text, LblDen.BackColor) = ($"+ Nhận {Chung.Tien(x)} từ {tu} – {Chu(tu)} lúc {gio}", GD.Hex("#bfe8ff"));
            }
            else if (tu == tk) ThemLs(gio, tu, den, x, Chung.Chuoi(g, "noi_dung"), true, $" ({Chung.Chuoi(g, "nguoi")})");   // tiền ra do phiên khác của cùng tài khoản
        }
        long lech = Chung.SoL(tt, "tong") - Chung.SoL(tt, "tong_dau");
        lblHeThong.Text = $"Máy chủ: {ds.Count} tài khoản, {soGd} giao dịch" + (Chung.Dung(tt, "khoa_ngay_tho") ? " – khóa ngây thơ" : "")
                          + (lech != 0 ? $"\nTổng tiền hệ thống LỆCH {Chung.Tien(lech)}!" : "");
        lblHeThong.ForeColor = lech != 0 ? GD.DoDam : GD.Hex("#404040");
        lblDem.Text = $"Đã chuyển {soChuyen} lần · thành công {soThanhCong} · nhận {soNhan}";
    }

    protected override string DongDinhKy() =>
        $"{tk} số dư {Chung.Tien(SoDu())}; chuyển {soChuyen}, thành công {soThanhCong}, nhận {soNhan}";
}
