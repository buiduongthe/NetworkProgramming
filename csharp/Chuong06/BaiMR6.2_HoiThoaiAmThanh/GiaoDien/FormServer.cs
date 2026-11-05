using System.Collections.Concurrent;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_2;

// Cửa sổ "Bảng điều khiển âm thanh" của server.
// Quy tắc luồng: luồng giao diện (luồng chạy Application.Run) là luồng DUY NHẤT chạm vào điều khiển.
//   - Phòng (PhongAmThanh) chạy trên các luồng nền: nhận kết nối, nhận UDP, trộn 20 ms. Nhật ký của phòng được
//     đẩy vào hàng đợi an toàn luồng (ConcurrentQueue); bộ hẹn giờ giao diện 100 ms lấy ra và hiển thị.
//   - Mỗi 100 ms bộ hẹn giờ cũng chụp nhanh trạng thái phòng (trong khóa phòng, rất ngắn) để vẽ lại các hàng client
//     và thanh đo mức – không có luồng nền nào gọi thẳng vào điều khiển.
//   - Nút bấm gọi phương thức của phòng (có khóa bên trong, không chờ mạng) nên không chặn giao diện.
class FormServer : Form
{
    readonly ConcurrentQueue<string> nhatKy = new();
    PhongAmThanh? phong;
    INguonMic? micServer;                                     // micro của máy server (null = không nói)
    PhatAmWinMM? loa;                                         // loa của máy server (null = không nghe)
    double doLoiMic = 1.0;
    readonly List<string> tenMicThat = WinMM.DanhSachMicro();
    readonly Dictionary<uint, HangKhach> hang = [];
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 };
    readonly System.Windows.Forms.Timer treAmLuongChung;

    // ---- Điều khiển (public để kịch bản demo thao tác như người dùng)
    public readonly TextBox TxtIp, TxtTcp, TxtUdp;
    public readonly Button BtnStart, BtnStop, BtnMoTatCa, BtnTatTatCa, BtnMicStart, BtnMicStop;
    public readonly TrackBar TrkAmLuongChung, TrkMic;
    readonly Label lblAmLuongChung, lblMic, lblLoaiMic, lblLoa, lblTrangThai, lblSoClient;
    public readonly ComboBox CboMic;
    public readonly CheckBox ChkNghe;
    readonly ThanhMucAm mucMic = new() { Width = 300 };
    public readonly FlowLayoutPanel DsHang;
    readonly TextBox txtNhatKy;

    public FormServer(string host, int congTcp, int congUdp)
    {
        Text = "Bảng điều khiển âm thanh – server";
        ClientSize = new Size(1080, 720);
        StartPosition = FormStartPosition.Manual;
        Location = new Point(10, 10);
        BackColor = Color.WhiteSmoke;

        // ---- Hàng nhóm điều khiển trên cùng
        var tren = new FlowLayoutPanel { Dock = DockStyle.Top, Height = 170, BackColor = GD.NenXanh, Padding = new Padding(4) };
        var gKetNoi = GD.Nhom("Kết nối", 250, 150);
        TxtIp = GD.O(host, 70, 24, 160);
        TxtTcp = GD.O(congTcp.ToString(CultureInfo.InvariantCulture), 70, 52, 60);
        TxtUdp = GD.O(congUdp.ToString(CultureInfo.InvariantCulture), 170, 52, 60);
        BtnStart = GD.Nut("Start Server", GD.NutXanhLa, 12, 90, 108, 34);
        BtnStop = GD.Nut("Stop Server", GD.NutDo, 126, 90, 108, 34);
        BtnStop.Enabled = false;
        gKetNoi.Controls.AddRange([GD.Nhan("IP:", 12, 27), TxtIp, GD.Nhan("TCP:", 12, 55), TxtTcp, GD.Nhan("UDP:", 134, 55), TxtUdp, BtnStart, BtnStop]);

        var gChung = GD.Nhom("Điều khiển chung", 250, 150);
        BtnMoTatCa = GD.Nut("Mở mic tất cả", GD.NutXanhDuong, 12, 26, 108, 34);
        BtnTatTatCa = GD.Nut("Tắt mic tất cả", GD.NutCam, 126, 26, 108, 34);
        lblAmLuongChung = GD.Nhan("Âm lượng chung: 100%", 12, 74, 220);
        TrkAmLuongChung = GD.Truot(8, 96, 230);
        gChung.Controls.AddRange([BtnMoTatCa, BtnTatTatCa, lblAmLuongChung, TrkAmLuongChung]);

        var gMic = GD.Nhom("Micro của server", 330, 150);
        CboMic = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(12, 24), Width = 306 };
        CboMic.Items.Add("Giả lập (sin 330 Hz)");
        for (int i = 0; i < tenMicThat.Count; i++) CboMic.Items.Add($"{i}: {tenMicThat[i]}");
        CboMic.SelectedIndex = 0;
        BtnMicStart = GD.Nut("Start Mic", GD.NutXanhLa, 12, 54, 90, 28);
        BtnMicStop = GD.Nut("Stop Mic", GD.NutDo, 108, 54, 90, 28);
        BtnMicStop.Enabled = false;
        lblLoaiMic = GD.Nhan("Mic: tắt", 204, 60, 120);
        lblMic = GD.Nhan("Mic Volume: 100%", 12, 92, 120);
        TrkMic = GD.Truot(130, 88, 190);
        mucMic.Location = new Point(12, 120);
        gMic.Controls.AddRange([CboMic, BtnMicStart, BtnMicStop, lblLoaiMic, lblMic, TrkMic, mucMic]);

        var gNghe = GD.Nhom("Nghe", 200, 150);
        ChkNghe = new CheckBox { Text = "Phát tiếng client ra loa", Location = new Point(12, 28), AutoSize = true };
        lblLoa = GD.Nhan(WinMM.CoLoa() ? "Loa: có thiết bị" : "Loa: không có thiết bị", 12, 58, 180);
        gNghe.Controls.AddRange([ChkNghe, lblLoa, GD.Nhan("(không gồm tiếng server)", 12, 84, 180)]);
        foreach (var g in new[] { gKetNoi, gChung, gMic, gNghe }) GD.PhongThuong(g);
        tren.Controls.AddRange([gKetNoi, gChung, gMic, gNghe]);

        // ---- Dòng trạng thái
        var trangThai = new Panel { Dock = DockStyle.Top, Height = 44, BackColor = Color.White };
        lblTrangThai = GD.Nhan("Server chưa chạy", 10, 4, 900);
        lblTrangThai.Font = new Font(Font, FontStyle.Bold);
        lblSoClient = GD.Nhan("Số client: 0", 10, 24, 400);
        trangThai.Controls.AddRange([lblTrangThai, lblSoClient]);

        // ---- Danh sách client (mỗi client một hàng) và nhật ký
        DsHang = new FlowLayoutPanel { Dock = DockStyle.Fill, AutoScroll = true, FlowDirection = FlowDirection.TopDown, WrapContents = false, Padding = new Padding(6) };
        txtNhatKy = new TextBox { Dock = DockStyle.Bottom, Height = 170, Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical, BackColor = Color.White, Font = new Font(FontFamily.GenericMonospace, 8.5f) };
        // Thứ tự thêm quyết định thứ tự xếp Dock: điều khiển thêm SAU được xếp TRƯỚC (sát mép hơn)
        Controls.AddRange([DsHang, txtNhatKy, trangThai, tren]);

        // ---- Sự kiện
        treAmLuongChung = GD.TreLenh(300, () => phong?.DatAmLuongChung(TrkAmLuongChung.Value / 100.0));
        BtnStart.Click += (_, _) => BatDauServer();
        BtnStop.Click += (_, _) => DungServer();
        BtnMoTatCa.Click += (_, _) => { if (phong != null) phong.DatKhoaMic(phong.Chon("tat_ca"), false, "tất cả"); };
        BtnTatTatCa.Click += (_, _) => { if (phong != null) phong.DatKhoaMic(phong.Chon("tat_ca"), true, "tất cả"); };
        TrkAmLuongChung.ValueChanged += (_, _) =>
        {
            lblAmLuongChung.Text = $"Âm lượng chung: {TrkAmLuongChung.Value}%";
            GD.ChayLai(treAmLuongChung);                        // gửi sau khi người dùng dừng kéo 300 ms
        };
        TrkMic.ValueChanged += (_, _) =>
        {
            lblMic.Text = $"Mic Volume: {TrkMic.Value}%";
            doLoiMic = TrkMic.Value / 100.0;
            if (phong != null) lock (phong.Khoa) phong.DoLoiServer = doLoiMic;
        };
        BtnMicStart.Click += (_, _) => BatMicServer();
        BtnMicStop.Click += (_, _) => TatMicServer();
        ChkNghe.CheckedChanged += (_, _) => DoiNghe();
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            DungServer();
            TatMicServer();
            loa?.Dispose();
        };
    }

    /// <summary>Gọi được từ mọi luồng: chỉ đưa vào hàng đợi, luồng giao diện sẽ hiển thị.</summary>
    public void Log(string s)
    {
        nhatKy.Enqueue($"[{DateTime.Now:HH:mm:ss}] {s}");
        Console.WriteLine($"[server] {s}");                     // Console an toàn luồng; dùng cho nhật ký demo
    }

    public PhongAmThanh? Phong => phong;

    void BatDauServer()
    {
        try
        {
            var p = new PhongAmThanh(Log)
            {
                DoLoiServer = doLoiMic,
                LayKhungServer = () => micServer?.LayKhung(),      // gọi trong luồng trộn mỗi 20 ms
                NgheServer = loa != null ? loa.Phat : null,
                AmLuongChung = TrkAmLuongChung.Value / 100.0,
            };
            p.BatDau(TxtIp.Text.Trim(), int.Parse(TxtTcp.Text), int.Parse(TxtUdp.Text));
            phong = p;
            lblTrangThai.Text = $"Server đang chạy tại {TxtIp.Text.Trim()} (TCP {TxtTcp.Text}, UDP {TxtUdp.Text})";
            lblTrangThai.ForeColor = Color.Green;
            BtnStart.Enabled = false;
            BtnStop.Enabled = true;
            TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = false;
        }
        catch (Exception e) when (e is FormatException || e is System.Net.Sockets.SocketException || e is OverflowException ||
                                  e is ArgumentException)
        {
            lblTrangThai.Text = $"Không khởi động được server: {e.Message}";
            lblTrangThai.ForeColor = Color.Red;
        }
    }

    void DungServer()
    {
        if (phong == null) return;
        phong.Dung();
        phong.InTongKet();
        phong = null;
        foreach (var h in hang.Values) DsHang.Controls.Remove(h.Khung);
        hang.Clear();
        lblTrangThai.Text = "Server đã dừng";
        lblTrangThai.ForeColor = Color.DarkRed;
        BtnStart.Enabled = true;
        BtnStop.Enabled = false;
        TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = true;
    }

    /// <summary>Start Mic: chỉ lúc này mới mở micro thật (không tự mở khi khởi động chương trình).</summary>
    void BatMicServer()
    {
        INguonMic m;
        int chon = CboMic.SelectedIndex;
        try { m = chon <= 0 ? new MicGiaLap(330) : new MicThat(chon - 1, tenMicThat[chon - 1]); }
        catch (Exception e) when (e is InvalidOperationException || e is DllNotFoundException || e is EntryPointNotFoundException)
        {
            Log($"Không mở được micro thật ({e.Message}) – chuyển sang giả lập");
            m = new MicGiaLap(330);
            CboMic.SelectedIndex = 0;
        }
        var cu = micServer;
        micServer = m;
        cu?.Dispose();
        lblLoaiMic.Text = m.That ? "Mic: THẬT" : "Mic: giả lập";
        lblLoaiMic.ForeColor = m.That ? Color.DarkRed : Color.DarkBlue;
        BtnMicStart.Enabled = false;
        BtnMicStop.Enabled = true;
        Log($"Server bật micro: {m.MoTa}");
    }

    void TatMicServer()
    {
        var cu = micServer;
        micServer = null;
        cu?.Dispose();
        lblLoaiMic.Text = "Mic: tắt";
        lblLoaiMic.ForeColor = Color.Black;
        BtnMicStart.Enabled = true;
        BtnMicStop.Enabled = false;
        if (cu != null) Log("Server tắt micro");
    }

    void DoiNghe()
    {
        if (ChkNghe.Checked && loa == null)
        {
            try { loa = new PhatAmWinMM(); }
            catch (Exception e) when (e is InvalidOperationException || e is DllNotFoundException || e is EntryPointNotFoundException)
            {
                Log($"Không mở được loa: {e.Message}");
                ChkNghe.Checked = false;
                return;
            }
            Log("Server nghe: phát tổng tiếng các client ra loa");
        }
        else if (!ChkNghe.Checked && loa != null)
        {
            if (phong != null) lock (phong.Khoa) phong.NgheServer = null;
            loa.Dispose();
            loa = null;
            Log("Server thôi nghe");
        }
        if (phong != null) lock (phong.Khoa) phong.NgheServer = loa != null ? loa.Phat : null;
    }

    /// <summary>Bộ hẹn giờ 100 ms (luồng giao diện): hiển thị nhật ký, đồng bộ các hàng client với trạng thái phòng.</summary>
    void CapNhat()
    {
        var moi = new List<string>();
        while (nhatKy.TryDequeue(out var s)) moi.Add(s);
        if (moi.Count > 0)
        {
            if (txtNhatKy.TextLength > 60000) txtNhatKy.Clear();
            txtNhatKy.AppendText(string.Join(Environment.NewLine, moi) + Environment.NewLine);
        }
        if (phong == null)
        {
            mucMic.DatMuc(double.NegativeInfinity);
            lblSoClient.Text = "Số client: 0";
            return;
        }
        List<TrangThaiKhach> ds;
        double mucServer;
        lock (phong.Khoa)                                     // chụp nhanh trạng thái trong khóa (vài micro giây)
        {
            ds = phong.DsKhach.Values.Select(k => new TrangThaiKhach(k.Ma, k.Ten, k.LyDo(), k.MicMo, k.ServerKhoa, k.MucVao,
                                                                     k.DoLoi, k.AmLuongNghe, k.BoDem.Nhan, k.BoDem.Mat)).ToList();
            mucServer = phong.MucServer;
        }
        mucMic.DatMuc(mucServer);
        lblSoClient.Text = $"Số client: {ds.Count}";
        foreach (var ma in hang.Keys.Where(m => ds.All(k => k.Ma != m)).ToList())   // client đã rời
        {
            DsHang.Controls.Remove(hang[ma].Khung);
            hang.Remove(ma);
        }
        foreach (var k in ds)
        {
            if (!hang.TryGetValue(k.Ma, out var h))
            {
                h = new HangKhach(k.Ma, k.Ten, this);
                hang[k.Ma] = h;
                DsHang.Controls.Add(h.Khung);
            }
            h.CapNhat(k);
        }
    }

    /// <summary>Thao tác trên một client theo mã (gọi từ hàng của client đó).</summary>
    public void KhoaMic(uint ma, bool khoa)
    {
        var p = phong;
        if (p == null) return;
        Khach? k;
        lock (p.Khoa) p.DsKhach.TryGetValue(ma, out k);
        if (k != null) p.DatKhoaMic([k], khoa, k.Ten);
    }

    public void DatDoLoi(uint ma, double g)
    {
        var p = phong;
        if (p == null) return;
        Khach? k;
        lock (p.Khoa) p.DsKhach.TryGetValue(ma, out k);
        if (k != null) p.DatDoLoi([k], g, k.Ten);
    }

    public HangKhach? Hang(string ten) => hang.Values.FirstOrDefault(h => h.Ten == ten);
}

record TrangThaiKhach(uint Ma, string Ten, string LyDo, bool MicMo, bool ServerKhoa, double MucVao,
                      double DoLoi, double AmLuongNghe, int GoiNhan, long GoiMat);

/// <summary>Một hàng trong danh sách client của server: tên, trạng thái mic, mức, khóa, độ lợi, âm lượng nghe.</summary>
class HangKhach
{
    public readonly uint Ma;
    public readonly string Ten;
    public readonly Panel Khung = new() { Size = new Size(1030, 44), BackColor = Color.FromArgb(230, 236, 245), Margin = new Padding(2) };
    readonly Label lblMic = GD.Nhan("", 150, 13, 110);
    readonly ThanhMucAm muc = new() { Location = new Point(266, 13), Width = 190 };
    public readonly Button BtnKhoa = GD.Nut("Khóa", GD.NutCam, 466, 7, 84, 30);
    readonly Label lblLoi = GD.Nhan("Độ lợi: 100%", 560, 13, 100);
    public readonly TrackBar TrkLoi = GD.Truot(660, 9, 170);
    readonly Label lblNghe = GD.Nhan("", 840, 6, 185);
    readonly Label lblGoi = GD.Nhan("", 840, 24, 185);
    readonly System.Windows.Forms.Timer treLoi;
    bool dangKhoa, dangKeo, dangGan;

    public HangKhach(uint ma, string ten, FormServer f)
    {
        (Ma, Ten) = (ma, ten);
        var lblTen = GD.Nhan($"#{ma} {ten}", 8, 13, 136);
        lblTen.Font = new Font(SystemFonts.DefaultFont, FontStyle.Bold);
        Khung.Controls.AddRange([lblTen, lblMic, muc, BtnKhoa, lblLoi, TrkLoi, lblNghe, lblGoi]);
        BtnKhoa.Click += (_, _) => f.KhoaMic(Ma, !dangKhoa);
        treLoi = GD.TreLenh(300, () =>
        {
            dangKeo = false;
            f.DatDoLoi(Ma, TrkLoi.Value / 100.0);
        });
        TrkLoi.ValueChanged += (_, _) =>
        {
            lblLoi.Text = $"Độ lợi: {TrkLoi.Value}%";
            if (dangGan) return;                              // giá trị do server báo về, không phải người dùng kéo
            dangKeo = true;                                   // đang kéo: không ghi đè bằng giá trị cũ của server
            GD.ChayLai(treLoi);
        };
    }

    public void CapNhat(TrangThaiKhach k)
    {
        dangKhoa = k.ServerKhoa;
        lblMic.Text = $"Mic: {k.LyDo}";
        lblMic.ForeColor = k.MicMo ? Color.DarkGreen : Color.DarkRed;
        muc.DatMuc(k.MucVao);
        BtnKhoa.Text = k.ServerKhoa ? "Gỡ khóa" : "Khóa";
        BtnKhoa.BackColor = k.ServerKhoa ? GD.NutXanhLa : GD.NutCam;
        int loi = (int)Math.Round(k.DoLoi * 100);
        if (!dangKeo && TrkLoi.Value != Math.Clamp(loi, 0, 200))
        {
            dangGan = true;
            TrkLoi.Value = Math.Clamp(loi, 0, 200);
            dangGan = false;
        }
        lblNghe.Text = $"Client nghe: {GD.PhanTram(k.AmLuongNghe)}";
        lblGoi.Text = $"Gói nhận {k.GoiNhan}, mất {k.GoiMat}";
    }

    public double Muc => muc.LayMuc();
}
