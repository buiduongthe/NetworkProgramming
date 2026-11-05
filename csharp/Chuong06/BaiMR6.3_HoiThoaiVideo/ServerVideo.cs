using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;
using static BaiMR6_3.Chung;

namespace BaiMR6_3;

// Server phòng hội thoại video nhiều client (ghép lưới kiểu MCU).
// Hai kênh giống MR6.2:
//   - TCP: thông điệp JSON điều khiển (vào phòng, bật/tắt camera, tham số gửi/nhận, sự kiện);
//   - UDP: khung hình nén zlib, chia mảnh ≤ 1200 byte mỗi datagram.
// Kiến trúc MCU (Multipoint Control Unit): server giải nén khung của mọi client, ghép thành một lưới
// (mỗi ô 160×120), nén lại và gửi cho từng người nghe theo tốc độ khung/độ phân giải người đó yêu cầu.
// Quy tắc quyền điều khiển (giống mic ở MR6.2):
//   1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
//   2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
//   3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
//   4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và fps chung do server đặt.
//   5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn; fps nhận thực tế ≤ fps chung.
//   6. Server tự thực thi: bỏ khung của camera đang tắt, bỏ khung đến nhanh hơn fps cho phép,
//      bỏ datagram sai địa chỉ nguồn.
// Lệnh bảng điều khiển (stdin; "@t lệnh" = chờ tới giây thứ t):
//   tat_camera <tên|tat_ca>   bat_camera <tên|tat_ca>   fps_toi_da <tên|tat_ca> <n>
//   do_phan_giai <tên|tat_ca> <1|2|4>   fps_chung <n>   trang_thai   thoat
// Phần xử lý phòng nằm trong lớp PhongVideo, dùng chung cho bản dòng lệnh (lớp ServerVideo bên dưới) và bản
// giao diện (thư mục GiaoDien: lưới camera của mọi client hiển thị ngay trên cửa sổ server).
class Khach(ushort ma, string ten, Socket conn, IPAddress ip)
{
    public const int FpsMacDinh = 10;
    public readonly ushort Ma = ma;
    public readonly string Ten = ten;
    public readonly Socket Conn = conn;
    public readonly IPAddress Ip = ip;
    readonly object khoaGui = new();
    public IPEndPoint? Udp;
    public bool TuTat, ServerTat;                 // client tự tắt / server tắt camera
    public int FpsToiDa = FpsMacDinh;             // giới hạn gửi do server đặt
    public int Chia = 1;                          // độ phân giải gửi = 1/Chia mỗi chiều
    public int NhanFps = FpsMacDinh, NhanChia = 1; // tham số nhận do client chọn
    public readonly GhepKhung Ghep = new();
    public byte[]? O;                             // ô 160×120 đã phóng to, có nhãn – dùng khi ghép lưới
    public double LucNhanCuoi;
    public int BoVuotFps, BoTat, SaiNguon;
    public int DemKhung, DemByte;                 // cửa sổ 1 giây để đo fps, kbit/s vào
    public double FpsVao, KbpsVao;
    public long SoKhungRa;                        // số khung lưới đã gửi cho client này
    public double HanGuiRa;
    public int DemRa, ByteRa;
    public double FpsRa, KbpsRa;

    public bool CamMo => !TuTat && !ServerTat;     // quy tắc 1
    public string LyDo() => CamMo ? "mở" : TuTat && ServerTat ? "tắt (cả hai)" : ServerTat ? "tắt (server)" : "tắt (tự tắt)";

    public void Gui(JsonObject obj)
    {
        try { lock (khoaGui) GuiJson(Conn, obj); }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
    }

    /// <summary>Báo cho client tham số gửi hiện hành.</summary>
    public void GuiCamera()
    {
        Gui(new JsonObject { ["loai"] = "camera", ["mo"] = CamMo, ["ly_do"] = LyDo(), ["fps"] = FpsToiDa, ["chia"] = Chia });
        VeO();
    }

    public void GuiNhan(int fpsChung) =>
        Gui(new JsonObject { ["loai"] = "nhan", ["fps"] = Math.Min(NhanFps, fpsChung), ["chia"] = NhanChia });

    /// <summary>Vẽ ô của client trong lưới: hình đã phóng về 160×120 + dải nhãn trạng thái.</summary>
    public void VeO((int W, int H, byte[] Anh)? khung = null)
    {
        byte[] o;
        if (khung is { } k) o = PhongTo(k.Anh, k.W, k.H, Rong / k.W);
        else if (CamMo && O != null) o = O;
        else
        {
            o = new byte[Rong * Cao * 3];
            ToMau(o, Rong, 0, 0, Rong, Cao, (40, 40, 40));
            if (!CamMo)
            {
                VeChu(o, Rong, Cao, 20, 44, "TAT CAMERA", 2);
                VeChu(o, Rong, Cao, 44, 66, ServerTat ? "(SERVER)" : "(TU TAT)", 2);
            }
        }
        ToMau(o, Rong, 0, Cao - 11, Rong, 11, (0, 0, 0));
        string nhan = $"{Ten} {FpsToiDa}FPS" + (Chia > 1 ? $" 1/{Chia}" : "");
        VeChu(o, Rong, Cao, 2, Cao - 9, nhan, 1, (255, 255, 0));
        O = o;
    }
}

/// <summary>Một phòng video: nhận kết nối, ghép mảnh, ghép lưới và gửi, thực thi quyền điều khiển.
/// Trạng thái dùng chung được bảo vệ bằng Khoa (bản giao diện cũng khóa Khoa khi đọc ô hình của client).</summary>
class PhongVideo(Action<string> log)
{
    public readonly object Khoa = new();
    public readonly SortedDictionary<ushort, Khach> DsKhach = [];   // mã -> client (theo thứ tự mã)
    public readonly List<Khach> DaRoi = [];
    ushort maMoi;
    public int FpsChung { get; private set; } = Khach.FpsMacDinh;   // giới hạn fps đầu ra chung cho mọi người nhận
    readonly double t0 = Now;
    int congUdp;
    TcpListener? srv;
    Socket? udp;
    volatile bool dung;

    public double ThoiGian => Now - t0;
    void Log(string text) => log(text);

    void PhatSuKien(string text)
    {
        Log($"  => {text}");
        foreach (var k in DsKhach.Values.ToList()) k.Gui(new JsonObject { ["loai"] = "su_kien", ["noi_dung"] = text });
    }

    // ---------------------------------------------------------------- Khởi động / dừng
    public void BatDau(string host, int congTcp, int congUdp)
    {
        this.congUdp = congUdp;
        srv = new TcpListener(IPAddress.Parse(host), congTcp);
        srv.Start();
        udp = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp) { ReceiveBufferSize = 1 << 20 };   // mảnh đến dồn dập
        try { udp.Bind(new IPEndPoint(IPAddress.Parse(host), congUdp)); }
        catch { srv.Stop(); udp.Close(); throw; }
        Log($"TCP {host}:{congTcp} điều khiển; UDP :{congUdp} video");
        Log($"Khung {Rong}x{Cao} RGB {Rong * Cao * 3} B, nén zlib, mảnh ≤ 1200 B");
        void Bg(Action f) => new Thread(() => f()) { IsBackground = true }.Start();
        var l = srv;
        var u = udp;
        Bg(() =>
        {
            while (!dung)
            {
                Socket c;
                try { c = l.AcceptSocket(); }
                catch (Exception e) when (e is SocketException || e is ObjectDisposedException || e is InvalidOperationException) { return; }
                c.NoDelay = true;
                Bg(() => PhucVu(c));
            }
        });
        Bg(() => NhanUdp(u));
        Bg(() => VongGui(u));
    }

    /// <summary>Dừng phòng (bản giao diện: nút STOP SERVER).</summary>
    public void Dung()
    {
        dung = true;
        srv?.Stop();
        udp?.Close();
        List<Khach> ds;
        lock (Khoa) ds = DsKhach.Values.ToList();
        foreach (var k in ds)
        {
            try { k.Conn.Shutdown(SocketShutdown.Both); } catch (Exception) { }
            k.Conn.Close();
        }
    }

    // ---------------------------------------------------------------- Kênh điều khiển TCP
    void PhucVu(Socket conn)
    {
        var addr = (IPEndPoint)conn.RemoteEndPoint!;
        var f = new StreamReader(new NetworkStream(conn), Encoding.UTF8);
        Khach? k = null;
        try
        {
            var tin = JsonNode.Parse(f.ReadLine() ?? "{}") as JsonObject;
            string? ten = tin?["ten"]?.GetValue<string>();
            if (tin?["loai"]?.GetValue<string>() != "vao" || string.IsNullOrEmpty(ten)) return;
            lock (Khoa)
            {
                k = new Khach(++maMoi, ten.Length > 8 ? ten[..8] : ten, conn, addr.Address);
                DsKhach[k.Ma] = k;
            }
            k.Gui(new JsonObject { ["loai"] = "chao", ["ma"] = k.Ma, ["cong_udp"] = congUdp, ["rong"] = Rong, ["cao"] = Cao });
            Log($"#{k.Ma} {k.Ten} vào phòng từ {addr.Address}:{addr.Port}");
            lock (Khoa)
            {
                k.GuiCamera();
                k.GuiNhan(FpsChung);
                PhatSuKien($"{k.Ten} vào phòng");
            }
            string? dong;
            while ((dong = f.ReadLine()) != null)
            {
                var t = JsonNode.Parse(dong)!.AsObject();
                lock (Khoa) XuLyTinClient(k, t);
            }
        }
        catch (Exception e) when (e is IOException || e is SocketException || e is System.Text.Json.JsonException ||
                                  e is InvalidOperationException || e is FormatException || e is ObjectDisposedException) { }
        finally
        {
            if (k != null)
            {
                lock (Khoa)
                {
                    DsKhach.Remove(k.Ma);
                    DaRoi.Add(k);
                    Log($"#{k.Ma} {k.Ten} rời phòng");
                    PhatSuKien($"{k.Ten} rời phòng");
                }
            }
            conn.Close();
        }
    }

    void XuLyTinClient(Khach k, JsonObject tin)
    {
        switch (tin["loai"]?.GetValue<string>())
        {
            case "tu_tat_camera":
                k.TuTat = true;
                k.GuiCamera();
                PhatSuKien($"{k.Ten} tự tắt camera");
                break;
            case "tu_bat_camera":
                k.TuTat = false;
                if (k.ServerTat)                                          // quy tắc 2
                {
                    Log($"{k.Ten} xin bật camera -> từ chối (server đang tắt)");
                    k.Gui(new JsonObject { ["loai"] = "tu_choi", ["ly_do"] = "server đang tắt camera của bạn" });
                }
                else
                {
                    k.GuiCamera();
                    PhatSuKien($"{k.Ten} tự bật camera");
                }
                break;
            case "nhan":                                                  // quy tắc 5
                k.NhanFps = Math.Clamp(tin["fps"]?.GetValue<int>() ?? k.NhanFps, 1, 30);
                k.NhanChia = (tin["chia"]?.GetValue<int>() ?? k.NhanChia) == 2 ? 2 : 1;
                Log($"{k.Ten} chọn nhận {k.NhanFps} fps, độ phân giải 1/{k.NhanChia}");
                k.GuiNhan(FpsChung);
                break;
        }
    }

    // ---------------------------------------------------------------- Kênh dữ liệu UDP
    void NhanUdp(Socket sock)
    {
        var buf = new byte[2048];
        while (true)
        {
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            int n;
            try { n = sock.ReceiveFrom(buf, ref from); }
            catch (SocketException) when (!dung) { continue; }
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }   // đã Dung()
            var addr = (IPEndPoint)from;
            var h = TachHeader(buf.AsSpan(0, n));
            if (h == null) continue;
            double bayGio = Now;
            lock (Khoa)
            {
                if (!DsKhach.TryGetValue(h.Ma, out var k)) continue;
                if (k.Udp == null && addr.Address.Equals(k.Ip))
                {
                    k.Udp = addr;
                    Log($"#{k.Ma} {k.Ten} đăng ký UDP {addr.Address}:{addr.Port}");
                }
                if (!addr.Equals(k.Udp))                                  // quy tắc 6
                {
                    k.SaiNguon++;
                    continue;
                }
                if (h.Loai != LoaiManh) continue;
                var khung = k.Ghep.Them(h.So, h.I, h.Tong, h.W, h.H, h.Payload, bayGio);
                if (khung == null) continue;
                if (!k.CamMo)                                             // quy tắc 6: camera tắt
                {
                    k.BoTat++;
                    continue;
                }
                if (bayGio - k.LucNhanCuoi < 0.8 / k.FpsToiDa)           // quy tắc 6: vượt fps (dung sai 20%)
                {
                    k.BoVuotFps++;
                    continue;
                }
                byte[] anh;
                try { anh = GiaiNen(khung.Value.DuLieu); }
                catch (InvalidDataException) { continue; }
                int w = h.W, cao = h.H;
                if (w == 0 || w * cao * 3 != anh.Length || Rong % w != 0 || w * Cao != cao * Rong) continue;   // kích thước không hợp lệ
                k.LucNhanCuoi = bayGio;
                k.DemKhung++;
                k.DemByte += khung.Value.DuLieu.Length;
                k.VeO((w, cao, anh));
            }
        }
    }

    // ---------------------------------------------------------------- Ghép lưới và gửi
    /// <summary>Ghép ô của mọi client thành lưới c cột × r hàng (c = ⌈√n⌉).</summary>
    (byte[] Anh, int W, int H) GhepLuoi(List<Khach> ds)
    {
        int n = Math.Max(1, ds.Count);
        int cot = (int)Math.Ceiling(Math.Sqrt(n)), hang = (n + cot - 1) / cot;
        int W = cot * Rong, H = hang * Cao;
        var ra = new byte[W * H * 3];
        ToMau(ra, W, 0, 0, W, H, (15, 15, 15));                       // ô trống
        for (int idx = 0; idx < ds.Count; idx++)
        {
            int r = idx / cot, c = idx % cot;
            var o = ds[idx].O!;
            for (int y = 0; y < Cao; y++)
                Array.Copy(o, y * Rong * 3, ra, ((r * Cao + y) * W + c * Rong) * 3, Rong * 3);
        }
        return (ra, W, H);
    }

    /// <summary>Mỗi 5 ms kiểm tra: người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ theo độ phân giải
    /// nhận, nén, chia mảnh và gửi. Mỗi giây tính lại fps, kbit/s.</summary>
    void VongGui(Socket sock)
    {
        double mocDo = Now;
        while (!dung)
        {
            double bayGio = Now;
            lock (Khoa)
            {
                var ds = DsKhach.Values.Where(k => k.O != null).ToList();
                foreach (var k in DsKhach.Values)
                {
                    int fps = Math.Min(k.NhanFps, FpsChung);
                    if (k.Udp == null || ds.Count == 0 || bayGio < k.HanGuiRa) continue;
                    k.HanGuiRa = Math.Max(k.HanGuiRa + 1.0 / fps, bayGio);
                    var (anh, w, h) = GhepLuoi(ds);
                    (anh, w, h) = ThuNho(anh, w, h, k.NhanChia);
                    var nen = Nen(anh);
                    foreach (var g in ChiaManh(MaServer, k.SoKhungRa, w, h, nen))
                    {
                        try { sock.SendTo(g, k.Udp); } catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
                    }
                    k.SoKhungRa++;
                    k.DemRa++;
                    k.ByteRa += nen.Length;
                }
                if (bayGio - mocDo >= 1.0)
                {
                    foreach (var k in DsKhach.Values)
                    {
                        (k.FpsVao, k.KbpsVao) = (k.DemKhung, k.DemByte * 8 / 1000.0);
                        (k.FpsRa, k.KbpsRa) = (k.DemRa, k.ByteRa * 8 / 1000.0);
                        k.DemKhung = k.DemByte = k.DemRa = k.ByteRa = 0;
                    }
                    mocDo += 1.0;
                }
            }
            Thread.Sleep(5);
        }
    }

    // ---------------------------------------------------------------- Lệnh điều khiển (bảng điều khiển / nút bấm)
    public List<Khach> Chon(string ten)
    {
        lock (Khoa) return DsKhach.Values.Where(k => ten == "tat_ca" || k.Ten == ten).ToList();
    }

    /// <summary>Server tắt (tat = true) hoặc bật lại camera của các client ds; ai: tên hiển thị trong sự kiện.</summary>
    public void DatCamera(List<Khach> ds, bool tat, string ai)
    {
        lock (Khoa)
        {
            foreach (var k in ds)
            {
                k.ServerTat = tat;
                k.GuiCamera();
            }
            if (tat) PhatSuKien($"server tắt camera {ai}");
            else
            {
                var vanTat = ds.Where(k => k.TuTat).Select(k => k.Ten).ToList();   // quy tắc 3
                string them = vanTat.Count > 0 ? $" ({string.Join(", ", vanTat)} vẫn tự tắt)" : "";
                PhatSuKien($"server bật camera {ai}{them}");
            }
        }
    }

    public void DatFpsToiDa(List<Khach> ds, int fps, string ai)
    {
        lock (Khoa)
        {
            foreach (var k in ds)
            {
                k.FpsToiDa = Math.Clamp(fps, 1, 30);
                k.GuiCamera();
            }
            PhatSuKien($"server giới hạn {ai} gửi tối đa {fps} fps");
        }
    }

    public void DatDoPhanGiai(List<Khach> ds, int chia, string ai)
    {
        lock (Khoa)
        {
            foreach (var k in ds)
            {
                k.Chia = chia;
                k.GuiCamera();
            }
            PhatSuKien($"server đặt độ phân giải {ai} = 1/{chia}");
        }
    }

    public void DatFpsChung(int fps)
    {
        lock (Khoa)
        {
            FpsChung = Math.Clamp(fps, 1, 30);
            foreach (var k in DsKhach.Values) k.GuiNhan(FpsChung);
            PhatSuKien($"server đặt fps chung tối đa {FpsChung}");
        }
    }

    /// <summary>Thực hiện một dòng lệnh bảng điều khiển; trả false khi gặp "thoat".</summary>
    public bool Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (p.Length == 0) return true;
        Log($"t={F(ThoiGian, "F1")}s > {dong}");
        lock (Khoa)
        {
            string ai = p.Length > 1 && p[1] == "tat_ca" ? "tất cả" : p.Length > 1 ? p[1] : "";
            if ((p[0] == "tat_camera" || p[0] == "bat_camera") && p.Length == 2) DatCamera(Chon(p[1]), p[0] == "tat_camera", ai);
            else if (p[0] == "fps_toi_da" && p.Length == 3) DatFpsToiDa(Chon(p[1]), int.Parse(p[2]), ai);
            else if (p[0] == "do_phan_giai" && p.Length == 3 && p[2] is "1" or "2" or "4") DatDoPhanGiai(Chon(p[1]), int.Parse(p[2]), ai);
            else if (p[0] == "fps_chung" && p.Length == 2) DatFpsChung(int.Parse(p[1]));
            else if (p[0] == "trang_thai") InTrangThai();
            else if (p[0] == "thoat") return false;
            else Log("lệnh không hợp lệ");
        }
        return true;
    }

    public void InTrangThai()
    {
        lock (Khoa)
        {
            Log($"Trạng thái: {DsKhach.Count} client, fps chung {FpsChung}");
            Log("                     giới hạn vào        ra (lưới)");
            Log(" #  tên camera       fps 1/n  fps  kbit  fps  kbit");
            foreach (var k in DsKhach.Values)
                Log($"{k.Ma,2}  {k.Ten,-3} {k.LyDo(),-12} {k.FpsToiDa,3} {k.Chia,3} {F(k.FpsVao, "F0"),4}" +
                    $" {F(k.KbpsVao, "F0"),5} {F(k.FpsRa, "F0"),4} {F(k.KbpsRa, "F0"),5}");
        }
    }

    public void InTongKet()
    {
        lock (Khoa)
        {
            Log("Tổng kết luồng vào (từ client tới server):");
            Log(" #  tên mảnh khung hỏng mảnh mất  bỏ:tắt vượt fps");
            Log("        nhận    đủ        mất hẳn");
            foreach (var k in DsKhach.Values.Concat(DaRoi).OrderBy(k => k.Ma))
            {
                var g = k.Ghep;
                Log($"{k.Ma,2}  {k.Ten,-3} {g.ManhNhan,4} {g.KhungDu,5} {g.KhungHong,4} {g.ManhMat,4}" +
                    $" {g.KhungMatHan,4} {k.BoTat,6} {k.BoVuotFps,6}");
            }
        }
    }
}

/// <summary>Bản dòng lệnh: một phòng + bảng điều khiển đọc lệnh từ stdin.</summary>
static class ServerVideo
{
    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int congTcp = a.Length > 1 ? int.Parse(a[1]) : 41630;
        int congUdp = a.Length > 2 ? int.Parse(a[2]) : 41631;
        void Log(string text) => Console.WriteLine($"[server] {text}");
        var phong = new PhongVideo(Log);
        phong.BatDau(host, congTcp, congUdp);
        var dung = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; dung.Set(); };
        new Thread(() =>
        {
            string? dong;
            while ((dong = Console.ReadLine()) != null)
            {
                dong = dong.Split('#')[0].Trim();
                if (dong.StartsWith('@'))
                {
                    int sp = dong.IndexOf(' ');
                    string moc = sp < 0 ? dong : dong[..sp];
                    dong = sp < 0 ? "" : dong[(sp + 1)..];
                    double cho = double.Parse(moc[1..], CultureInfo.InvariantCulture) - phong.ThoiGian;
                    if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
                }
                if (dong != "" && !phong.Lenh(dong))
                {
                    dung.Set();
                    return;
                }
            }
        }) { IsBackground = true }.Start();
        dung.Wait();
        Log("Dừng server (Ctrl+C)");
        phong.InTongKet();
        return 0;
    }
}
