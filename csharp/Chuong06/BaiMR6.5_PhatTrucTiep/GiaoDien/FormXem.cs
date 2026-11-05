using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Cửa sổ người xem phát trực tiếp. Lõi NguoiXem (../NguoiXem.cs) nhận, giải mã, phát âm thanh
// (đồng hồ chủ) và chọn khung để hiện; cửa sổ cứ 50 ms đọc khung đang hiện để vẽ (phóng ×2) và cứ 200 ms cập nhật
// thanh mức, thống kê. Điều khiển: Kết nối/Ngắt, công tắc Đồng bộ tiếng-hình, chọn nhận Âm thanh / Video / Cả hai,
// độ sâu bộ đệm jitter, Loa (winmm, nếu có thiết bị phát), Chụp (lưu out/&lt;tên&gt;_gui_chup_N.png).
sealed class FormXem : Form
{
    readonly HangDoiGiaoDien ui = new();
    public NguoiXem? Lop;
    readonly ILoa? loa;
    readonly System.Windows.Forms.Timer nhipHinh = new() { Interval = 50 }, nhip = new() { Interval = 200 };
    long soDaVe = -1;
    int soChup;
    string nhanTt = "chưa kết nối";
    public readonly TextBox TxtIp, TxtCong, TxtTen;
    public readonly Button BtnKetNoi, BtnChup;
    public readonly CheckBox ChkDongBo, ChkLoa;
    public readonly RadioButton RdoAm, RdoVideo, RdoCaHai;
    public readonly NumericUpDown NumDem;
    public readonly Label LblTrangThai;
    readonly Label lblKhung, lblLoa;
    readonly Dictionary<string, Label> lblTk = [];
    readonly OAnh hinh = new(2 * Chung.Rong, 2 * Chung.Cao);
    readonly ThanhMuc thanh;
    readonly NhatKy nhatKy;
    readonly ThongKeDinhKy dinhKy;

    public FormXem(string ten = "A", string host = "127.0.0.1", int cong = 42652)
    {
        string lyDoLoa;
        (loa, lyDoLoa) = ThietBiAmThanh.MoLoa();
        nhatKy = new NhatKy($"xem {ten}", 8, 20, 648, 84);
        dinhKy = new ThongKeDinhKy($"xem {ten}");
        Text = $"Người xem trực tiếp – MR6.5 ({ten})";
        BackColor = GD.Nen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.Manual;
        ClientSize = new Size(672, 580);

        var g = GD.Nhom("Kết nối server", 8, 4, 656, 52);
        TxtIp = GD.O(host, 32, 20, 110);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 186, 20, 56);
        TxtTen = GD.O(ten, 282, 20, 70);
        BtnKetNoi = GD.Nut("Kết nối", GD.XanhLa, 368, 17, 110);
        GD.Them(g, GD.Nhan("IP:", 8, 23), TxtIp, GD.Nhan("Port:", 150, 23), TxtCong, GD.Nhan("Tên:", 250, 23), TxtTen, BtnKetNoi);

        var g2 = GD.Nhom("Hình (phóng ×2)", 8, 60, 336, 340);
        hinh.Location = new Point(8, 20);
        hinh.Dat(null, "Chưa kết nối");
        lblKhung = GD.Nhan("Khung: –", 8, 264, 320, GD.CoDinh);
        thanh = new ThanhMuc(52, 288, 200);
        ChkLoa = new CheckBox { Text = "Loa", Checked = loa != null, Enabled = loa != null, Location = new Point(262, 286), AutoSize = true };
        lblLoa = GD.Nhan(loa != null ? "Loa: THẬT – thiết bị mặc định" : $"Loa: {lyDoLoa}", 8, 314, 320);
        lblLoa.ForeColor = loa != null ? Color.FromArgb(0x00, 0x60, 0x00) : Color.FromArgb(0x8A, 0x4B, 0x00);
        GD.Them(g2, hinh, lblKhung, GD.Nhan("Tiếng:", 8, 290), thanh, ChkLoa, lblLoa);

        var g3 = GD.Nhom("Điều khiển", 350, 60, 314, 104);
        ChkDongBo = new CheckBox { Text = "Đồng bộ tiếng-hình (tiếng làm đồng hồ chủ)", Checked = true, Location = new Point(8, 18), AutoSize = true };
        RdoAm = new RadioButton { Text = "Âm thanh", Location = new Point(50, 44), AutoSize = true };
        RdoVideo = new RadioButton { Text = "Video", Location = new Point(134, 44), AutoSize = true };
        RdoCaHai = new RadioButton { Text = "Cả hai", Location = new Point(200, 44), AutoSize = true, Checked = true };
        NumDem = GD.So(176, 72, 1, 25, Chung.DoSauMacDinh);
        BtnChup = GD.Nut("Chụp", GD.XanhDuong, 236, 70, 66, 26);
        GD.Them(g3, ChkDongBo, GD.Nhan("Nhận:", 8, 46), RdoAm, RdoVideo, RdoCaHai, GD.Nhan("Bộ đệm jitter (khung 20 ms):", 8, 75),
                NumDem, BtnChup);

        var g4 = GD.Nhom("Thống kê", 350, 168, 314, 232);
        int y = 20;
        foreach (var (k, t) in new[] { ("tre", "Trễ đầu-cuối (tiếng)*"), ("lech", "Lệch tiếng-hình hiện tại"), ("lech_tb", "Lệch TB giai đoạn này"),
                                       ("mat", "Tiếng nhận / mất"), ("khung", "Khung nhận I / P"), ("dung", "Đứng hình / PLI gửi"),
                                       ("dau", "Khung đầu sau") })
        {
            GD.Them(g4, GD.Nhan(t + ":", 8, y, 150));
            lblTk[k] = GD.Nhan("–", 158, y - 1, 150, GD.Dam);
            lblTk[k].Height = 20;
            GD.Them(g4, lblTk[k]);
            y += 26;
        }
        var chuThich = GD.Nhan("* giả định cùng đồng hồ (cùng máy / NTP); lệch +: hình chậm hơn tiếng", 8, y + 2, 300);
        chuThich.Height = 30;
        chuThich.Font = new Font(Control.DefaultFont.FontFamily, 7);
        chuThich.ForeColor = Color.FromArgb(0x40, 0x40, 0x40);
        GD.Them(g4, chuThich);

        var g5 = GD.Nhom("Nhật ký", 8, 404, 656, 112);
        GD.Them(g5, nhatKy);
        LblTrangThai = new Label { Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: chưa kết nối", BackColor = Color.White,
                                   BorderStyle = BorderStyle.Fixed3D, TextAlign = ContentAlignment.MiddleLeft };
        ClientSize = new Size(672, 548);
        Controls.AddRange([g, g2, g3, g4, g5, LblTrangThai]);

        BtnKetNoi.Click += (_, _) => BamKetNoi();
        ChkDongBo.CheckedChanged += (_, _) => Lop?.DatDongBo(ChkDongBo.Checked);
        foreach (var r in new[] { RdoAm, RdoVideo, RdoCaHai })
            r.CheckedChanged += (_, _) =>
            {
                if (r.Checked) Lop?.DangKy(!RdoVideo.Checked, !RdoAm.Checked);
            };
        NumDem.ValueChanged += (_, _) =>
        {
            if (Lop == null) return;
            Lop.DatDoSau((int)NumDem.Value);
            nhatKy.Ghi($"Bộ đệm jitter: {NumDem.Value} khung = {NumDem.Value * 20} ms");
        };
        ChkLoa.CheckedChanged += (_, _) =>
        {
            if (Lop != null) Lop.Loa = ChkLoa.Checked ? loa : null;
        };
        BtnChup.Click += (_, _) => Chup();
        nhipHinh.Tick += (_, _) => VeHinh();
        nhip.Tick += (_, _) => CapNhat();
        nhipHinh.Start();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhipHinh.Stop();
            nhip.Stop();
            ui.Dispose();
            if (Lop != null)
            {
                Lop.Loa = null;
                Lop.Dong();
            }
            loa?.Dispose();
        };
    }

    void Ghi(string t) => ui.DuaVao(() => nhatKy.Ghi(t));

    // ------------------------------------------------------------ kết nối
    void BamKetNoi()
    {
        if (Lop is { DaKetNoi: true })
        {
            Lop.Ngat();
            DaNgat("đã ngắt kết nối");
            return;
        }
        string ten = TxtTen.Text.Trim(), host = TxtIp.Text.Trim();
        if (!int.TryParse(TxtCong.Text, out int cong))
        {
            nhanTt = "cổng không hợp lệ";
            return;
        }
        if (Lop != null)
        {
            Lop.Loa = null;                                           // loa dùng chung giữa các lần kết nối: không đóng
            Lop.Dong();
        }
        var lop = Lop = new NguoiXem(ten, Ghi, (int)NumDem.Value, ChkDongBo.Checked, !RdoVideo.Checked, !RdoAm.Checked,
                                     loa: ChkLoa.Checked ? loa : null);
        BtnKetNoi.Enabled = false;
        BtnKetNoi.Text = "Đang kết nối…";
        soDaVe = -1;
        Task.Run(() =>
        {
            try
            {
                int ma = lop.KetNoi(host, cong);
                ui.DuaVao(() => DaKetNoi(ma));
            }
            catch (Exception e) when (e is IOException or System.Net.Sockets.SocketException)
            {
                ui.DuaVao(() => DaNgat($"không kết nối được – {e.Message}"));
            }
        });
    }

    void DaKetNoi(int ma)
    {
        BtnKetNoi.Enabled = true;
        (BtnKetNoi.Text, BtnKetNoi.BackColor) = ("Ngắt kết nối", GD.Do);
        nhanTt = $"đã kết nối, mã {ma}";
        hinh.Dat(null, "Chờ khung hình…");
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = false;
    }

    void DaNgat(string text)
    {
        BtnKetNoi.Enabled = true;
        (BtnKetNoi.Text, BtnKetNoi.BackColor) = ("Kết nối", GD.XanhLa);
        nhanTt = text;
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = true;
    }

    /// <summary>Chụp khung đang hiện (có chú thích) thành out/&lt;tên&gt;_gui_chup_N.png; trả về đường dẫn hoặc null.</summary>
    public string? Chup()
    {
        if (Lop?.Chup() is not { } c)
        {
            nhatKy.Ghi("Chưa có hình để chụp");
            return null;
        }
        var (w, h, rgb) = NguoiXem.AnhChup(c.K, c.Lech);
        Directory.CreateDirectory("out");
        soChup++;
        string duong = $"out/{Lop.Ten}_gui_chup_{soChup}.png";
        Chung.GhiPng(duong, rgb, w, h);
        nhatKy.Ghi($"Đã chụp {duong}");
        return duong;
    }

    // ------------------------------------------------------------ vẽ lại
    void VeHinh()
    {
        if (Lop is not { } lop) return;
        var (k, so) = lop.LayHien();
        if (k != null && so != soDaVe)
        {
            hinh.Dat(GD.SangBitmap(k.Anh, k.W, k.H));
            lblKhung.Text = $"Khung #{k.So}  hình t={k.Ts / 1000.0:F3} s  tiếng t=" + (k.Am is { } am ? $"{am / 1000:F3} s" : "–");
            soDaVe = so;
        }
        else if (k == null && soDaVe != -2)
        {
            hinh.Dat(null);
            soDaVe = -2;
        }
    }

    void CapNhat()
    {
        if (Lop is not { } lop) return;
        var tt = lop.TrangThai();
        thanh.Dat(tt.MucAm);
        if (tt.DaKetNoi)
            dinhKy.InNeuDenHan(() =>
                $"tiếng {tt.GoiAm} gói, mất {tt.MatAm}, mức {(double.IsInfinity(tt.MucAm) ? "-inf" : tt.MucAm.ToString("F0", CultureInfo.InvariantCulture))} dBFS," +
                $" loa {(loa != null && ChkLoa.Checked ? "BẬT" : "tắt")}" +
                $" | hình I {tt.KhungI} P {tt.KhungP}, đứng {tt.DungHinh}, lệch {GD.ChuMs(tt.LechHien, true)}, trễ {GD.ChuMs(tt.TreHien)}" +
                $" | đồng bộ {(tt.DongBo ? "BẬT" : "TẮT")}");
        var gd = tt.GiaiDoan[^1];
        lblTk["tre"].Text = GD.ChuMs(tt.TreHien);
        lblTk["lech"].Text = GD.ChuMs(tt.LechHien, true);
        lblTk["lech"].ForeColor = tt.LechHien is { } l && Math.Abs(l) <= 40 ? Color.FromArgb(0x00, 0x60, 0x00) : Color.FromArgb(0xB0, 0x00, 0x00);
        lblTk["lech_tb"].Text = (gd.N > 0 ? GD.ChuMs(gd.Tong / gd.N, true) : "–") + $" (đồng bộ {(gd.DongBo ? "BẬT" : "TẮT")})";
        lblTk["mat"].Text = $"{tt.GoiAm} / {tt.MatAm} gói";
        lblTk["khung"].Text = $"{tt.KhungI} / {tt.KhungP} (hỏng {tt.Hong})";
        lblTk["dung"].Text = $"{tt.DungHinh} / {tt.PliGui}";
        lblTk["dau"].Text = tt.KhungDauMs is { } d ? $"{d:F0} ms ({tt.NguonDau})" : "–";
        if (nhanTt.StartsWith("đã kết nối", StringComparison.Ordinal) && !tt.DaKetNoi) DaNgat("mất kết nối server");
        LblTrangThai.Text = "Trạng thái: " + (tt.DaKetNoi ? $"đang xem – nhận {tt.Kbps:F0} kbit/s TB" : nhanTt);
    }
}
