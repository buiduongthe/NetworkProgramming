using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Cửa sổ máy phát trực tiếp. Lõi MayPhat (../MayPhat.cs) giữ kết nối, luồng âm thanh 20 ms và
// luồng video; cửa sổ chọn nguồn (camera thật qua ffmpeg / tệp video / mẫu thử testsrc / giả lập clapper; micro thật
// qua winmm / tiếng của tệp / giả lập), fps, GOP, bấm Phát / Dừng phát / Khung khóa ngay; cứ 100 ms đọc trạng thái
// để vẽ xem trước, thanh mức micro, thống kê và danh sách người xem. Thiết bị thật chỉ được mở khi bấm PHÁT.
sealed class FormPhat : Form
{
    readonly HangDoiGiaoDien ui = new();
    public MayPhat? Lop;
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 }, moiGiay = new() { Interval = 1000 };
    readonly System.Windows.Forms.Timer henThamSo = new() { Interval = 400 };   // hẹn giờ áp dụng FPS/GOP (chờ người dùng ngừng bấm)
    long? tsDaVe;
    string nhanTt = "chưa kết nối";
    public readonly List<(string Ma, string Ten)> DsCam = [], DsMic = [];
    public readonly TextBox TxtIp, TxtCong, TxtTen, TxtTep;
    public readonly Button BtnKetNoi, BtnPhat, BtnDung, BtnI, BtnChonTep, BtnTaoMau;
    public readonly ComboBox CboCam, CboMic;
    public readonly NumericUpDown NumFps, NumGop;
    public readonly Label LblTrangThai;
    readonly Label lblChiBao, lblDs;
    readonly Dictionary<string, Label> lblTk = [];
    readonly OAnh xemTruoc = new(2 * Chung.Rong, 2 * Chung.Cao);
    readonly ThanhMuc thanhMic;
    readonly ListBox dsXem;
    readonly NhatKy nhatKy;
    readonly ThongKeDinhKy dinhKy;

    public FormPhat(string ten = "P", string host = "127.0.0.1", int cong = 42652)
    {
        nhatKy = new NhatKy($"phat {ten}", 8, 20, 700, 90);
        dinhKy = new ThongKeDinhKy($"phat {ten}");
        Text = $"Máy phát trực tiếp – MR6.5 ({ten})";
        BackColor = GD.Nen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.Manual;
        ClientSize = new Size(724, 600);
        bool coFfmpeg = CameraFfmpeg.CoFfmpeg();

        var g = GD.Nhom("Kết nối server", 8, 4, 196, 176);
        TxtIp = GD.O(host, 72, 22, 112);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 72, 50, 60);
        TxtTen = GD.O(ten, 72, 78, 80);
        BtnKetNoi = GD.Nut("Kết nối", GD.XanhLa, 30, 112, 130);
        GD.Them(g, GD.Nhan("Server IP:", 8, 25), TxtIp, GD.Nhan("Port:", 8, 53), TxtCong, GD.Nhan("Tên:", 8, 81), TxtTen, BtnKetNoi);

        var g2 = GD.Nhom("Nguồn và tham số", 210, 4, 506, 176);
        if (coFfmpeg)
        {
            DsCam.Add((TepVideo.Tep, "Tệp video (chọn tệp bên dưới)"));
            DsCam.Add((CameraFfmpeg.MauThu, "Mẫu thử ffmpeg (testsrc)"));
        }
        DsCam.Add((CameraFfmpeg.GiaLap, "Camera giả lập (bảng clapper)"));
        CboCam = GD.Chon(76, 20, 260);
        CboCam.Items.AddRange(DsCam.Select(c => (object)c.Ten).ToArray());
        CboCam.SelectedIndex = DsCam.Count - 1;
        DsMic.AddRange(ThietBiAmThanh.LietKeMicro().Select(m => (m.ChiSo.ToString(CultureInfo.InvariantCulture), m.Ten)));
        if (coFfmpeg) DsMic.Add((TepVideo.Tep, "Tiếng của tệp video"));
        DsMic.Add(("gia_lap", "Micro giả lập (440 Hz + bíp mỗi giây)"));
        CboMic = GD.Chon(76, 48, 260);
        CboMic.Items.AddRange(DsMic.Select(c => (object)c.Ten).ToArray());
        // Mặc định: micro thật đầu tiên tìm thấy; máy không có micro -> micro giả lập (mục cuối)
        CboMic.SelectedIndex = DsMic[0].Ma is not (TepVideo.Tep or "gia_lap") ? 0 : DsMic.Count - 1;
        NumFps = GD.So(76, 76, 1, 30, Chung.FpsMacDinh);
        NumGop = GD.So(240, 76, 1, 300, Chung.GopMacDinh);
        TxtTep = GD.O("", 76, 104, 230);
        BtnChonTep = GD.Nut("Chọn tệp…", GD.Xam, 312, 102, 84, 24);
        BtnTaoMau = GD.Nut("Tạo tệp mẫu", GD.Xam, 400, 102, 96, 24);
        if (!coFfmpeg) TxtTep.Enabled = BtnChonTep.Enabled = BtnTaoMau.Enabled = false;
        BtnPhat = GD.Nut("PHÁT", GD.XanhLa, 8, 130, 90);
        BtnDung = GD.Nut("DỪNG PHÁT", GD.Do, 104, 130, 90);
        BtnI = GD.Nut("Khung khóa ngay", GD.XanhDuong, 200, 130, 130);
        BtnPhat.Enabled = BtnDung.Enabled = BtnI.Enabled = false;
        lblChiBao = GD.Nhan("Camera: tắt | Micro: tắt" + (coFfmpeg ? "" : " (không có ffmpeg)"), 8, 158, 490);
        lblChiBao.ForeColor = Color.FromArgb(0x40, 0x40, 0x40);
        GD.Them(g2, GD.Nhan("Camera:", 8, 23), CboCam, GD.Nhan("Micro:", 8, 51), CboMic, GD.Nhan("FPS:", 8, 79), NumFps,
                GD.Nhan("GOP (khung):", 160, 79), NumGop, GD.Nhan("Tệp video:", 8, 107), TxtTep, BtnChonTep, BtnTaoMau,
                BtnPhat, BtnDung, BtnI, lblChiBao);

        var g3 = GD.Nhom("Xem trước (khung vừa gửi)", 8, 184, 336, 290);
        xemTruoc.Location = new Point(8, 20);
        thanhMic = new ThanhMuc(84, 264, 236);
        GD.Them(g3, xemTruoc, GD.Nhan("Mức micro:", 8, 266), thanhMic);

        var g4 = GD.Nhom("Thống kê", 350, 184, 366, 290);
        int y = 18;
        foreach (var (k, t) in new[] { ("ip", "Khung I / P đã gửi"), ("tb", "Kích thước TB I / P"), ("pmin", "P nhỏ nhất / lớn nhất"),
                                       ("kb", "kbit/s tiếng / hình"), ("pli", "PLI (xin khung khóa) nhận") })
        {
            GD.Them(g4, GD.Nhan(t + ":", 8, y));
            lblTk[k] = GD.Nhan("–", 20, y + 16, 330, GD.Dam);
            lblTk[k].Height = 20;
            GD.Them(g4, lblTk[k]);
            y += 38;
        }
        lblDs = GD.Nhan("Người xem (0):", 8, y + 2);
        dsXem = new ListBox { Location = new Point(8, y + 20), Size = new Size(350, 62), Font = GD.Thuong };
        GD.Them(g4, lblDs, dsXem);

        var g5 = GD.Nhom("Nhật ký", 8, 478, 708, 116 - 4);
        nhatKy.Height = 84;
        GD.Them(g5, nhatKy);
        LblTrangThai = new Label { Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: chưa kết nối", BackColor = Color.White,
                                   BorderStyle = BorderStyle.Fixed3D, TextAlign = ContentAlignment.MiddleLeft };
        ClientSize = new Size(724, 620);
        Controls.AddRange([g, g2, g3, g4, g5, LblTrangThai]);

        // ---- Sự kiện
        Load += async (_, _) =>
        {
            var that = await Task.Run(CameraFfmpeg.LietKeCamera);    // ffmpeg -list_devices mất khoảng 1 s – KHÔNG bật camera
            int chon = CboCam.SelectedIndex;
            for (int i = 0; i < that.Count; i++)
            {
                DsCam.Insert(i, that[i]);
                CboCam.Items.Insert(i, that[i].Ten);
            }
            CboCam.SelectedIndex = chon + that.Count;
        };
        BtnKetNoi.Click += (_, _) => BamKetNoi();
        BtnPhat.Click += (_, _) => BatPhat();
        BtnDung.Click += (_, _) => DungPhat();
        BtnI.Click += (_, _) => Lop?.KhungKhoaNgay("nút Khung khóa ngay");
        NumFps.ValueChanged += (_, _) => ApThamSo();
        NumGop.ValueChanged += (_, _) => ApThamSo();
        CboCam.SelectedIndexChanged += (_, _) => DoiCamera();
        BtnChonTep.Click += (_, _) => ChonTep();
        BtnTaoMau.Click += (_, _) => TaoMau();
        nhip.Tick += (_, _) => CapNhat();
        moiGiay.Tick += (_, _) => Lop?.ThongKeGiay();                // cập nhật kbit/s của giây vừa qua
        henThamSo.Tick += (_, _) => { henThamSo.Stop(); ApThamSoThat(); };   // chỉ chạy một lần sau mỗi lần hẹn
        nhip.Start();
        moiGiay.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            moiGiay.Stop();
            henThamSo.Stop();
            ui.Dispose();
            Lop?.Dong();
        };
    }

    void Ghi(string t) => ui.DuaVao(() => nhatKy.Ghi(t));

    // ------------------------------------------------------------ kết nối
    void BamKetNoi()
    {
        if (Lop is { DaKetNoi: true })
        {
            Lop.Dong();
            DaNgat("đã ngắt kết nối");
            return;
        }
        string ten = TxtTen.Text.Trim(), host = TxtIp.Text.Trim();
        if (!int.TryParse(TxtCong.Text, out int cong))
        {
            nhanTt = "cổng không hợp lệ";
            return;
        }
        Lop?.Dong();
        var lop = Lop = new MayPhat(ten, Ghi);
        BtnKetNoi.Enabled = false;
        BtnKetNoi.Text = "Đang kết nối…";
        Task.Run(() =>                                                // connect có thể chờ tới 5 s
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
        BtnPhat.Enabled = true;
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = false;
    }

    void DaNgat(string text)
    {
        BtnKetNoi.Enabled = true;
        (BtnKetNoi.Text, BtnKetNoi.BackColor) = ("Kết nối", GD.XanhLa);
        nhanTt = text;
        BtnPhat.Enabled = BtnDung.Enabled = BtnI.Enabled = false;
        CboCam.Enabled = CboMic.Enabled = true;
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = true;
    }

    // ------------------------------------------------------------ phát
    void BatPhat()
    {
        var lop = Lop;
        if (lop is not { DaKetNoi: true }) return;
        var (maCam, tenCam) = DsCam[CboCam.SelectedIndex];
        int fps = (int)NumFps.Value, gop = (int)NumGop.Value;
        var (maMic, tenMic) = DsMic[CboMic.SelectedIndex];
        INguonHinh? cam = null;
        INguonMic? mic = null;
        if (maCam == TepVideo.Tep)                                    // tệp video: hình (và tiếng nếu chọn)
        {
            try
            {
                CameraTep ct;
                (ct, var mt) = TepVideo.MoTep(TxtTep.Text.Trim(), fps, layTieng: maMic == TepVideo.Tep);
                (cam, mic) = (ct, mt);
                nhatKy.Ghi($"Phát tệp {ct.Tep.Ten}" + (mt != null ? " (cả tiếng)" : ""));
            }
            catch (IOException e)
            {
                nhatKy.Ghi($"Không mở được tệp video ({e.Message}); dùng camera giả lập");
                cam = null;
            }
        }
        if (cam == null)
        {
            try { cam = CameraFfmpeg.MoCamera(maCam == TepVideo.Tep ? CameraFfmpeg.GiaLap : maCam, tenCam, lop.Ten, lop.Ma ?? 1, fps); }
            catch (Exception e) when (e is System.ComponentModel.Win32Exception or InvalidOperationException)
            {
                nhatKy.Ghi($"Không chạy được ffmpeg ({e.Message}) – dùng camera giả lập");
                cam = new CameraGiaLap(lop.Ten, lop.Ma ?? 1, fps);
            }
        }
        if (mic == null)
        {
            if (maMic == TepVideo.Tep) nhatKy.Ghi("Tiếng của tệp chỉ dùng được khi camera là tệp video; dùng micro giả lập");
            int? chiSo = maMic is "gia_lap" or TepVideo.Tep ? null : int.Parse(maMic, CultureInfo.InvariantCulture);
            (mic, var loi) = ThietBiAmThanh.MoMic(chiSo, tenMic);
            if (loi != "") nhatKy.Ghi(loi);
        }
        lop.BatDauPhat(cam, mic, fps, gop);
        nhatKy.Ghi($"Camera: {cam.MoTa}");
        nhatKy.Ghi($"Micro: {mic.MoTa}");
        lblChiBao.Text = $"Camera: {cam.MoTa} | Micro: {mic.MoTa}";
        lblChiBao.ForeColor = cam.That || mic.That ? Color.FromArgb(0x00, 0x60, 0x00) : Color.FromArgb(0x8A, 0x4B, 0x00);
        BtnPhat.Enabled = false;
        BtnDung.Enabled = BtnI.Enabled = true;
        CboCam.Enabled = CboMic.Enabled = false;
    }

    void DungPhat()
    {
        Lop?.DungPhat();
        lblChiBao.Text = "Camera: tắt | Micro: tắt";
        lblChiBao.ForeColor = Color.FromArgb(0x40, 0x40, 0x40);
        BtnPhat.Enabled = true;
        BtnDung.Enabled = BtnI.Enabled = false;
        CboCam.Enabled = CboMic.Enabled = true;
    }

    /// <summary>Giữ nút mũi tên của ô FPS/GOP sẽ đổi giá trị liên tục: chỉ áp dụng (và gửi thong_tin) khi giá trị
    /// đứng yên 400 ms – tránh gửi hàng chục thông điệp cho mỗi lần đổi.</summary>
    void ApThamSo()
    {
        henThamSo.Stop();       // đếm lại 400 ms từ lần đổi mới nhất
        henThamSo.Start();
    }

    void ApThamSoThat()
    {
        if (Lop is { DangPhat: true }) Lop.DatThamSo((int)NumFps.Value, (int)NumGop.Value);
    }

    /// <summary>Chọn camera "Tệp video" -> micro tự chuyển sang tiếng của tệp (đổi lại được); bỏ "Tệp video" mà micro
    /// còn là tiếng của tệp -> micro về thiết bị đầu tiên trong danh sách (micro thật nếu có).</summary>
    void DoiCamera()
    {
        if (CboCam.SelectedIndex < 0 || CboMic.SelectedIndex < 0) return;
        if (DsCam[CboCam.SelectedIndex].Ma == TepVideo.Tep)
        {
            int i = DsMic.FindIndex(m => m.Ma == TepVideo.Tep);
            if (i >= 0) CboMic.SelectedIndex = i;
        }
        else if (DsMic[CboMic.SelectedIndex].Ma == TepVideo.Tep) CboMic.SelectedIndex = 0;
    }

    void ChonTep()
    {
        using var hop = new OpenFileDialog
        {
            Title = "Chọn tệp video",
            Filter = "Tệp video|*.mp4;*.mkv;*.avi;*.mov;*.webm;*.flv;*.ts;*.m4v|Mọi tệp|*.*",
        };
        if (hop.ShowDialog(this) == DialogResult.OK) DatTep(hop.FileName);
    }

    /// <summary>Đặt đường dẫn tệp và chuyển camera (và micro) sang tệp video.</summary>
    public void DatTep(string duong)
    {
        TxtTep.Text = duong;
        int i = DsCam.FindIndex(c => c.Ma == TepVideo.Tep);
        if (i >= 0) CboCam.SelectedIndex = i;
        DoiCamera();
    }

    /// <summary>Tạo tệp mẫu 10 s (chớp trắng + bíp mỗi giây) cạnh chương trình (ở nền), rồi chọn nó.</summary>
    void TaoMau()
    {
        string duong = Path.Combine(AppContext.BaseDirectory, "out", "tep_mau.mp4");
        Directory.CreateDirectory(Path.GetDirectoryName(duong)!);
        BtnTaoMau.Enabled = false;
        BtnTaoMau.Text = "Đang tạo…";
        Task.Run(() =>
        {
            string? loi = null;
            try { TepVideo.TaoTepMau(duong, 10); }
            catch (IOException e) { loi = e.Message; }
            ui.DuaVao(() =>
            {
                BtnTaoMau.Enabled = true;
                BtnTaoMau.Text = "Tạo tệp mẫu";
                if (loi == null)
                {
                    nhatKy.Ghi($"Đã tạo tệp mẫu {duong}");
                    DatTep(duong);
                }
                else nhatKy.Ghi($"Không tạo được tệp mẫu: {loi}");
            });
        });
    }

    // ------------------------------------------------------------ vẽ lại
    void CapNhat()
    {
        if (Lop is not { } lop) return;
        var tt = lop.TrangThai();
        thanhMic.Dat(tt.MucMic);
        if (tt.DangPhat)
            dinhKy.InNeuDenHan(() => $"I {tt.SoI} P {tt.SoP} | hình {tt.KbpsHinh:F0} tiếng {tt.KbpsAm:F0} kbit/s" +
                                     $" | mic {(double.IsInfinity(tt.MucMic) ? "-inf" : tt.MucMic.ToString("F0", CultureInfo.InvariantCulture))} dBFS" +
                                     $" | PLI {tt.PliNhan} | {tt.NguoiXem.Count} người xem");
        if (tt.XemTruoc is { } x && x.Ts != tsDaVe && tt.DangPhat)
        {
            xemTruoc.Dat(GD.SangBitmap(x.Anh, x.W, x.H));
            tsDaVe = x.Ts;
        }
        else if (!tt.DangPhat && xemTruoc.CoHinh) xemTruoc.Dat(null);
        lblTk["ip"].Text = $"{tt.SoI} / {tt.SoP}";
        lblTk["tb"].Text = $"{tt.TbI:F0} B / {tt.TbP:F0} B";
        lblTk["pmin"].Text = $"{tt.PMin} B / {tt.PMax} B";
        lblTk["kb"].Text = $"{tt.KbpsAm:F0} / {tt.KbpsHinh:F0}";
        lblTk["pli"].Text = tt.PliNhan.ToString(CultureInfo.InvariantCulture);
        if (!dsXem.Items.Cast<string>().SequenceEqual(tt.NguoiXem))
        {
            dsXem.Items.Clear();
            dsXem.Items.AddRange(tt.NguoiXem.Cast<object>().ToArray());
        }
        lblDs.Text = $"Người xem ({tt.NguoiXem.Count}):";
        if (nhanTt.StartsWith("đã kết nối", StringComparison.Ordinal) && !tt.DaKetNoi) DaNgat("mất kết nối server");
        string trang = tt.DangPhat ? $"ĐANG PHÁT {tt.Fps} fps, GOP {tt.Gop} – {tt.NguoiXem.Count} người xem" : nhanTt;
        LblTrangThai.Text = $"Trạng thái: {trang}";
    }
}
