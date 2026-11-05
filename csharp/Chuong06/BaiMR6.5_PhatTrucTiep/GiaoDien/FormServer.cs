using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Cửa sổ máy chủ phát trực tiếp (SFU). Lõi MayChuPhat (../MayChu.cs) chạy các luồng mạng;
// cửa sổ chỉ gọi phương thức của lõi và cứ 100 ms đọc TrangThai() để vẽ lại. Server bật XemTruoc: tự giải mã MỘT
// bản luồng video (chỉ để hiện ảnh xem trước, việc chuyển tiếp vẫn là nhân bản nguyên byte – không giải nén).
sealed class FormServer : Form
{
    readonly HangDoiGiaoDien ui = new();
    public readonly MayChuPhat Lop;
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 };
    (long Ts, long So)? soDaVe;
    public readonly TextBox TxtIp, TxtTcp, TxtUdp;
    public readonly Button BtnStart, BtnStop, BtnNgat;
    public readonly CheckBox ChkBoDem;
    readonly Label lblBoDem, lblVao, lblRa, lblHeSo, lblPli, lblDongHo, lblPhat, lblTrangThai;
    readonly OAnh xemTruoc = new(2 * Chung.Rong, 2 * Chung.Cao);
    readonly ListView bang;
    readonly NhatKy nhatKy;
    readonly ThongKeDinhKy dinhKy = new("server");

    public FormServer(string host = "127.0.0.1", int congTcp = 42652, int? congUdp = null)
    {
        nhatKy = new NhatKy("server", 8, 20, 876, 110);
        Lop = new MayChuPhat(t => ui.DuaVao(() => nhatKy.Ghi(t))) { XemTruoc = true };
        Text = "Máy chủ phát trực tiếp (SFU) – MR6.5";
        BackColor = GD.Nen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.Manual;
        ClientSize = new Size(908, 580);

        var g = GD.Nhom("Kết nối", 8, 4, 236, 112);
        TxtIp = GD.O(host, 50, 22, 170);
        TxtTcp = GD.O(congTcp.ToString(CultureInfo.InvariantCulture), 50, 50, 60);
        TxtUdp = GD.O((congUdp ?? congTcp + 1).ToString(CultureInfo.InvariantCulture), 160, 50, 60);
        BtnStart = GD.Nut("START SERVER", GD.XanhLa, 8, 78, 106);
        BtnStop = GD.Nut("STOP SERVER", GD.Do, 120, 78, 106);
        BtnStop.Enabled = false;
        GD.Them(g, GD.Nhan("IP:", 8, 25), TxtIp, GD.Nhan("TCP:", 8, 53), TxtTcp, GD.Nhan("UDP:", 120, 53), TxtUdp, BtnStart, BtnStop);

        var g2 = GD.Nhom("Bộ đệm GOP (GOP cache)", 250, 4, 250, 112);
        ChkBoDem = new CheckBox { Text = "Bật – người vào muộn nhận ngay\nkhung I mới nhất + các P sau nó", Checked = true,
                                  Location = new Point(8, 20), Size = new Size(236, 40) };
        lblBoDem = GD.Nhan("Đang giữ: –", 8, 66, 236);
        GD.Them(g2, ChkBoDem, lblBoDem);

        var g3 = GD.Nhom("Chỉ số (mỗi giây)", 506, 4, 394, 112);
        lblVao = GD.Nhan("Vào: –", 8, 20, 380, GD.Dam);
        lblRa = GD.Nhan("Ra: –", 8, 42, 380, GD.Dam);
        lblHeSo = GD.Nhan("Hệ số nhân bản (ra/vào): –", 8, 64, 380, GD.Dam);
        lblHeSo.ForeColor = Color.FromArgb(0x00, 0x3A, 0x80);
        lblPli = GD.Nhan("PLI chuyển: 0 (bỏ 0)", 8, 88, 380);
        foreach (var l in new[] { lblVao, lblRa, lblHeSo }) l.Height = 20;
        GD.Them(g3, lblVao, lblRa, lblHeSo, lblPli);

        var g4 = GD.Nhom("Xem trước luồng (server tự giải mã)", 8, 120, 336, 290);
        xemTruoc.Location = new Point(8, 20);
        xemTruoc.Dat(null, "Chưa có luồng");
        lblDongHo = GD.Nhan("Đồng hồ: –", 8, 266, 320, GD.CoDinh);
        GD.Them(g4, xemTruoc, lblDongHo);

        var g5 = GD.Nhom("Người xem", 350, 120, 550, 290);
        bang = new ListView { View = View.Details, FullRowSelect = true, MultiSelect = true, HideSelection = false,
                              Location = new Point(8, 20), Size = new Size(534, 226), Font = GD.Thuong };
        foreach (var (t, w, phai) in new[] { ("Tên", 60, false), ("IP:cổng UDP", 130, false), ("Nhận", 45, true), ("Gói gửi", 65, true),
                                             ("kbit/s", 55, true), ("Khung I đầu", 150, false) })
            bang.Columns.Add(t, w, phai ? HorizontalAlignment.Right : HorizontalAlignment.Left);
        BtnNgat = GD.Nut("Ngắt người chọn", GD.Cam, 8, 252, 130);
        lblPhat = GD.Nhan("  Máy phát: –", 144, 258, 390);
        GD.Them(g5, bang, BtnNgat, lblPhat);

        var g6 = GD.Nhom("Nhật ký", 8, 414, 892, 136);
        GD.Them(g6, nhatKy);
        lblTrangThai = new Label { Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: server chưa chạy", BackColor = Color.White,
                                   BorderStyle = BorderStyle.Fixed3D, TextAlign = ContentAlignment.MiddleLeft };
        Controls.AddRange([g, g2, g3, g4, g5, g6, lblTrangThai]);

        BtnStart.Click += (_, _) => BatDau();
        BtnStop.Click += (_, _) => Dung();
        ChkBoDem.CheckedChanged += (_, _) => Lop.DatBoDem(ChkBoDem.Checked);
        BtnNgat.Click += (_, _) => NgatChon();
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            ui.Dispose();
            Lop.Dung();
        };
    }

    // ------------------------------------------------------------ nút
    void BatDau()
    {
        try
        {
            Lop.BoDemBat = ChkBoDem.Checked;
            Lop.BatDau(TxtIp.Text.Trim(), int.Parse(TxtTcp.Text, CultureInfo.InvariantCulture), int.Parse(TxtUdp.Text, CultureInfo.InvariantCulture));
        }
        catch (Exception e) when (e is System.Net.Sockets.SocketException or FormatException or OverflowException or ArgumentException)
        {
            nhatKy.Ghi($"Không khởi động được: {e.Message}");
            return;
        }
        BtnStart.Enabled = false;
        BtnStop.Enabled = true;
        TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = false;
    }

    void Dung()
    {
        BtnStop.Enabled = false;
        Task.Run(() =>                                                // chờ luồng nền dừng: không chặn giao diện
        {
            Lop.Dung();
            ui.DuaVao(() =>
            {
                BtnStart.Enabled = true;
                TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = true;
            });
        });
    }

    void NgatChon()
    {
        foreach (ListViewItem it in bang.SelectedItems) Lop.Ngat(ma: (int)it.Tag!);
    }

    /// <summary>Chọn dòng theo tên (dùng trong kịch bản demo).</summary>
    public void ChonNguoiXem(string ten)
    {
        foreach (ListViewItem it in bang.Items) it.Selected = it.Text == ten;
    }

    // ------------------------------------------------------------ vẽ lại định kỳ
    void CapNhat()
    {
        var tt = Lop.TrangThai();
        if (Lop.Chay)
        {
            dinhKy.InNeuDenHan(() =>
                $"vào {tt.KbpsVao:F0} kbit/s (tiếng {tt.KbpsVaoAm:F0}, hình {tt.KbpsVaoHinh:F0}), ra {tt.KbpsRa:F0}, hệ số {tt.HeSo:F2}," +
                $" {(tt.DangPhat ? "đang phát" : "chưa phát")};" + string.Concat(tt.NguoiXem.Select(x => $" {x.Ten} {x.Nhan} {x.Kbps:F0}")));
            lblVao.Text = $"Vào: {tt.KbpsVao:F0} kbit/s (tiếng {tt.KbpsVaoAm:F0}, hình {tt.KbpsVaoHinh:F0})";
            lblRa.Text = $"Ra: {tt.KbpsRa:F0} kbit/s tới {tt.NguoiXem.Count} người xem";
            lblHeSo.Text = $"Hệ số nhân bản (ra/vào): {tt.HeSo:F2}";
            lblPli.Text = $"PLI chuyển: {tt.PliChuyen} (bỏ {tt.PliBo} do lọc 500 ms)";
            lblBoDem.Text = tt.BoDemBat && tt.BoDemSoI != null
                ? $"Đang giữ: I #{tt.BoDemSoI} và các P sau: {tt.BoDemGoi} gói, {tt.BoDemByte / 1024.0:F0} KB"
                : tt.BoDemBat ? "Đang giữ: – (chờ khung I)" : "Tắt: người vào muộn chờ khung I";
            lblPhat.Text = $"  Máy phát: {tt.TenPhat ?? "–"}" + (tt.DangPhat ? " (đang phát)" : "");
            lblTrangThai.Text = $"Trạng thái: server chạy tại {Lop.DiaChi} (UDP {Lop.CongUdp}) – {(tt.DangPhat ? "đang phát" : "chưa phát")}," +
                                $" {tt.NguoiXem.Count} người xem";
        }
        else lblTrangThai.Text = "Trạng thái: server chưa chạy";
        var co = new HashSet<int>();
        bang.BeginUpdate();
        foreach (var x in tt.NguoiXem)
        {
            co.Add(x.Ma);
            string dau = x.KhungDauMs is { } d ? $"{d:F0} ms ({x.NguonDau})" : "–";
            string[] gt = [x.Ten, x.Udp, x.Nhan, x.GoiRa.ToString(CultureInfo.InvariantCulture), $"{x.Kbps:F0}", dau];
            var it = bang.Items.Cast<ListViewItem>().FirstOrDefault(i => (int)i.Tag! == x.Ma);
            if (it == null) bang.Items.Add(new ListViewItem(gt) { Tag = x.Ma });
            else
                for (int c = 0; c < gt.Length; c++)
                    if (it.SubItems[c].Text != gt[c]) it.SubItems[c].Text = gt[c];
        }
        foreach (var it in bang.Items.Cast<ListViewItem>().Where(i => !co.Contains((int)i.Tag!)).ToList()) bang.Items.Remove(it);
        bang.EndUpdate();
        if (tt.Anh is { } a && (a.Ts, a.So) != soDaVe && Lop.Chay)
        {
            xemTruoc.Dat(GD.SangBitmap(a.Anh, a.W, a.H));
            lblDongHo.Text = $"Đồng hồ phương tiện: t={a.Ts / 1000.0,7:F3} s, khung #{a.So}";
            soDaVe = (a.Ts, a.So);
        }
        else if (tt.Anh == null && xemTruoc.CoHinh) xemTruoc.Dat(null, "Chưa có luồng");
    }
}
