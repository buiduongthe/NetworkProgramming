using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using static BaiMR6_3.Chung;

namespace BaiMR6_3;

// Phần "máy" của client video giao diện (không phụ thuộc WinForms). Cùng giao thức với KhachVideo.cs:
//   - luồng đọc TCP: "camera" (server cho gửi không, fps tối đa, độ phân giải), "nhan", "su_kien", "tu_choi";
//   - luồng nhận UDP: ghép mảnh -> giải nén -> giữ khung lưới mới nhất;
//   - luồng gửi (chạy suốt đời đối tượng): theo fps cho phép lấy khung từ camera (thật/giả lập), giữ làm ảnh xem
//     trước; nếu đã kết nối và server cho camera mở -> thu nhỏ, nén zlib, chia mảnh ≤ 1200 byte, gửi UDP.
// Giao diện đọc ảnh xem trước, lưới và số liệu mỗi 100 ms (trong Khoa).
sealed class KetNoiVideo : IDisposable
{
    public readonly object Khoa = new();
    readonly Action<string> log;

    Socket? tcp, udp;
    GhepKhung ghep = new();
    public bool DaKetNoi { get; private set; }
    public ushort Ma { get; private set; }
    public bool CamMo { get; private set; }               // server cho camera mở (thông điệp "camera")
    public string LyDoCam { get; private set; } = "chưa kết nối";
    public int FpsCho { get; private set; } = 30;          // giới hạn gửi do server đặt
    public int Chia { get; private set; } = 1;
    public int NhanFps { get; private set; }
    public int NhanChia { get; private set; } = 1;
    public string TrangThai { get; private set; } = "Chưa kết nối";
    public bool TrangThaiLoi { get; private set; }

    public INguonHinh? Camera;                             // null = CANCEL (không thu hình)
    public int FpsCamera = 15;
    public byte[]? XemTruoc { get; private set; }          // khung 160×120 mới nhất của camera mình
    public (int W, int H, byte[] Anh)? Luoi { get; private set; }   // khung lưới mới nhất từ server
    public double FpsGui { get; private set; }
    public double KbpsGui { get; private set; }
    public double FpsNhan { get; private set; }
    public double KbpsNhan { get; private set; }
    public int ManhMat => ghep.ManhMat;
    int demGui, byteGui, demNhan, byteNhan;

    volatile bool huy;
    readonly Thread luongGui;

    public KetNoiVideo(Action<string> log)
    {
        this.log = log;
        luongGui = new Thread(VongGui) { IsBackground = true, Name = "gui-video" };
        luongGui.Start();
    }

    void DatTrangThai(string s, bool loi = false)
    {
        lock (Khoa) (TrangThai, TrangThaiLoi) = (s, loi);
        log(s);
    }

    // ---------------------------------------------------------------- Kết nối (gọi bằng Task.Run)
    public void KetNoi(string host, int cong, string ten)
    {
        var t = new Socket(SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        try
        {
            if (!t.ConnectAsync(host, cong).Wait(TimeSpan.FromSeconds(3)))
                throw new TimeoutException($"không kết nối được {host}:{cong} sau 3 s");
            t.ReceiveTimeout = 3000;
            var f = new StreamReader(new NetworkStream(t), Encoding.UTF8);
            GuiJson(t, new JsonObject { ["loai"] = "vao", ["ten"] = ten });
            var chao = JsonNode.Parse(f.ReadLine() ?? "{}")!.AsObject();
            if (chao["loai"]?.GetValue<string>() != "chao") throw new InvalidDataException("server không chào");
            t.ReceiveTimeout = 0;
            var u = new Socket(SocketType.Dgram, ProtocolType.Udp) { ReceiveBufferSize = 1 << 20 };
            u.Connect(host, chao["cong_udp"]!.GetValue<int>());
            bool camBat;
            lock (Khoa)
            {
                (tcp, udp) = (t, u);
                Ma = chao["ma"]!.GetValue<ushort>();
                ghep = new GhepKhung();
                (CamMo, LyDoCam, Luoi) = (false, "chờ server", null);
                DaKetNoi = true;
                camBat = Camera != null;
            }
            DatTrangThai($"Đã kết nối {host}:{cong}, mã {Ma}");
            new Thread(() => DocTcp(t, f)) { IsBackground = true, Name = "doc-tcp" }.Start();
            new Thread(() => NhanUdp(u)) { IsBackground = true, Name = "nhan-udp" }.Start();
            u.Send(GoiGiu(Ma));                                          // đăng ký địa chỉ UDP với server
            if (!camBat) Gui(new JsonObject { ["loai"] = "tu_tat_camera" });   // chưa START: không tự gửi hình
        }
        catch (Exception e)
        {
            t.Close();
            lock (Khoa) (TrangThai, TrangThaiLoi) = ($"Không kết nối được: {e.GetBaseException().Message}", true);
            throw;
        }
    }

    public void NgatKetNoi()
    {
        Socket? t, u;
        lock (Khoa)
        {
            (t, u) = (tcp, udp);
            (tcp, udp) = (null, null);
            DaKetNoi = false;
            (CamMo, LyDoCam, Luoi) = (false, "chưa kết nối", null);
        }
        t?.Close();
        u?.Close();
        if (t != null) DatTrangThai("Đã ngắt kết nối");
    }

    void Gui(JsonObject obj)
    {
        Socket? t;
        lock (Khoa) t = tcp;
        try { if (t != null) GuiJson(t, obj); }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
    }

    /// <summary>START: dùng nguồn hình này và xin server bật camera (server có thể từ chối).</summary>
    public void BatCamera(INguonHinh nguon)
    {
        INguonHinh? cu;
        lock (Khoa) (cu, Camera) = (Camera, nguon);
        cu?.Dispose();
        log($"START: {nguon.MoTa}");
        Gui(new JsonObject { ["loai"] = "tu_bat_camera" });
    }

    /// <summary>CANCEL: tự tắt có hiệu lực ngay, rồi báo server.</summary>
    public void TatCamera()
    {
        INguonHinh? cu;
        lock (Khoa)
        {
            (cu, Camera, XemTruoc) = (Camera, null, null);
            if (DaKetNoi) (CamMo, LyDoCam) = (false, "tắt (tự tắt)");
        }
        cu?.Dispose();
        log("CANCEL: tắt camera");
        Gui(new JsonObject { ["loai"] = "tu_tat_camera" });
    }

    /// <summary>Tham số nhận do client tự chọn (quy tắc 5); server trả lại "nhan" với fps thực tế.</summary>
    public void DatNhan(int? fps, int? chia)
    {
        var o = new JsonObject { ["loai"] = "nhan" };
        if (fps is int f) o["fps"] = f;
        if (chia is int c) o["chia"] = c;
        Gui(o);
    }

    // ---------------------------------------------------------------- Luồng đọc TCP
    void DocTcp(Socket t, StreamReader f)
    {
        try
        {
            string? dong;
            while ((dong = f.ReadLine()) != null)
            {
                var tin = JsonNode.Parse(dong)!.AsObject();
                switch (tin["loai"]?.GetValue<string>())
                {
                    case "camera":
                        lock (Khoa)
                            (CamMo, LyDoCam, FpsCho, Chia) = (tin["mo"]!.GetValue<bool>(), tin["ly_do"]?.GetValue<string>() ?? "",
                                                              tin["fps"]!.GetValue<int>(), tin["chia"]!.GetValue<int>());
                        log($"Camera: {tin["ly_do"]}, gửi tối đa {tin["fps"]} fps, 1/{tin["chia"]}");
                        break;
                    case "nhan":
                        lock (Khoa) (NhanFps, NhanChia) = (tin["fps"]!.GetValue<int>(), tin["chia"]!.GetValue<int>());
                        log($"Nhận: {tin["fps"]} fps, độ phân giải 1/{tin["chia"]}");
                        break;
                    case "su_kien":
                        log($"Sự kiện: {tin["noi_dung"]}");
                        break;
                    case "tu_choi":
                        DatTrangThai($"Bị từ chối: {tin["ly_do"]}", true);
                        break;
                }
            }
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is JsonException ||
                                  e is ObjectDisposedException || e is InvalidOperationException) { }
        bool laHienTai;
        lock (Khoa) laHienTai = tcp == t;
        if (laHienTai)
        {
            NgatKetNoi();
            DatTrangThai("Mất kết nối với server", true);
        }
    }

    // ---------------------------------------------------------------- Luồng nhận UDP
    void NhanUdp(Socket u)
    {
        var buf = new byte[2048];
        while (true)
        {
            int n;
            try { n = u.Receive(buf); }
            catch (SocketException) when (udp == u) { continue; }
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }
            var h = TachHeader(buf.AsSpan(0, n));
            if (h == null || h.Loai != LoaiManh) continue;
            (int W, int H, byte[] DuLieu)? khung;
            lock (Khoa) khung = ghep.Them(h.So, h.I, h.Tong, h.W, h.H, h.Payload, Now);
            if (khung is not { } k) continue;
            byte[] anh;
            try { anh = GiaiNen(k.DuLieu); }
            catch (InvalidDataException) { continue; }
            if (anh.Length != h.W * h.H * 3) continue;
            lock (Khoa)
            {
                Luoi = (h.W, h.H, anh);
                demNhan++;
                byteNhan += k.DuLieu.Length;
            }
        }
    }

    // ---------------------------------------------------------------- Luồng gửi
    void VongGui()
    {
        double hanChup = Now, hanGiu = Now, mocDo = Now;
        long soKhung = 0;
        while (!huy)
        {
            double bayGio = Now;
            INguonHinh? cam;
            Socket? u;
            bool mo;
            int fps, chia;
            ushort ma;
            lock (Khoa)
            {
                (cam, u, mo, chia, ma) = (Camera, udp, CamMo, Chia, Ma);
                fps = Math.Max(1, u != null ? Math.Min(FpsCamera, FpsCho) : FpsCamera);
            }
            bool daGui = false;
            if (cam != null && bayGio >= hanChup)
            {
                hanChup = Math.Max(hanChup + 1.0 / fps, bayGio);
                var anh = cam.LayKhung();
                if (anh != null)
                {
                    lock (Khoa) XemTruoc = anh;
                    if (u != null && mo)
                    {
                        var (nho, w, h) = ThuNho(anh, Rong, Cao, chia);      // giảm độ phân giải trước khi nén
                        var nen = Nen(nho);
                        try
                        {
                            foreach (var g in ChiaManh(ma, soKhung, w, h, nen)) u.Send(g);
                            daGui = true;
                        }
                        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
                        soKhung++;
                        lock (Khoa)
                        {
                            demGui++;
                            byteGui += nen.Length;
                        }
                    }
                }
            }
            if (u != null && !daGui && (cam == null || !mo) && bayGio >= hanGiu)
            {
                try { u.Send(GoiGiu(ma)); } catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
                hanGiu = bayGio + 1.0;                                     // giữ kết nối khi không gửi hình
            }
            if (bayGio - mocDo >= 1.0)
            {
                mocDo += 1.0;
                lock (Khoa)
                {
                    (FpsGui, KbpsGui, FpsNhan, KbpsNhan) = (demGui, byteGui * 8 / 1000.0, demNhan, byteNhan * 8 / 1000.0);
                    demGui = byteGui = demNhan = byteNhan = 0;
                }
            }
            if (cam?.Loi is string loi)                                    // camera thật hỏng (rút cáp, ffmpeg lỗi)
            {
                DatTrangThai($"Camera lỗi – {loi}", true);
                lock (Khoa) if (Camera == cam) Camera = null;
                cam.Dispose();
            }
            Thread.Sleep(5);
        }
    }

    public void Dispose()
    {
        NgatKetNoi();
        huy = true;
        luongGui.Join(500);
        INguonHinh? c;
        lock (Khoa) (c, Camera) = (Camera, null);
        c?.Dispose();
    }
}
