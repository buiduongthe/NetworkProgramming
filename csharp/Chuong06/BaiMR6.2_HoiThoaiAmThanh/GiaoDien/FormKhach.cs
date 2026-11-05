using System.Collections.Concurrent;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_2;

// Cửa sổ client âm thanh. Phần mạng/âm thanh nằm trong KetNoiAmThanh (luồng nền); cửa sổ chỉ:
//   - đặt cài đặt khi người dùng bấm/kéo (Start/Stop Mic, Start/Stop Audio, âm lượng);
//   - Connect chạy bằng await Task.Run(...) – kết nối TCP có thể mất vài giây, không được làm trên luồng giao diện;
//   - mỗi 100 ms đọc trạng thái (thanh đo mức, Server Vol, trạng thái mic) và nhật ký từ hàng đợi.
class FormKhach : Form
{
    readonly ConcurrentQueue<string> nhatKy = new();
    public readonly KetNoiAmThanh May;
    readonly double tanSoGiaLap;
    readonly List<string> tenMicThat = WinMM.DanhSachMicro();
    PhatAmWinMM? loa;
    bool dangKetNoi;
    readonly string tag;
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 };
    readonly System.Windows.Forms.Timer treNghe;

    public readonly TextBox TxtIp, TxtCong, TxtTen;
    public readonly Button BtnConnect, BtnDisconnect, BtnAudioStart, BtnAudioStop, BtnMicStart, BtnMicStop;
    public readonly TrackBar TrkNghe, TrkMic;
    public readonly ComboBox CboMic;
    readonly Label lblNghe, lblServerVol, lblLoa, lblMicVol, lblMicTrangThai, lblLoaiMic, lblThongKe, lblTrangThai;
    public readonly ThanhMucAm MucNhan = new() { Width = 400 }, MucMic = new() { Width = 400 };
    readonly TextBox txtNhatKy;

    public FormKhach(string host, int cong, string ten, double tanSoGiaLap = 440)
    {
        this.tanSoGiaLap = tanSoGiaLap;
        tag = $"[khach {ten}]";
        May = new KetNoiAmThanh(Log);
        Text = $"Client âm thanh – {ten}";
        ClientSize = new Size(460, 640);
        StartPosition = FormStartPosition.Manual;
        BackColor = GD.NenXanh;

        var gKetNoi = GD.Nhom("Kết nối", 436, 120);
        gKetNoi.Location = new Point(12, 8);
        TxtIp = GD.O(host, 60, 24, 140);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 260, 24, 60);
        TxtTen = GD.O(ten, 60, 54, 140);
        BtnConnect = GD.Nut("Connect", GD.NutXanhLa, 60, 82, 120, 30);
        BtnDisconnect = GD.Nut("Disconnect", GD.NutDo, 190, 82, 120, 30);
        BtnDisconnect.Enabled = false;
        gKetNoi.Controls.AddRange([GD.Nhan("IP:", 12, 27), TxtIp, GD.Nhan("Cổng:", 214, 27), TxtCong, GD.Nhan("Tên:", 12, 57), TxtTen,
                                   BtnConnect, BtnDisconnect]);

        var gNhan = GD.Nhom("Nhận âm thanh", 436, 170);
        gNhan.Location = new Point(12, 136);
        lblNghe = GD.Nhan("Âm lượng nghe: 100%", 12, 28, 140);
        TrkNghe = GD.Truot(160, 24, 260);
        BtnAudioStart = GD.Nut("Start Audio", GD.NutXanhLa, 12, 60, 120, 32);
        BtnAudioStop = GD.Nut("Stop Audio", GD.NutDo, 140, 60, 120, 32);
        BtnAudioStop.Enabled = false;
        lblServerVol = GD.Nhan("Server Vol: --", 280, 68, 140);
        MucNhan.Location = new Point(12, 106);
        lblLoa = GD.Nhan(WinMM.CoLoa() ? "Loa: tắt (chỉ đo mức)" : "Loa: không có thiết bị – chỉ đo mức", 12, 134, 410);
        gNhan.Controls.AddRange([lblNghe, TrkNghe, BtnAudioStart, BtnAudioStop, lblServerVol, MucNhan, lblLoa]);

        var gMic = GD.Nhom("Micro", 436, 200);
        gMic.Location = new Point(12, 314);
        CboMic = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(12, 24), Width = 410 };
        CboMic.Items.Add($"Giả lập (sin {tanSoGiaLap.ToString(CultureInfo.InvariantCulture)} Hz)");
        for (int i = 0; i < tenMicThat.Count; i++) CboMic.Items.Add($"{i}: {tenMicThat[i]}");
        CboMic.SelectedIndex = 0;
        BtnMicStart = GD.Nut("Start Mic", GD.NutXanhLa, 12, 56, 120, 32);
        BtnMicStop = GD.Nut("Stop Mic", GD.NutDo, 140, 56, 120, 32);
        BtnMicStop.Enabled = false;
        lblLoaiMic = GD.Nhan("Mic: tắt", 280, 64, 140);
        lblMicVol = GD.Nhan("Mic Volume: 100%", 12, 100, 140);
        TrkMic = GD.Truot(160, 96, 260);
        MucMic.Location = new Point(12, 132);
        lblMicTrangThai = GD.Nhan("Server: chưa kết nối", 12, 162, 410);
        gMic.Controls.AddRange([CboMic, BtnMicStart, BtnMicStop, lblLoaiMic, lblMicVol, TrkMic, MucMic, lblMicTrangThai]);
        foreach (var g in new[] { gKetNoi, gNhan, gMic }) GD.PhongThuong(g);

        lblThongKe = GD.Nhan("", 14, 520, 430);
        lblTrangThai = GD.Nhan("Chưa kết nối", 14, 542, 430);
        lblTrangThai.BackColor = Color.White;
        lblTrangThai.Font = new Font(Font, FontStyle.Bold);
        txtNhatKy = new TextBox { Location = new Point(12, 566), Size = new Size(436, 64), Multiline = true, ReadOnly = true,
                                  ScrollBars = ScrollBars.Vertical, BackColor = Color.White, Font = new Font(FontFamily.GenericMonospace, 8f) };
        Controls.AddRange([gKetNoi, gNhan, gMic, lblThongKe, lblTrangThai, txtNhatKy]);

        // ---- Sự kiện
        treNghe = GD.TreLenh(300, () => May.GuiAmLuongNghe(TrkNghe.Value / 100.0));
        BtnConnect.Click += async (_, _) => await KetNoiAsync();
        BtnDisconnect.Click += (_, _) => May.NgatKetNoi();
        BtnAudioStart.Click += (_, _) => BatLoa();
        BtnAudioStop.Click += (_, _) => TatLoa();
        BtnMicStart.Click += (_, _) => BatMic();
        BtnMicStop.Click += (_, _) =>
        {
            May.TatMic();
            BtnMicStart.Enabled = true;
            BtnMicStop.Enabled = false;
            lblLoaiMic.Text = "Mic: tắt";
            lblLoaiMic.ForeColor = Color.Black;
        };
        TrkNghe.ValueChanged += (_, _) =>
        {
            lblNghe.Text = $"Âm lượng nghe: {TrkNghe.Value}%";
            lock (May.Khoa) May.AmLuongNghe = TrkNghe.Value / 100.0;   // áp dụng ngay tại client
            GD.ChayLai(treNghe);                                         // báo server khi dừng kéo
        };
        TrkMic.ValueChanged += (_, _) =>
        {
            lblMicVol.Text = $"Mic Volume: {TrkMic.Value}%";
            lock (May.Khoa) May.AmLuongMic = TrkMic.Value / 100.0;
        };
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            TatLoa();
            May.Dispose();
        };
    }

    public void Log(string s)
    {
        nhatKy.Enqueue($"[{DateTime.Now:HH:mm:ss}] {s}");
        Console.WriteLine($"{tag} {s}");
    }

    public async Task KetNoiAsync()
    {
        string host = TxtIp.Text.Trim(), ten = TxtTen.Text.Trim();
        if (!int.TryParse(TxtCong.Text, out int cong) || ten == "")
        {
            lblTrangThai.Text = "Cổng hoặc tên không hợp lệ";
            lblTrangThai.ForeColor = Color.Red;
            return;
        }
        dangKetNoi = true;
        lblTrangThai.Text = $"Đang kết nối {host}:{cong}…";
        lblTrangThai.ForeColor = Color.DarkBlue;
        try
        {
            await Task.Run(() => May.KetNoi(host, cong, ten));          // chặn tối đa vài giây – trên thread pool
            Text = $"Client âm thanh – {ten}";
            dangKetNoi = false;
        }
        catch (Exception e)
        {
            Log($"Kết nối lỗi: {e.GetBaseException().Message}");
            lblTrangThai.Text = $"Không kết nối được: {e.GetBaseException().Message}";
            lblTrangThai.ForeColor = Color.Red;
            dangKetNoi = false;
        }
    }

    void BatMic()
    {
        INguonMic m;
        int chon = CboMic.SelectedIndex;
        try { m = chon <= 0 ? new MicGiaLap(tanSoGiaLap) : new MicThat(chon - 1, tenMicThat[chon - 1]); }
        catch (Exception e) when (e is InvalidOperationException || e is DllNotFoundException || e is EntryPointNotFoundException)
        {
            Log($"Không mở được micro thật ({e.Message}) – chuyển sang giả lập");
            m = new MicGiaLap(tanSoGiaLap);
            CboMic.SelectedIndex = 0;
        }
        May.BatMic(m);
        BtnMicStart.Enabled = false;
        BtnMicStop.Enabled = true;
        lblLoaiMic.Text = m.That ? "Mic: THẬT" : "Mic: giả lập";
        lblLoaiMic.ForeColor = m.That ? Color.DarkRed : Color.DarkBlue;
    }

    /// <summary>Start Audio: mở loa thật; không có loa thì vẫn nhận và đo mức.</summary>
    void BatLoa()
    {
        try
        {
            loa = new PhatAmWinMM();
            lock (May.Khoa) May.PhatRaLoa = loa.Phat;
            lblLoa.Text = "Loa: đang phát";
        }
        catch (Exception e) when (e is InvalidOperationException || e is DllNotFoundException || e is EntryPointNotFoundException)
        {
            loa = null;
            lblLoa.Text = $"Loa: không mở được ({e.Message}) – chỉ đo mức";
        }
        BtnAudioStart.Enabled = false;
        BtnAudioStop.Enabled = true;
    }

    void TatLoa()
    {
        lock (May.Khoa) May.PhatRaLoa = null;
        loa?.Dispose();
        loa = null;
        lblLoa.Text = "Loa: tắt (chỉ đo mức)";
        BtnAudioStart.Enabled = true;
        BtnAudioStop.Enabled = false;
    }

    void CapNhat()
    {
        var moi = new List<string>();
        while (nhatKy.TryDequeue(out var s)) moi.Add(s);
        if (moi.Count > 0)
        {
            if (txtNhatKy.TextLength > 30000) txtNhatKy.Clear();
            txtNhatKy.AppendText(string.Join(Environment.NewLine, moi) + Environment.NewLine);
        }
        MucNhan.DatMuc(May.MucNhan.Lay());
        MucMic.DatMuc(May.MucMic.Lay());
        bool ketNoi;
        lock (May.Khoa)
        {
            ketNoi = May.DaKetNoi;
            lblServerVol.Text = May.ServerVol is double v ? $"Server Vol: {GD.PhanTram(v)}" : "Server Vol: --";
            lblMicTrangThai.Text = ketNoi ? $"Server: mic {May.LyDoMic}" : "Server: chưa kết nối";
            lblMicTrangThai.ForeColor = May.MicMo ? Color.DarkGreen : Color.DarkRed;
            if (!dangKetNoi)
            {
                lblTrangThai.Text = May.TrangThai;
                lblTrangThai.ForeColor = May.TrangThaiLoi ? Color.Red : ketNoi ? Color.Green : Color.Black;
            }
            lblThongKe.Text = ketNoi ? $"Mã {May.Ma}: gửi {May.GoiGui} gói, nhận {May.GoiNhan}, mất {May.MatNhan}, jitter {May.JitterMs:F1} ms" : "";
        }
        BtnConnect.Enabled = !ketNoi && !dangKetNoi;
        BtnDisconnect.Enabled = ketNoi;
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = !ketNoi;
    }
}
