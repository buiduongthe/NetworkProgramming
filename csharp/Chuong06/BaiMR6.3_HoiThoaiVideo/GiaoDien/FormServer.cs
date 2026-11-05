using System.Collections.Concurrent;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_3;

// Cửa sổ "Camera Server": lưới 4×4 ô (mỗi client một ô), khung phải là danh sách client, điều khiển và nhật ký.
// Phòng (PhongVideo) chạy trên các luồng nền; luồng giao diện chỉ:
//   - mỗi 100 ms chụp nhanh trạng thái + ô hình (Khach.O, 160×120 RGB) trong khóa phòng rồi vẽ lại các ô;
//   - lấy nhật ký từ hàng đợi ConcurrentQueue (luồng nền chỉ Enqueue, không chạm điều khiển);
//   - gọi phương thức của phòng khi người dùng bấm nút (các phương thức này khóa phòng và gửi TCP rất ngắn).
class FormServer : Form
{
    const int SoO = 16;                                        // lưới 4 × 4
    readonly ConcurrentQueue<string> nhatKy = new();
    PhongVideo? phong;
    ushort? maChon;                                            // client đang chọn trong danh sách
    readonly OHinh[] o = new OHinh[SoO];
    readonly List<ushort> maTrongDs = [];
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 };
    readonly System.Windows.Forms.Timer treFps, treFpsChung;
    bool dangGan;                                              // đang đặt giá trị điều khiển từ trạng thái phòng

    public readonly TextBox TxtIp, TxtTcp, TxtUdp;
    public readonly Button BtnStart, BtnStop, BtnTatCam, BtnBatCam;
    public readonly ListBox LstKhach;
    public readonly TrackBar TrkFps, TrkFpsChung;
    public readonly ComboBox CboChia;
    readonly Label lblFps, lblFpsChung, lblChon, lblTrangThai;
    readonly TextBox txtNhatKy;

    public FormServer(string host, int congTcp, int congUdp)
    {
        Text = "Camera Server";
        ClientSize = new Size(1260, 780);
        StartPosition = FormStartPosition.Manual;
        Location = new Point(10, 10);

        // ---- Lưới 4×4
        var luoi = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 4, RowCount = 4, Padding = new Padding(6) };
        for (int i = 0; i < 4; i++)
        {
            luoi.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 25));
            luoi.RowStyles.Add(new RowStyle(SizeType.Percent, 25));
        }
        for (int i = 0; i < SoO; i++)
        {
            o[i] = new OHinh { Dock = DockStyle.Fill };
            luoi.Controls.Add(o[i], i % 4, i / 4);
        }

        // ---- Khung phải
        var phai = new Panel { Dock = DockStyle.Right, Width = 300, Padding = new Padding(6) };
        int y = 8;
        phai.Controls.Add(GD.Nhan("Danh sách client:", 8, y));
        LstKhach = new ListBox { Location = new Point(8, y += 20), Size = new Size(284, 110) };
        for (int i = 0; i < SoO; i++)
        {
            int chiSo = i;
            o[i].Click += (_, _) => { if (chiSo < maTrongDs.Count) LstKhach.SelectedIndex = chiSo; };   // bấm ô = chọn client
        }
        lblChon = GD.Nhan("Client đang chọn: (chưa chọn)", 8, y += 116, 284);
        BtnTatCam = GD.Nut("Tắt camera", 8, y += 22, 138);
        BtnBatCam = GD.Nut("Bật camera", 154, y, 138);
        lblFps = GD.Nhan("fps gửi tối đa: 10", 8, y += 38, 140);
        TrkFps = GD.Truot(150, y - 4, 142, 1, 15, 10);
        CboChia = GD.Chon(150, y += 30, 142, "1/1", "1/2", "1/4");
        phai.Controls.Add(GD.Nhan("Độ phân giải gửi:", 8, y + 3));
        lblFpsChung = GD.Nhan("fps chung: 10", 8, y += 40, 140);
        TrkFpsChung = GD.Truot(150, y - 4, 142, 1, 15, 10);
        phai.Controls.Add(GD.Nhan("Nhật ký:", 8, y += 32));
        txtNhatKy = new TextBox { Location = new Point(8, y += 18), Size = new Size(284, 300), Multiline = true, ReadOnly = true,
                                  ScrollBars = ScrollBars.Vertical, BackColor = Color.White, Font = new Font(FontFamily.GenericMonospace, 8f) };
        y += 306;
        phai.Controls.Add(GD.Nhan("IP:", 8, y + 3));
        TxtIp = GD.O(host, 40, y, 100);
        phai.Controls.Add(GD.Nhan("TCP:", 146, y + 3));
        TxtTcp = GD.O(congTcp.ToString(CultureInfo.InvariantCulture), 180, y, 50);
        TxtUdp = GD.O(congUdp.ToString(CultureInfo.InvariantCulture), 236, y, 56);
        BtnStart = GD.Nut("START SERVER", 8, y += 30, 284, 30);
        BtnStop = GD.Nut("STOP SERVER", 8, y += 34, 284, 30);
        BtnStop.Enabled = false;
        phai.Controls.AddRange([LstKhach, lblChon, BtnTatCam, BtnBatCam, lblFps, TrkFps, CboChia, lblFpsChung, TrkFpsChung, txtNhatKy,
                                TxtIp, TxtTcp, TxtUdp, BtnStart, BtnStop]);

        lblTrangThai = new Label { Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: server chưa chạy", TextAlign = ContentAlignment.MiddleLeft,
                                   BorderStyle = BorderStyle.Fixed3D };
        Controls.AddRange([luoi, phai, lblTrangThai]);                // thêm sau = xếp Dock trước

        // ---- Sự kiện
        treFps = GD.TreLenh(300, () => VoiChon((p, k) => p.DatFpsToiDa([k], TrkFps.Value, k.Ten)));
        treFpsChung = GD.TreLenh(300, () => phong?.DatFpsChung(TrkFpsChung.Value));
        BtnStart.Click += (_, _) => BatDauServer();
        BtnStop.Click += (_, _) => DungServer();
        BtnTatCam.Click += (_, _) => VoiChon((p, k) => p.DatCamera([k], true, k.Ten));
        BtnBatCam.Click += (_, _) => VoiChon((p, k) => p.DatCamera([k], false, k.Ten));
        LstKhach.SelectedIndexChanged += (_, _) =>
        {
            maChon = LstKhach.SelectedIndex >= 0 && LstKhach.SelectedIndex < maTrongDs.Count ? maTrongDs[LstKhach.SelectedIndex] : null;
        };
        TrkFps.ValueChanged += (_, _) =>
        {
            lblFps.Text = $"fps gửi tối đa: {TrkFps.Value}";
            if (!dangGan) GD.ChayLai(treFps);
        };
        CboChia.SelectedIndexChanged += (_, _) =>
        {
            if (!dangGan) VoiChon((p, k) => p.DatDoPhanGiai([k], 1 << CboChia.SelectedIndex, k.Ten));
        };
        TrkFpsChung.ValueChanged += (_, _) =>
        {
            lblFpsChung.Text = $"fps chung: {TrkFpsChung.Value}";
            GD.ChayLai(treFpsChung);
        };
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            DungServer();
        };
    }

    public void Log(string s)
    {
        nhatKy.Enqueue($"[{DateTime.Now:HH:mm:ss}] {s}");
        Console.WriteLine($"[server] {s}");
    }

    public PhongVideo? Phong => phong;

    /// <summary>Thực hiện thao tác với client đang chọn (nếu có).</summary>
    void VoiChon(Action<PhongVideo, Khach> viec)
    {
        var p = phong;
        if (p == null || maChon is not ushort ma) return;
        Khach? k;
        lock (p.Khoa) p.DsKhach.TryGetValue(ma, out k);
        if (k != null) viec(p, k);
    }

    /// <summary>Chọn client theo tên (dùng cho kịch bản demo).</summary>
    public void ChonKhach(string ten)
    {
        var p = phong;
        if (p == null) return;
        lock (p.Khoa)
            for (int i = 0; i < maTrongDs.Count; i++)
                if (p.DsKhach.TryGetValue(maTrongDs[i], out var k) && k.Ten == ten) LstKhach.SelectedIndex = i;
        CapNhat();
    }

    void BatDauServer()
    {
        try
        {
            var p = new PhongVideo(Log);
            p.BatDau(TxtIp.Text.Trim(), int.Parse(TxtTcp.Text), int.Parse(TxtUdp.Text));
            phong = p;
            if (TrkFpsChung.Value != p.FpsChung) p.DatFpsChung(TrkFpsChung.Value);
            lblTrangThai.Text = $"Trạng thái: server đang chạy tại {TxtIp.Text.Trim()} (TCP {TxtTcp.Text}, UDP {TxtUdp.Text})";
            BtnStart.Enabled = false;
            BtnStop.Enabled = true;
            TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = false;
        }
        catch (Exception e) when (e is FormatException || e is System.Net.Sockets.SocketException || e is OverflowException || e is ArgumentException)
        {
            lblTrangThai.Text = $"Trạng thái: không khởi động được – {e.Message}";
        }
    }

    void DungServer()
    {
        if (phong == null) return;
        phong.Dung();
        phong.InTongKet();
        phong = null;
        lblTrangThai.Text = "Trạng thái: server đã dừng";
        BtnStart.Enabled = true;
        BtnStop.Enabled = false;
        TxtIp.Enabled = TxtTcp.Enabled = TxtUdp.Enabled = true;
        CapNhat();
    }

    record Anh(ushort Ma, string Ten, string LyDo, bool CamMo, int FpsToiDa, int Chia, double FpsVao, double KbpsVao, double FpsRa, byte[]? O);

    /// <summary>Bộ hẹn giờ 100 ms trên luồng giao diện.</summary>
    void CapNhat()
    {
        var moi = new List<string>();
        while (nhatKy.TryDequeue(out var s)) moi.Add(s);
        if (moi.Count > 0)
        {
            if (txtNhatKy.TextLength > 60000) txtNhatKy.Clear();
            txtNhatKy.AppendText(string.Join(Environment.NewLine, moi) + Environment.NewLine);
        }
        var ds = new List<Anh>();
        if (phong != null)
            lock (phong.Khoa)                                   // chụp nhanh trong khóa: chỉ sao chép, không vẽ
                ds = phong.DsKhach.Values.Take(SoO).Select(k => new Anh(k.Ma, k.Ten, k.LyDo(), k.CamMo, k.FpsToiDa, k.Chia,
                         k.FpsVao, k.KbpsVao, k.FpsRa, k.O == null ? null : (byte[])k.O.Clone())).ToList();
        // Danh sách client (chỉ dựng lại khi thành phần thay đổi để không mất lựa chọn)
        var maMoi = ds.Select(a => a.Ma).ToList();
        if (!maMoi.SequenceEqual(maTrongDs))
        {
            maTrongDs.Clear();
            maTrongDs.AddRange(maMoi);
            LstKhach.BeginUpdate();
            LstKhach.Items.Clear();
            foreach (var a in ds) LstKhach.Items.Add($"#{a.Ma} {a.Ten}");
            int vt = maChon is ushort m ? maTrongDs.IndexOf(m) : -1;
            LstKhach.SelectedIndex = vt;
            LstKhach.EndUpdate();
            if (vt < 0) maChon = null;
        }
        for (int i = 0; i < ds.Count; i++)
        {
            string dong = $"#{ds[i].Ma} {ds[i].Ten} – {ds[i].LyDo}, vào {ds[i].FpsVao:F0} fps {ds[i].KbpsVao:F0} kbit/s";
            if ((string)LstKhach.Items[i] != dong) LstKhach.Items[i] = dong;
        }
        for (int i = 0; i < SoO; i++)
        {
            if (i >= ds.Count)
            {
                o[i].Dat(null, "", "", false);
                continue;
            }
            var a = ds[i];
            var b = a.O != null ? GD.SangBitmap(a.O, Chung.Rong, Chung.Cao) : null;
            o[i].Dat(b, $"#{a.Ma} {a.Ten}", $"{a.FpsVao:F0} fps nhận, {a.KbpsVao:F0} kbit/s, giới hạn {a.FpsToiDa} fps 1/{a.Chia}",
                     !a.CamMo, a.Ma == maChon);
        }
        // Điều khiển của client đang chọn hiển thị đúng trạng thái phòng
        var chon = ds.FirstOrDefault(a => a.Ma == maChon);
        lblChon.Text = chon != null ? $"Client đang chọn: #{chon.Ma} {chon.Ten} ({chon.LyDo})" : "Client đang chọn: (chưa chọn)";
        BtnTatCam.Enabled = BtnBatCam.Enabled = TrkFps.Enabled = CboChia.Enabled = chon != null;
        if (chon != null && !treFps.Enabled)
        {
            dangGan = true;
            TrkFps.Value = Math.Clamp(chon.FpsToiDa, TrkFps.Minimum, TrkFps.Maximum);
            CboChia.SelectedIndex = chon.Chia == 4 ? 2 : chon.Chia == 2 ? 1 : 0;
            dangGan = false;
        }
        if (phong != null)
            lblTrangThai.Text = $"Trạng thái: server đang chạy tại {TxtIp.Text.Trim()}:{TxtTcp.Text} – {ds.Count} client, fps chung {phong.FpsChung}, " +
                                $"gửi lưới {ds.Sum(a => a.FpsRa):F0} khung/s";
    }

    /// <summary>Số liệu cho kịch bản demo.</summary>
    public List<string> SoLieu()
    {
        var kq = new List<string>();
        var p = phong;
        if (p == null) return kq;
        lock (p.Khoa)
            foreach (var k in p.DsKhach.Values)
                kq.Add($"server thấy {k.Ten}: camera {k.LyDo()}, giới hạn {k.FpsToiDa} fps 1/{k.Chia}, vào {k.FpsVao:F0} fps {k.KbpsVao:F0} kbit/s, " +
                       $"ra (lưới) {k.FpsRa:F0} fps {k.KbpsRa:F0} kbit/s");
        return kq;
    }
}
