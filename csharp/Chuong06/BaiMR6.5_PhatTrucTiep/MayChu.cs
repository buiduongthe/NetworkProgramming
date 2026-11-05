using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR6_5.Chung;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Lõi máy chủ trung chuyển phát trực tiếp 1→n (kiểu SFU), dạng lớp.
//
// Dùng chung cho bản dòng lệnh (lớp ServerPhat cuối tệp) và bản giao diện (GiaoDien/FormServer.cs); không chạm
// giao diện, báo sự kiện bằng hàm `bao(text)`. BatDau/Dung được nhiều lần; socket UDP có thời gian chờ để dừng gọn.
//
// Kiến trúc SFU (Selective Forwarding Unit – RFC 7667 mục 3.7): máy chủ KHÔNG giải nén, KHÔNG trộn; mỗi datagram
// của máy phát được NHÂN BẢN nguyên byte tới từng người xem (fan-out):
//     băng thông ra ≈ n × băng thông vào; CPU chỉ tốn cho SendTo() – khác MCU ở MR6.3.
// Máy chủ chỉ đọc header (byte đầu, loại khung, số khung) để: đếm gói/byte, lọc theo đăng ký (âm thanh/video) và
// giữ BỘ ĐỆM GOP: khung I mới nhất + mọi khung P sau nó (như GOP cache của SRS, nginx-rtmp). Người vào muộn nhận
// ngay cả nhóm -> giải được hình tức thì, không phải chờ I kế tiếp.
//
// Luồng: một luồng nhận kết nối TCP, mỗi client một luồng đọc TCP, một luồng nhận UDP chuyển tiếp ngay (mọi gói
// phương tiện đi ra từ CHỈ luồng này nên thứ tự gói được giữ). Trạng thái chung (danh sách client, bộ đệm GOP,
// thống kê) được bảo vệ bằng một khóa.

/// <summary>Một kết nối: máy phát (vai trò "phat") hoặc người xem ("xem").</summary>
sealed class Khach(int ma, string ten, string vaiTro, Socket conn, IPAddress ip)
{
    public readonly int Ma = ma;
    public readonly string Ten = ten, VaiTro = vaiTro;
    public readonly Socket Conn = conn;
    public readonly IPAddress Ip = ip;
    readonly object khoaGui = new();
    public IPEndPoint? Udp;                          // địa chỉ UDP đã đăng ký
    public bool AmThanh = true, Video = true;        // đăng ký nhận (người xem)
    public readonly double LucChao = Now;
    public double LucMoc;                            // mốc đo khung I đầu: lúc vào, hoặc lúc bắt đầu phát nếu muộn hơn
    public bool CanBoDem;                            // chờ luồng UDP gửi bộ đệm GOP cho riêng người này
    public long GoiRa, ByteRa;
    public long DemByte;                             // cửa sổ 1 giây
    public double Kbps;
    public double? KhungDauMs;                       // từ lúc chào tới mảnh khung I đầu tiên gửi cho người này
    public string NguonDau = "";                     // "bộ đệm" hoặc "trực tiếp"
    public int Pli;

    public Khach Moc()
    {
        LucMoc = LucChao;
        return this;
    }

    public void Gui(JsonObject obj)
    {
        try { lock (khoaGui) GuiJson(Conn, obj); }
        catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
    }

    public string Nhan() => AmThanh && Video ? "A+V" : AmThanh ? "A" : Video ? "V" : "-";

    public TomTat Chup() => new(Ma, Ten, Udp != null ? $"{Udp.Address}:{Udp.Port}" : "–", Nhan(), GoiRa, ByteRa, Kbps,
                                KhungDauMs, NguonDau, Pli, Now - LucChao);
}

/// <summary>Tóm tắt một người xem (cho bảng trạng thái, tổng kết, giao diện).</summary>
record TomTat(int Ma, string Ten, string Udp, string Nhan, long GoiRa, long ByteRa, double Kbps, double? KhungDauMs,
              string NguonDau, int Pli, double Giay);

/// <summary>Ảnh chụp trạng thái máy chủ cho dòng lệnh và giao diện.</summary>
record TrangThaiMayChu(bool DangPhat, string? TenPhat, JsonObject? ThongTin, bool BoDemBat, int BoDemGoi, long BoDemByte,
                       long? BoDemSoI, double KbpsVao, double KbpsVaoAm, double KbpsVaoHinh, double KbpsRa, double HeSo,
                       long ByteVao, long ByteRa, long GoiVao, long GoiRa, int PliChuyen, int PliBo, List<TomTat> NguoiXem,
                       List<TomTat> DaRoi, (int W, int H, byte[] Anh, long Ts, long So)? Anh);

sealed class MayChuPhat
{
    readonly Action<string> bao;
    public readonly object Khoa = new();
    volatile bool chay;
    public bool Chay => chay;
    public bool BoDemBat = true;
    public bool XemTruoc { get; set; }       // true: tự giải mã một bản để giao diện hiện ảnh xem trước
    readonly List<Thread> luong = [];
    TcpListener? srv;
    Socket? udp;
    public int CongUdp { get; private set; }
    public string DiaChi { get; private set; } = "";
    int maMoi;

    // trạng thái của một lần chạy (làm mới ở mỗi BatDau)
    Dictionary<int, Khach> ds = [];                  // mã -> Khach
    Khach? mayPhat;
    bool dangPhat;
    JsonObject? thongTin, dongHo;
    List<byte[]> boDem = [];
    long boDemByte;
    long? boDemSoI;
    long byteVao, goiVao, byteRa, goiRa;
    long demVaoAm, demVaoHinh, demRa;
    double kbpsVaoAm, kbpsVaoHinh, kbpsRa;
    int pliChuyen, pliBo;
    double lucPli = -1e9;
    public int SaiNguon;
    List<TomTat> daRoi = [];                         // tóm tắt người xem đã rời (bảng tổng kết)
    (int W, int H, byte[] Anh, long Ts, long So)? anhXemTruoc;
    GhepKhung ghep = new();
    GiaiMaVideo giaiMa = new();

    public MayChuPhat(Action<string>? bao = null) => this.bao = bao ?? (_ => { });

    void LamMoi()
    {
        ds = [];
        mayPhat = null;
        dangPhat = false;
        thongTin = dongHo = null;
        (boDem, boDemByte, boDemSoI) = ([], 0, null);
        byteVao = goiVao = byteRa = goiRa = 0;
        demVaoAm = demVaoHinh = demRa = 0;
        kbpsVaoAm = kbpsVaoHinh = kbpsRa = 0;
        pliChuyen = pliBo = 0;
        lucPli = -1e9;
        SaiNguon = 0;
        daRoi = [];
        anhXemTruoc = null;
        (ghep, giaiMa) = (new(), new());
    }

    // ------------------------------------------------------------ khởi động / dừng
    public void BatDau(string host, int congTcp, int? congUdp = null)
    {
        int cu = congUdp ?? congTcp + 1;
        var l = new TcpListener(IPAddress.Parse(host), congTcp);
        var u = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp)
        {
            ReceiveBufferSize = 1 << 20, SendBufferSize = 1 << 20, ReceiveTimeout = 500,
        };
        try
        {
            l.Start();
            u.Bind(new IPEndPoint(IPAddress.Parse(host), cu));
        }
        catch
        {
            l.Stop();
            u.Close();
            throw;
        }
        TatLoiUdpReset(u);
        lock (Khoa) LamMoi();
        (srv, udp, CongUdp, maMoi) = (l, u, cu, 0);
        DiaChi = $"{host}:{congTcp}";
        chay = true;
        luong.Clear();
        luong.Add(new Thread(() => NhanKetNoi(l)) { IsBackground = true, Name = "nhan-ket-noi" });
        luong.Add(new Thread(() => NhanUdp(u)) { IsBackground = true, Name = "nhan-udp" });
        foreach (var t in luong) t.Start();
        bao($"TCP {host}:{congTcp}, UDP {cu}; bộ đệm GOP: " + (BoDemBat ? "bật" : "tắt"));
    }

    public void Dung()
    {
        if (!chay) return;
        chay = false;
        srv?.Stop();
        List<Khach> dsK;
        lock (Khoa) dsK = ds.Values.ToList();
        foreach (var k in dsK)
        {
            try { k.Conn.Shutdown(SocketShutdown.Both); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        }
        foreach (var t in luong) t.Join(2000);
        udp?.Close();
        bao("Server đã dừng");
    }

    // ------------------------------------------------------------ TCP điều khiển
    /// <summary>Gửi sự kiện cho mọi client (gọi khi đang giữ khóa).</summary>
    void SuKien(string text)
    {
        bao($"=> {text}");
        foreach (var k in ds.Values.ToList()) k.Gui(new JsonObject { ["loai"] = "su_kien", ["noi_dung"] = text });
    }

    List<Khach> DsNguoiXem() => ds.Values.Where(k => k.VaiTro == "xem").ToList();

    /// <summary>Gửi máy phát danh sách người xem mỗi khi danh sách đổi (gọi khi giữ khóa).</summary>
    void BaoNguoiXem()
    {
        if (mayPhat == null) return;
        var ten = DsNguoiXem().OrderBy(k => k.Ma).Select(k => (JsonNode?)JsonValue.Create(k.Ten)).ToArray();
        mayPhat.Gui(new JsonObject { ["loai"] = "nguoi_xem", ["so"] = ten.Length, ["ds"] = new JsonArray(ten) });
    }

    void NhanKetNoi(TcpListener l)
    {
        while (chay)
        {
            Socket c;
            try { c = l.AcceptSocket(); }
            catch (Exception e) when (e is SocketException or ObjectDisposedException or InvalidOperationException) { break; }
            c.NoDelay = true;
            new Thread(() => PhucVu(c)) { IsBackground = true, Name = "tcp-client" }.Start();
        }
    }

    void PhucVu(Socket conn)
    {
        var addr = (IPEndPoint)conn.RemoteEndPoint!;
        var doc = new DocDong(new NetworkStream(conn));
        Khach? k = null;
        try
        {
            var tin = doc.Doc();
            string? vaiTro = tin == null ? null : Chuoi(tin, "vai_tro");
            if (tin == null || Chuoi(tin, "loai") != "chao" || vaiTro is not ("phat" or "xem")) return;
            string? ten = Chuoi(tin, "ten");
            if (!TenHopLe(ten))
            {
                GuiJson(conn, new JsonObject { ["loai"] = "loi", ["ly_do"] = "tên không hợp lệ" });
                return;
            }
            lock (Khoa)
            {
                if (vaiTro == "phat" && mayPhat != null)
                {
                    GuiJson(conn, new JsonObject { ["loai"] = "loi", ["ly_do"] = "đã có máy phát" });
                    bao($"Từ chối máy phát thứ hai {ten}");
                    return;
                }
                k = new Khach(++maMoi, ten!, vaiTro, conn, addr.Address).Moc();
                ds[k.Ma] = k;
                k.Gui(new JsonObject { ["loai"] = "chao", ["ma"] = k.Ma, ["cong_udp"] = CongUdp, ["phat"] = dangPhat });
                if (k.VaiTro == "phat")
                {
                    mayPhat = k;
                    bao($"#{k.Ma} {k.Ten} (phát) vào từ {addr.Address}:{addr.Port}");
                    SuKien($"{k.Ten} kết nối (máy phát)");
                    BaoNguoiXem();
                }
                else
                {
                    if (thongTin != null) k.Gui(thongTin);         // bản đã lưu cho người vào sau
                    if (dongHo != null) k.Gui(dongHo);
                    bao($"#{k.Ma} {k.Ten} (xem) vào từ {addr.Address}:{addr.Port}");
                    SuKien($"{k.Ten} vào xem");
                    BaoNguoiXem();
                }
            }
            while (chay)
            {
                tin = doc.Doc();
                if (tin == null) break;
                lock (Khoa) XuLy(k, tin);
            }
        }
        catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ObjectDisposedException
                                      or InvalidOperationException) { }
        finally
        {
            if (k != null)
                lock (Khoa) Roi(k);
            conn.Close();
        }
    }

    void Roi(Khach k, string lyDo = "rời")
    {
        if (!ds.Remove(k.Ma)) return;
        if (k == mayPhat)
        {
            if (dangPhat) KetThucPhat();
            mayPhat = null;
            SuKien($"{k.Ten} (máy phát) thoát");
        }
        else
        {
            daRoi.Add(k.Chup());
            SuKien($"{k.Ten} {lyDo}");
            BaoNguoiXem();
        }
    }

    void KetThucPhat()
    {
        dangPhat = false;
        thongTin = dongHo = null;
        (boDem, boDemByte, boDemSoI) = ([], 0, null);
        (ghep, giaiMa) = (new(), new());
        foreach (var x in ds.Values) x.Gui(new JsonObject { ["loai"] = "ket_thuc" });
    }

    static string Gt(JsonObject o, string k) => o[k]?.ToJsonString() ?? "None";

    void XuLy(Khach k, JsonObject tin)
    {
        string? loai = Chuoi(tin, "loai");
        if (k.VaiTro == "phat")
        {
            if (loai == "thong_tin")
            {
                thongTin = tin;
                foreach (var x in DsNguoiXem()) x.Gui(tin);           // chuyển tiếp nguyên văn
                bao($"{k.Ten} phát {Gt(tin, "rong")}x{Gt(tin, "cao")} {Gt(tin, "fps")} fps GOP {Gt(tin, "gop")}, {Gt(tin, "tan_so_mau")} Hz");
                if (!dangPhat)
                {
                    dangPhat = true;
                    double bayGio = Now;
                    foreach (var x in DsNguoiXem())                  // người vào trước lúc phát: đo từ lúc phát
                        if (x.KhungDauMs == null) x.LucMoc = bayGio;
                    SuKien($"{k.Ten} bắt đầu phát");
                }
            }
            else if (loai == "dong_ho")
            {
                dongHo = tin;
                foreach (var x in DsNguoiXem()) x.Gui(tin);
            }
            else if (loai == "dung_phat" && dangPhat)
            {
                KetThucPhat();
                SuKien($"{k.Ten} dừng phát");
            }
            return;
        }
        if (loai == "dang_ky")
        {
            bool coVideo = k.Video;
            k.AmThanh = Bool(tin, "am_thanh") ?? k.AmThanh;
            k.Video = Bool(tin, "video") ?? k.Video;
            if (k.Video && !coVideo && BoDemBat && k.Udp != null) k.CanBoDem = true;   // bật lại video: gửi bộ đệm GOP
            bao($"{k.Ten} đăng ký nhận: {k.Nhan()}");
        }
        else if (loai == "xin_khung_khoa")
        {
            k.Pli++;
            double bayGio = Now;
            if (mayPhat != null && dangPhat && bayGio - lucPli >= PliServerS)
            {
                lucPli = bayGio;
                pliChuyen++;
                mayPhat.Gui(new JsonObject { ["loai"] = "xin_khung_khoa", ["tu"] = k.Ten });
                bao($"PLI từ {k.Ten} -> {mayPhat.Ten}");
            }
            else
            {
                pliBo++;
                bao($"PLI từ {k.Ten}: bỏ (lọc 500 ms)");
            }
        }
    }

    // ------------------------------------------------------------ UDP: nhận và nhân bản
    void NhanUdp(Socket sock)
    {
        var buf = new byte[2048];
        double moc = Now;
        while (chay)
        {
            byte[]? goi = null;
            EndPoint from = new IPEndPoint(IPAddress.Any, 0);
            try
            {
                int n = sock.ReceiveFrom(buf, ref from);
                goi = buf.AsSpan(0, n).ToArray();
            }
            catch (SocketException) when (chay) { }                  // hết 0,5 s chờ (hoặc lỗi tạm): làm việc định kỳ
            catch (Exception e) when (e is SocketException or ObjectDisposedException) { break; }
            double bayGio = Now;
            GuiBoDemCho(sock);                                       // bộ đệm GOP đi TRƯỚC mọi gói trực tiếp
            if (goi is { Length: > 0 })
            {
                List<(byte[] G, IPEndPoint Dich)> ra;
                lock (Khoa) ra = XuLyGoi(goi, (IPEndPoint)from, bayGio);
                foreach (var (g, dich) in ra)
                {
                    try { sock.SendTo(g, dich); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
                }
            }
            if (bayGio - moc >= 1.0)
            {
                moc += 1.0;
                if (bayGio - moc > 1.0) moc = bayGio;
                lock (Khoa)
                {
                    (kbpsVaoAm, kbpsVaoHinh) = (demVaoAm * 8 / 1000.0, demVaoHinh * 8 / 1000.0);
                    kbpsRa = demRa * 8 / 1000.0;
                    demVaoAm = demVaoHinh = demRa = 0;
                    foreach (var k in ds.Values) (k.Kbps, k.DemByte) = (k.DemByte * 8 / 1000.0, 0);
                }
            }
        }
    }

    /// <summary>Gửi toàn bộ bộ đệm GOP cho người vừa có địa chỉ UDP / vừa bật lại video.
    /// Nghỉ 1 ms mỗi 32 gói để không làm tràn bộ đệm nhận của người xem.</summary>
    void GuiBoDemCho(Socket sock)
    {
        List<Khach> dsK;
        List<byte[]> goi;
        long? soI;
        lock (Khoa)
        {
            dsK = DsNguoiXem().Where(k => k.CanBoDem && k.Udp != null).ToList();
            if (dsK.Count == 0) return;
            goi = BoDemBat ? [.. boDem] : [];
            soI = boDemSoI;
            foreach (var k in dsK) k.CanBoDem = false;
        }
        if (goi.Count == 0) return;
        foreach (var k in dsK)
        {
            for (int n = 0; n < goi.Count; n++)
            {
                try { sock.SendTo(goi[n], k.Udp!); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
                if (n % 32 == 31) Thread.Sleep(1);
            }
            lock (Khoa)
            {
                long b = goi.Sum(g => (long)g.Length);
                k.GoiRa += goi.Count;
                k.ByteRa += b;
                k.DemByte += b;
                goiRa += goi.Count;
                byteRa += b;
                demRa += b;
                int soP = goi.Where(g => g[1] == LoaiP).Select(g => Convert.ToHexString(g, 4, 4)).Distinct().Count();
                bao($"{k.Ten}: gửi bộ đệm GOP {goi.Count} gói (I #{soI} + {soP} P)");
                if (k.KhungDauMs == null)
                {
                    k.KhungDauMs = (Now - k.LucMoc) * 1000;
                    k.NguonDau = "bộ đệm";
                    bao($"{k.Ten}: khung I đầu sau {k.KhungDauMs:F0} ms (bộ đệm)");
                }
            }
        }
    }

    /// <summary>Xử lý một datagram (đang giữ khóa). Trả về danh sách (gói, đích) cần gửi.</summary>
    List<(byte[], IPEndPoint)> XuLyGoi(byte[] goi, IPEndPoint addr, double bayGio)
    {
        var ra = new List<(byte[], IPEndPoint)>();
        byte b0 = goi[0];
        bool video;
        Header? h = null;
        if (b0 == PhienBan)
        {
            h = TachHeader(goi);
            if (h == null) return ra;
            if (h.Loai == LoaiGiu)
            {
                if (ds.TryGetValue(h.Ma, out var k) && addr.Address.Equals(k.Ip) && !addr.Equals(k.Udp))
                {
                    bool lanDau = k.Udp == null;
                    k.Udp = addr;
                    bao($"#{k.Ma} {k.Ten} đăng ký UDP {addr.Address}:{addr.Port}");
                    if (lanDau && k.VaiTro == "xem" && k.Video && BoDemBat) k.CanBoDem = true;
                }
                return ra;
            }
            video = true;
        }
        else if (b0 == DauRtp) video = false;
        else return ra;
        if (mayPhat == null || !addr.Equals(mayPhat.Udp) || !dangPhat)
        {
            SaiNguon++;                                              // chỉ nhận phương tiện từ máy phát
            return ra;
        }
        goiVao++;
        byteVao += goi.Length;
        if (video)
        {
            demVaoHinh += goi.Length;
            CapNhatBoDem(h!.Loai, h.So, goi);
            if (XemTruoc) GiaiMaXemTruoc(h, bayGio);
        }
        else demVaoAm += goi.Length;
        foreach (var k in DsNguoiXem())
        {
            if (k.Udp == null || k.CanBoDem || !(video ? k.Video : k.AmThanh)) continue;
            ra.Add((goi, k.Udp));
            k.GoiRa++;
            k.ByteRa += goi.Length;
            k.DemByte += goi.Length;
            if (video && h!.Loai == LoaiI && k.KhungDauMs == null)
            {
                k.KhungDauMs = (bayGio - k.LucMoc) * 1000;
                k.NguonDau = "trực tiếp";
                bao($"{k.Ten}: khung I đầu sau {k.KhungDauMs:F0} ms (trực tiếp)");
            }
        }
        goiRa += ra.Count;
        byteRa += (long)goi.Length * ra.Count;
        demRa += (long)goi.Length * ra.Count;
        return ra;
    }

    /// <summary>Bộ đệm GOP: mảnh đầu tiên của khung I mới -> xóa; lưu mảnh của I và các P sau nó.</summary>
    void CapNhatBoDem(byte loai, long so, byte[] goi)
    {
        if (!BoDemBat) return;
        if (loai == LoaiI && so != boDemSoI) (boDem, boDemByte, boDemSoI) = ([], 0, so);
        else if (loai != LoaiI && boDemSoI == null) return;          // chưa có I (hoặc vừa tràn): chờ I kế tiếp
        boDem.Add(goi);
        boDemByte += goi.Length;
        if (boDemByte > BoDemGopToiDa)
        {
            (boDem, boDemByte, boDemSoI) = ([], 0, null);
            bao("Bộ đệm GOP quá 4 MB: xóa, chờ khung I kế tiếp");
        }
    }

    /// <summary>Server giải mã MỘT bản chỉ để hiện ảnh xem trước (không ảnh hưởng việc chuyển tiếp).</summary>
    void GiaiMaXemTruoc(Header h, double bayGio)
    {
        var k = ghep.Them(h.Loai, h.So, h.I, h.Tong, h.W, h.H, h.Ts, h.Payload, bayGio);
        if (k == null) return;
        giaiMa.KiemTra(bayGio);
        foreach (var x in giaiMa.Them(k, bayGio)) anhXemTruoc = (x.W, x.H, x.Anh, x.Ts, x.So);
    }

    // ------------------------------------------------------------ lệnh của bảng điều khiển
    public void DatBoDem(bool bat)
    {
        lock (Khoa)
        {
            BoDemBat = bat;
            if (!bat) (boDem, boDemByte, boDemSoI) = ([], 0, null);
        }
        bao("Bộ đệm GOP: " + (bat ? "bật" : "tắt"));
    }

    /// <summary>Đuổi người xem theo tên (hoặc mã). Trả về số người bị ngắt.</summary>
    public int Ngat(string? ten = null, int? ma = null)
    {
        List<Khach> dsK;
        lock (Khoa)
        {
            dsK = DsNguoiXem().Where(k => (ma != null && k.Ma == ma) || k.Ten == ten).ToList();
            foreach (var k in dsK)
            {
                k.Gui(new JsonObject { ["loai"] = "bi_ngat", ["ly_do"] = "server ngắt" });
                Roi(k, "bị server ngắt");
            }
        }
        foreach (var k in dsK)
        {
            try { k.Conn.Shutdown(SocketShutdown.Both); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        }
        return dsK.Count;
    }

    /// <summary>Ảnh chụp trạng thái cho dòng lệnh và giao diện.</summary>
    public TrangThaiMayChu TrangThai()
    {
        lock (Khoa)
        {
            double vao = kbpsVaoAm + kbpsVaoHinh;
            return new(dangPhat, mayPhat?.Ten, thongTin, BoDemBat, boDem.Count, boDemByte, boDemSoI, vao, kbpsVaoAm, kbpsVaoHinh,
                       kbpsRa, vao > 0 ? kbpsRa / vao : 0.0, byteVao, byteRa, goiVao, goiRa, pliChuyen, pliBo,
                       DsNguoiXem().OrderBy(k => k.Ma).Select(k => k.Chup()).ToList(), [.. daRoi], anhXemTruoc);
        }
    }

    // ------------------------------------------------------------ bảng in (dùng chung cho trang_thai và tổng kết)
    /// <summary>Một dòng của bảng người xem (≤ 55 ký tự).</summary>
    public static string DongBang(TomTat x, double? kbps = null)
    {
        string dau = x.KhungDauMs is { } d ? $"{d,4:F0} ms {x.NguonDau}" : "–";
        string ten = x.Ten.Length > 6 ? x.Ten[..6] : x.Ten;
        return $"{ten,-6} {x.Nhan,-4} {x.GoiRa,6} {x.ByteRa / 1024.0,6:F0} {kbps ?? x.Kbps,6:F0} {dau}";
    }

    public static readonly string TieuDeBang = $"{"tên",-6} {"nhận",-4} {"gói ra",6} {"KB ra",6} {"kbit/s",6} khung I đầu";

    /// <summary>Các dòng in cho lệnh trang_thai.</summary>
    public static List<string> BangTrangThai(TrangThaiMayChu tt)
    {
        var ds = new List<string>
        {
            $"Trạng thái: {(tt.DangPhat ? tt.TenPhat + " đang phát" : "chưa phát")}, {tt.NguoiXem.Count} người xem",
            $"bộ đệm GOP {(tt.BoDemBat ? "bật" : "tắt")}: {tt.BoDemGoi} gói, {tt.BoDemByte / 1024.0:F0} KB",
            $"vào {tt.KbpsVao:F0} kbit/s (tiếng {tt.KbpsVaoAm:F0}, hình {tt.KbpsVaoHinh:F0})",
            $"ra {tt.KbpsRa:F0} kbit/s, hệ số nhân bản {tt.HeSo:F2}",
            $"PLI chuyển {tt.PliChuyen}, bỏ {tt.PliBo}",
            TieuDeBang,
        };
        ds.AddRange(tt.NguoiXem.Select(x => DongBang(x)));
        return ds;
    }

    /// <summary>Các dòng in khi server kết thúc: mọi người xem đã vào (cả người đã rời).</summary>
    public static List<string> BangTongKet(TrangThaiMayChu tt)
    {
        var dsX = tt.DaRoi.Concat(tt.NguoiXem).ToList();
        double heSo = tt.ByteVao > 0 ? (double)tt.ByteRa / tt.ByteVao : 0.0;
        var ds = new List<string>
        {
            $"===== TỔNG KẾT: {dsX.Count} người xem =====",
            $"vào {tt.GoiVao} gói {tt.ByteVao / 1024.0:F0} KB, ra {tt.GoiRa} gói {tt.ByteRa / 1024.0:F0} KB",
            $"hệ số nhân bản (byte ra/vào) {heSo:F2}",
            $"PLI chuyển {tt.PliChuyen}, bỏ {tt.PliBo}",
            TieuDeBang,
        };
        ds.AddRange(dsX.Select(x => DongBang(x, x.ByteRa * 8 / 1000.0 / Math.Max(x.Giay, 0.001))));
        ds.Add("(kbit/s = trung bình suốt thời gian xem)");
        return ds;
    }
}

/// <summary>Bản dòng lệnh: máy chủ + lệnh bàn phím (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
///   trang_thai      bo_dem bat|tat      ngat &lt;tên&gt;      thoat</summary>
static class ServerPhat
{
    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int congTcp = a.Length > 1 ? int.Parse(a[1]) : 42650;
        int congUdp = a.Length > 2 ? int.Parse(a[2]) : congTcp + 1;
        var khoaIn = new object();
        void Log(string text)
        {
            lock (khoaIn) Console.WriteLine($"[server] {text}");     // nhiều luồng cùng in: mỗi dòng in trọn vẹn
        }
        var sv = new MayChuPhat(Log);
        sv.BatDau(host, congTcp, congUdp);
        var xong = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>                          // Ctrl+C (công cụ demo dừng server bằng SIGINT)
        {
            e.Cancel = true;
            xong.Set();
        };

        void Lenh(string dong)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            Log($"> {dong}");
            if (p[0] == "trang_thai") foreach (var d in MayChuPhat.BangTrangThai(sv.TrangThai())) Log(d);
            else if (p[0] == "bo_dem" && p.Length == 2 && p[1] is "bat" or "tat") sv.DatBoDem(p[1] == "bat");
            else if (p[0] == "ngat" && p.Length == 2) Log($"Đã ngắt {sv.Ngat(ten: p[1])} người xem");
            else if (p[0] == "thoat") xong.Set();
            else Log("lệnh không hợp lệ");
        }

        double t0 = Now;
        new Thread(() => DocLenhStdin(t0, Lenh)) { IsBackground = true }.Start();
        xong.Wait();
        foreach (var d in MayChuPhat.BangTongKet(sv.TrangThai())) Log(d);
        sv.Dung();
        return 0;
    }
}
