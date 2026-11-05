using System.Collections.Concurrent;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR6_3;

// Cửa sổ "Camera Client". Mạng và camera chạy trong KetNoiVideo/CameraFfmpeg (luồng nền). Cửa sổ:
//   - lúc mở: liệt kê camera bằng await Task.Run(...) (ffmpeg -list_devices mất khoảng 1 s) – KHÔNG mở camera;
//   - START mới mở camera (thật hoặc giả lập) và xin server cho gửi; CANCEL tắt ngay;
//   - Kết nối bằng await Task.Run(...); mỗi 100 ms vẽ ảnh xem trước, lưới nhận và số liệu.
class FormKhach : Form
{
    readonly ConcurrentQueue<string> nhatKy = new();
    public readonly KetNoiVideo May;
    readonly string tag;
    List<string> camThat = [];
    bool dangKetNoi;
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 100 };

    public readonly TextBox TxtIp, TxtCong, TxtTen;
    public readonly Button BtnKetNoi, BtnNgat, BtnStart, BtnCancel;
    public readonly ComboBox CboCam, CboNhanChia;
    public readonly NumericUpDown NumNhanFps;
    readonly Label lblLoaiCam, lblNhan, lblTrangThai;
    public readonly OHinh XemTruoc = new(), Luoi = new();

    public FormKhach(string host, int cong, string ten)
    {
        tag = $"[khach {ten}]";
        May = new KetNoiVideo(Log);
        Text = $"Camera Client – {ten}";
        ClientSize = new Size(760, 500);
        StartPosition = FormStartPosition.Manual;

        var gKetNoi = new GroupBox { Text = "Kết nối server", Location = new Point(8, 4), Size = new Size(360, 110) };
        TxtIp = GD.O(host, 70, 22, 120);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 240, 22, 60);
        TxtTen = GD.O(ten, 70, 50, 120);
        BtnKetNoi = GD.Nut("Kết nối", 70, 76, 110, 28);
        BtnNgat = GD.Nut("Ngắt kết nối", 190, 76, 110, 28);
        BtnNgat.Enabled = false;
        gKetNoi.Controls.AddRange([GD.Nhan("Server IP:", 8, 25), TxtIp, GD.Nhan("Port:", 200, 25), TxtCong, GD.Nhan("Tên:", 8, 53), TxtTen,
                                   BtnKetNoi, BtnNgat]);

        var gCam = new GroupBox { Text = "Camera", Location = new Point(376, 4), Size = new Size(376, 110) };
        CboCam = GD.Chon(110, 22, 256, "Camera giả lập");
        BtnStart = GD.Nut("START", 110, 50, 120, 28);
        BtnCancel = GD.Nut("CANCEL", 240, 50, 120, 28);
        BtnCancel.Enabled = false;
        lblLoaiCam = GD.Nhan("Camera: tắt (đang tìm camera…)", 8, 86, 360);
        gCam.Controls.AddRange([GD.Nhan("Chọn camera:", 8, 25), CboCam, BtnStart, BtnCancel, lblLoaiCam]);

        XemTruoc.Location = new Point(8, 122);
        XemTruoc.Size = new Size(320, 240);
        Luoi.Location = new Point(336, 122);
        Luoi.Size = new Size(416, 312);
        var lblXem = GD.Nhan("Camera của tôi (xem trước)", 8, 366);
        NumNhanFps = new NumericUpDown { Minimum = 1, Maximum = 30, Value = 10, Location = new Point(400, 440), Width = 50 };
        CboNhanChia = GD.Chon(580, 440, 60, "1/1", "1/2");
        lblNhan = GD.Nhan("", 8, 390, 320);
        lblTrangThai = new Label { Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: chưa kết nối", TextAlign = ContentAlignment.MiddleLeft,
                                   BorderStyle = BorderStyle.Fixed3D };
        Controls.AddRange([gKetNoi, gCam, XemTruoc, Luoi, lblXem, GD.Nhan("Lưới nhận từ server – fps nhận:", 210, 443), NumNhanFps,
                           GD.Nhan("Độ phân giải nhận:", 470, 443), CboNhanChia, lblNhan, lblTrangThai]);

        // ---- Sự kiện
        Load += async (_, _) =>
        {
            camThat = await Task.Run(CameraFfmpeg.DanhSach);        // chạy ffmpeg trên thread pool
            foreach (var c in camThat) CboCam.Items.Add(c);
            lblLoaiCam.Text = camThat.Count > 0 ? $"Camera: tắt – tìm thấy {camThat.Count} camera thật" : "Camera: tắt – không có ffmpeg/camera thật";
        };
        BtnKetNoi.Click += async (_, _) => await KetNoiAsync();
        BtnNgat.Click += (_, _) => May.NgatKetNoi();
        BtnStart.Click += (_, _) => BatCamera();
        BtnCancel.Click += (_, _) =>
        {
            May.TatCamera();
            BtnStart.Enabled = true;
            BtnCancel.Enabled = false;
            lblLoaiCam.Text = "Camera: tắt";
        };
        NumNhanFps.ValueChanged += (_, _) => May.DatNhan((int)NumNhanFps.Value, null);
        CboNhanChia.SelectedIndexChanged += (_, _) => May.DatNhan(null, CboNhanChia.SelectedIndex + 1);
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            May.Dispose();
        };
    }

    public void Log(string s)
    {
        nhatKy.Enqueue(s);
        Console.WriteLine($"{tag} {s}");
    }

    public async Task KetNoiAsync()
    {
        string host = TxtIp.Text.Trim(), ten = TxtTen.Text.Trim();
        if (!int.TryParse(TxtCong.Text, out int cong) || ten == "") return;
        dangKetNoi = true;
        lblTrangThai.Text = $"Trạng thái: đang kết nối {host}:{cong}…";
        try
        {
            await Task.Run(() => May.KetNoi(host, cong, ten));
            Text = $"Camera Client – {ten}";
            if ((int)NumNhanFps.Value != 10 || CboNhanChia.SelectedIndex != 0)
                May.DatNhan((int)NumNhanFps.Value, CboNhanChia.SelectedIndex + 1);
        }
        catch (Exception e) { Log($"Kết nối lỗi: {e.GetBaseException().Message}"); }
        dangKetNoi = false;
    }

    /// <summary>START: chỉ lúc này mới mở camera thật; không có ffmpeg/camera thì dùng camera giả lập.</summary>
    void BatCamera()
    {
        INguonHinh nguon;
        int chon = CboCam.SelectedIndex;
        string ten = TxtTen.Text.Trim();
        ushort ma;
        lock (May.Khoa) ma = May.Ma;
        if (chon <= 0) nguon = new CameraGiaLap(ten, ma);
        else
        {
            try { nguon = CameraFfmpeg.Mo(camThat[chon - 1], May.FpsCamera); }
            catch (System.ComponentModel.Win32Exception e)
            {
                Log($"Không chạy được ffmpeg ({e.Message}) – dùng camera giả lập");
                nguon = new CameraGiaLap(ten, ma);
                CboCam.SelectedIndex = 0;
            }
        }
        May.BatCamera(nguon);
        BtnStart.Enabled = false;
        BtnCancel.Enabled = true;
        lblLoaiCam.Text = nguon.That ? $"Camera: THẬT – {camThat[chon - 1]}" : "Camera: giả lập (khung tổng hợp)";
        lblLoaiCam.ForeColor = nguon.That ? Color.DarkRed : Color.DarkBlue;
    }

    void CapNhat()
    {
        while (nhatKy.TryDequeue(out _)) { }                    // nhật ký đã in ra stdout; cửa sổ chỉ hiện trạng thái
        byte[]? xem;
        (int W, int H, byte[] Anh)? luoi;
        string trangThai, dongNhan;
        bool ketNoi, loi, camBat;
        lock (May.Khoa)
        {
            (xem, luoi, ketNoi, loi, camBat) = (May.XemTruoc, May.Luoi, May.DaKetNoi, May.TrangThaiLoi, May.Camera != null);
            string gui = camBat && May.CamMo && ketNoi ? "Đang truyền dữ liệu camera…" : camBat ? $"camera {May.LyDoCam} – không gửi" : "camera tắt";
            trangThai = loi || !ketNoi ? May.TrangThai
                : $"{gui} | gửi {May.FpsGui:F0} fps, {May.KbpsGui:F0} kbit/s (tối đa {May.FpsCho} fps, 1/{May.Chia}) | " +
                  $"nhận {May.FpsNhan:F0} fps, {May.KbpsNhan:F0} kbit/s";
            dongNhan = ketNoi ? $"Mã {May.Ma}; server cho nhận {May.NhanFps} fps, 1/{May.NhanChia}; mảnh mất {May.ManhMat}" : "";
        }
        if (!camBat && BtnCancel.Enabled)                        // camera thật hỏng: máy tự tắt camera
        {
            BtnStart.Enabled = true;
            BtnCancel.Enabled = false;
            lblLoaiCam.Text = "Camera: tắt (lỗi camera)";
        }
        XemTruoc.Dat(xem != null ? GD.SangBitmap(xem, Chung.Rong, Chung.Cao) : null, camBat ? "Camera của tôi" : "", "", !camBat);
        Luoi.Dat(luoi is { } l ? GD.SangBitmap(l.Anh, l.W, l.H) : null, ketNoi ? "Lưới từ server" : "",
                 luoi is { } l2 ? $"{l2.W}x{l2.H}" : "", false);
        if (!dangKetNoi) lblTrangThai.Text = "Trạng thái: " + trangThai;
        lblTrangThai.ForeColor = loi ? Color.Red : Color.Black;
        lblNhan.Text = dongNhan;
        BtnKetNoi.Enabled = !ketNoi && !dangKetNoi;
        BtnNgat.Enabled = ketNoi;
        TxtIp.Enabled = TxtCong.Enabled = TxtTen.Enabled = !ketNoi;
    }
}
