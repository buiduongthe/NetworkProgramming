using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR6_5.Chung;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Lõi người xem phát trực tiếp, dạng lớp, không chạm giao diện.
//
// Dùng chung cho bản dòng lệnh (lớp Xem cuối tệp) và bản giao diện (GiaoDien/FormXem.cs). Bốn luồng, chung một khóa:
//   1. luồng đọc TCP: thong_tin, dong_ho (báo cáo đồng hồ), su_kien, ket_thuc, bi_ngat;
//   2. luồng nhận UDP: RTP -> bộ đệm jitter; mảnh video -> GhepKhung -> GiaiMaVideo -> hàng hiển thị;
//   3. luồng phát 20 ms = ĐỒNG HỒ CHỦ: lấy một khung âm thanh, đưa ra loa, cập nhật dongHoAm = ts/16 (ms phương
//      tiện) và giờ thực lúc phát; ghi lịch sử ~3 s để đo lệch;
//   4. luồng hiển thị 5 ms: kiểm tra mất tham chiếu (xin khung khóa), chọn khung để hiện.
//
// Đồng bộ tiếng-hình (lip-sync) – âm thanh làm đồng hồ chủ (như trình phát thật: tai nhạy với tiếng bị ngắt hơn mắt
// nhạy với một khung hình hiện sớm/muộn):
//   - BẬT: chỉ hiện khung mới nhất có nhãn ≤ đồng hồ phát hiện tại dongHoAm + (bây giờ − lúc phát khung âm thanh
//     đó); khung quá cũ bị bỏ.
//   - TẮT (hoặc chỉ nhận video): hiện ngay khi giải mã xong -> hình đi TRƯỚC tiếng đúng bằng thời gian âm thanh
//     nằm trong bộ đệm jitter (độ sâu × 20 ms).
// Lệch tiếng-hình của một khung = giờ thực lúc hiện − giờ thực lúc âm thanh cùng nhãn được phát (nội suy trên lưới
// 20 ms của lịch sử phát); dương = hình chậm hơn tiếng, âm = hình đi trước.
// Độ trễ đầu-cuối (âm thanh) = giờ Unix lúc phát − (gio + (ts_ms − ts)) theo báo cáo đồng hồ mới nhất – chỉ đúng
// khi máy phát và người xem cùng đồng hồ (cùng máy hoặc đồng bộ NTP).

/// <summary>Khung đang hiện: ảnh, nhãn hình (ms), số khung, đồng hồ tiếng lúc hiện (ms) nếu có.</summary>
record KhungHien(int W, int H, byte[] Anh, long Ts, long So, double? Am);

/// <summary>Một giai đoạn bật/tắt đồng bộ: tổng lệch và số khung để tính trung bình.</summary>
sealed class GiaiDoan(bool dongBo)
{
    public readonly bool DongBo = dongBo;
    public double Tong;
    public int N;
    public GiaiDoan Sao() => new(DongBo) { Tong = Tong, N = N };
}

record TrangThaiNguoiXem(bool DaKetNoi, int? Ma, JsonObject? ThongTin, long GoiAm, long MatAm, double? TreTb, double? TreHien,
                         int KhungI, int KhungP, int KhungHien, int Hong, int BoP, int BoCu, int DungHinh, int PliGui,
                         double? KhungDauMs, string NguonDau, double? LechHien, List<GiaiDoan> GiaiDoan, double Kbps,
                         bool DongBo, bool AmThanh, bool Video, int DoSau, double MucAm, long SoHien, KhungHien? Hien,
                         int BoGiaLap);

sealed class NguoiXem
{
    const double MsMoiMau = 1000.0 / TanSoMau;
    public readonly string Ten;
    readonly Action<string> bao;
    public readonly object Khoa = new();
    readonly object khoaTcp = new();
    Socket? tcp, udp;
    public volatile bool DaKetNoi;
    public int? Ma;
    int doSau;
    bool dongBo, amThanh, video;
    readonly double matGoi;
    Random rng = new(0);
    int boGiaLap;
    public volatile ILoa? Loa;                       // đối tượng có Phat(mẫu) hoặc null (không phát)
    JsonObject? thongTin;
    volatile float mucAm = float.NegativeInfinity;
    readonly List<GiaiDoan> giaiDoan;
    volatile bool song = true;

    // thống kê
    double lucChao;
    double lucMoc;                                    // mốc đo khung đầu: lúc vào, hoặc lúc máy phát
    bool choPhat;                                     // bắt đầu phát nếu khi vào chưa có ai phát
    long goiAm, byteNhan;
    int khungI, khungP, khungHien, dungHinh, pliGui, boCu;
    long matCu;
    int hongCu, boPCu;                                // số đếm của các phiên phát trước
    double tongTre;
    int nTre;
    double? treHien, lechHien;
    double? khungDauMs;
    string nguonDau = "";
    (double? Cu, double Den)? iDau;
    double? choHinhLai;
    double lucPli;
    bool dangDung;
    int wAm, wHien;
    long wByte;
    List<double> wTre = [], wLech = [];

    // một phiên phát (làm mới khi máy phát dừng phát)
    BoDemJitter boDem = null!;
    GhepKhung ghep = null!;
    GiaiMaVideo giaiMa = null!;
    LinkedList<GiaiMaVideo.KhungGiai> hang = null!;
    Queue<(double TsMs, double Gio)> lichSu = null!;   // (ts ms, giờ đơn điệu) ~3 s gần nhất
    double? dongHoAm, lucPhatAm, lucNhanAm;
    double? tsAmMoi, tsHinhMoi;
    (double Ts, double Gio)? dongHoBao;               // báo cáo đồng hồ mới nhất
    KhungHien? hien;
    long soHien;

    public NguoiXem(string ten, Action<string>? bao = null, int doSau = DoSauMacDinh, bool dongBo = true, bool amThanh = true,
                    bool video = true, double matGoi = 0.0, ILoa? loa = null)
    {
        (Ten, this.bao, this.doSau, this.dongBo, this.amThanh, this.video, this.matGoi, Loa) =
            (ten, bao ?? (_ => { }), doSau, dongBo, amThanh, video, matGoi, loa);
        giaiDoan = [new GiaiDoan(dongBo)];
        LamMoiThongKe();
        LamMoiPhien();
        new Thread(VongAm) { IsBackground = true, Name = "vong-am" }.Start();
        new Thread(VongHien) { IsBackground = true, Name = "vong-hien" }.Start();
    }

    void LamMoiThongKe()
    {
        lucChao = Now;
        lucMoc = lucChao;
        choPhat = false;
        goiAm = byteNhan = 0;
        khungI = khungP = khungHien = dungHinh = pliGui = boCu = 0;
        matCu = 0;
        hongCu = boPCu = 0;
        tongTre = 0;
        nTre = 0;
        treHien = lechHien = null;
        (khungDauMs, nguonDau, iDau) = (null, "", null);
        choHinhLai = null;
        lucPli = -1e9;
        dangDung = false;
        wAm = wHien = 0;
        wByte = 0;
        (wTre, wLech) = ([], []);
    }

    /// <summary>Trạng thái của một phiên phát (làm mới khi máy phát dừng phát).</summary>
    void LamMoiPhien()
    {
        if (boDem != null)
        {
            matCu += boDem.Mat;
            hongCu += ghep.KhungHong;
            boPCu += giaiMa.BoP;
        }
        boDem = new BoDemJitter(doSau);
        (ghep, giaiMa) = (new(), new());
        hang = new();
        lichSu = new();
        dongHoAm = lucPhatAm = lucNhanAm = null;
        tsAmMoi = tsHinhMoi = null;
        dongHoBao = null;
        hien = null;
        soHien = 0;
    }

    // ------------------------------------------------------------ kết nối
    public int KetNoi(string host, int cong)
    {
        var (t, ip) = KetNoiTcp(host, cong);
        Socket u;
        DocDong doc;
        JsonObject tl;
        try
        {
            doc = new DocDong(new NetworkStream(t));
            GuiJson(t, new JsonObject { ["loai"] = "chao", ["vai_tro"] = "xem", ["ten"] = Ten });
            tl = doc.Doc() ?? [];
            if (Chuoi(tl, "loai") == "loi") throw new IOException($"server từ chối: {Chuoi(tl, "ly_do")}");
            if (Chuoi(tl, "loai") != "chao") throw new IOException("server không trả lời 'chao'");
            u = new Socket(ip.AddressFamily, SocketType.Dgram, ProtocolType.Udp) { ReceiveBufferSize = 1 << 20 };
            TatLoiUdpReset(u);
            u.Connect(ip, (int)(So(tl, "cong_udp") ?? throw new IOException("thiếu cong_udp")));   // chỉ nhận datagram từ server
        }
        catch (Exception e) when (e is SocketException or IOException or InvalidDataException)
        {
            t.Close();
            throw new IOException(e.Message);
        }
        int ma = (int)(So(tl, "ma") ?? 0);
        lock (Khoa)
        {
            (tcp, udp, Ma) = (t, u, ma);
            rng = new Random(ma);
            LamMoiPhien();
            LamMoiThongKe();
            choPhat = Bool(tl, "phat") != true;
            DaKetNoi = true;
        }
        if (!(amThanh && video)) Gui(new JsonObject { ["loai"] = "dang_ky", ["am_thanh"] = amThanh, ["video"] = video });
        u.Send(GoiGiu(ma));                                          // đăng ký địa chỉ UDP
        bao($"Vào xem: mã {ma}, " + (Bool(tl, "phat") == true ? "máy phát đang phát" : "chưa có ai phát"));
        new Thread(() => DocTcp(doc)) { IsBackground = true, Name = "doc-tcp" }.Start();
        new Thread(() => NhanUdp(u)) { IsBackground = true, Name = "nhan-udp" }.Start();
        return ma;
    }

    public void Gui(JsonObject obj)
    {
        var t = tcp;
        if (t == null) return;
        try { lock (khoaTcp) GuiJson(t, obj); }
        catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
    }

    public void Ngat()
    {
        Socket? t, u;
        lock (Khoa)
        {
            (t, u) = (tcp, udp);
            (tcp, udp) = (null, null);
            DaKetNoi = false;
        }
        try { t?.Shutdown(SocketShutdown.Both); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        t?.Close();
        u?.Close();
    }

    static string Gt(JsonObject o, string k) => o[k]?.ToJsonString() ?? "None";

    void DocTcp(DocDong doc)
    {
        try
        {
            while (true)
            {
                var tin = doc.Doc();
                if (tin == null) break;
                string? loai = Chuoi(tin, "loai");
                if (loai == "thong_tin")
                {
                    lock (Khoa)
                    {
                        thongTin = tin;
                        if (choPhat)                                         // vào trước khi phát: đo từ lúc bắt đầu phát
                        {
                            choPhat = false;
                            if (khungDauMs == null) lucMoc = Now;
                        }
                    }
                    bao($"Luồng {Gt(tin, "rong")}x{Gt(tin, "cao")}, {Gt(tin, "fps")} fps, GOP {Gt(tin, "gop")}, {Gt(tin, "tan_so_mau")} Hz");
                }
                else if (loai == "dong_ho")
                {
                    if (So(tin, "ts") is { } ts && So(tin, "gio") is { } gio)
                        lock (Khoa) dongHoBao = (ts, gio);
                }
                else if (loai == "su_kien") bao($"Sự kiện: {Chuoi(tin, "noi_dung")}");
                else if (loai == "ket_thuc")
                {
                    lock (Khoa)
                    {
                        thongTin = null;
                        choPhat = true;
                        LamMoiPhien();
                    }
                    bao("Máy phát kết thúc phát");
                }
                else if (loai == "bi_ngat") bao($"Bị ngắt: {Chuoi(tin, "ly_do")}");
            }
        }
        catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ObjectDisposedException) { }
        if (DaKetNoi)
        {
            bao("Mất kết nối server");
            Ngat();
        }
    }

    // ------------------------------------------------------------ luồng nhận UDP
    /// <summary>Ước lượng ms phương tiện hiện tại (để biết khung I đầu tiên "cũ" bao nhiêu).</summary>
    double? UocLuongTs()
    {
        if (dongHoBao is { } d) return d.Ts + (GioUnixMs - d.Gio);               // giả định cùng đồng hồ
        var moi = new[] { tsHinhMoi, tsAmMoi * MsMoiMau }.Where(x => x is { } v && v != 0).ToList();
        return moi.Count > 0 ? moi.Max() : null;
    }

    void NhanUdp(Socket u)
    {
        var buf = new byte[2048];
        while (true)
        {
            int n;
            try { n = u.Receive(buf); }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionReset && DaKetNoi) { continue; }
            catch (Exception e) when (e is SocketException or ObjectDisposedException) { return; }
            if (n == 0) continue;
            if (matGoi > 0 && rng.NextDouble() < matGoi)
            {
                boGiaLap++;                                                  // "mạng" làm rơi gói ở chiều xuống
                continue;
            }
            double bayGio = Now;
            var goi = buf.AsSpan(0, n);
            lock (Khoa)
            {
                byteNhan += n;
                wByte += n;
                if (goi[0] == DauRtp)
                {
                    var r = TachRtp(goi);
                    if (r is { } rr && rr.Pt == PtAmThanh && rr.Payload.Length == ByteMoiKhung && amThanh)
                    {
                        boDem.NhanGoi(rr.Seq, rr.Ts, rr.Payload);
                        (lucNhanAm, tsAmMoi) = (bayGio, rr.Ts);
                        goiAm++;
                        wAm++;
                    }
                }
                else if (goi[0] == PhienBan && video)
                {
                    var h = TachHeader(goi);
                    if (h == null || h.Loai is not (LoaiI or LoaiP)) continue;
                    var k = ghep.Them(h.Loai, h.So, h.I, h.Tong, h.W, h.H, h.Ts, h.Payload, bayGio);
                    if (k != null) NhanKhung(k, bayGio);
                }
            }
        }
    }

    /// <summary>Khung vừa đủ mảnh -> giải mã (có thể giải thêm các khung đang chờ) -> hàng hiển thị.</summary>
    void NhanKhung(GhepKhung.Khung k, double bayGio)
    {
        foreach (var kh in giaiMa.Them(k, bayGio))
        {
            tsHinhMoi = Math.Max(tsHinhMoi ?? 0, kh.Ts);
            if (kh.Loai == LoaiI)
            {
                khungI++;
                if (iDau == null)
                {
                    var uoc = UocLuongTs();
                    iDau = (uoc - kh.Ts, (bayGio - lucMoc) * 1000);
                }
                if (dangDung)
                {
                    dangDung = false;
                    double ms = giaiMa.LucMat is { } lm ? (bayGio - lm) * 1000 : 0;
                    bao($"Hồi phục bằng khung I #{kh.So} sau {ms:F0} ms");
                }
            }
            else khungP++;
            hang.AddLast(kh);
            if (hang.Count > HangHienThi)
            {
                hang.RemoveFirst();
                boCu++;
            }
        }
    }

    // ------------------------------------------------------------ luồng phát 20 ms (đồng hồ chủ)
    void VongAm()
    {
        double keTiep = Now;
        long n = 0;
        var imLang = new short[MauMoiKhung];
        while (song)
        {
            n++;
            short[]? mau = null;
            Socket? u;
            lock (Khoa)
            {
                u = udp;
                var r = DaKetNoi && amThanh ? boDem.Lay() : null;
                double bayGio = Now;
                if (r is { } rr)
                {
                    double tsMs = rr.Ts * MsMoiMau;
                    (dongHoAm, lucPhatAm) = (tsMs, bayGio);
                    lichSu.Enqueue((tsMs, bayGio));
                    while (lichSu.Count > 150) lichSu.Dequeue();
                    if (rr.P == null) mau = imLang;                              // mất gói: phát im lặng
                    else
                    {
                        mau = ByteSangPcm(rr.P);
                        if (dongHoBao is { } d)                                  // trễ đầu-cuối (cùng đồng hồ)
                        {
                            double tre = GioUnixMs - (d.Gio + (tsMs - d.Ts));
                            treHien = tre;
                            tongTre += tre;
                            nTre++;
                            wTre.Add(tre);
                        }
                    }
                }
            }
            mucAm = mau != null ? (float)MucDbfs(mau) : float.NegativeInfinity;
            Loa?.Phat(mau ?? imLang);
            if (u != null && n % 50 == 0)
            {
                try { u.Send(GoiGiu(Ma ?? 0)); }                                  // giữ kết nối mỗi 1 s
                catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
            }
            keTiep += KhungMs / 1000.0;
            double con = keTiep - Now;
            if (con < -0.1) keTiep = Now;                                         // tụt quá xa: bắt nhịp lại
            else if (con > 0) Ngu(con);
        }
    }

    // ------------------------------------------------------------ luồng hiển thị 5 ms
    void VongHien()
    {
        while (song)
        {
            Thread.Sleep(5);
            string? xin = null;
            bool guiPli = false;
            lock (Khoa)
            {
                if (!DaKetNoi) continue;
                double bayGio = Now;
                var gm = giaiMa;
                long? thieu = gm.ThamChieu is { } tc && gm.Doi.Count > 0 ? tc.So + 1 : null;
                bool duocXin = bayGio - lucPli >= PliXemS;                      // giới hạn 1 lần / 1 s
                if (video && gm.KiemTra(bayGio))
                {
                    dungHinh++;
                    dangDung = true;
                    xin = $"Thiếu khung #{thieu} -> đứng hình" + (duocXin ? ", xin khung khóa" : " (xin sau giới hạn 1 s)");
                    guiPli = duocXin;
                }
                else if (video && gm.MatThamChieu && duocXin)
                {
                    xin = "Chưa có khung I: xin khung khóa";                     // bị giới hạn lúc trước / I bị mất
                    guiPli = true;
                }
                if (guiPli)
                {
                    lucPli = bayGio;
                    pliGui++;
                }
                ghep.DonDep(bayGio);
                var chon = ChonKhung(bayGio);
                if (chon != null) Hien(chon, bayGio);
            }
            if (xin != null)
            {
                bao(xin);
                if (guiPli) Gui(new JsonObject { ["loai"] = "xin_khung_khoa" });
            }
        }
    }

    /// <summary>Đồng hồ phát âm thanh hiện tại (ms phương tiện) hoặc null nếu tiếng không chạy.</summary>
    double? DongHoHienTai(double bayGio)
    {
        if (lucPhatAm is not { } lp || bayGio - lp > 0.2) return null;
        return dongHoAm + Math.Min(bayGio - lp, 0.04) * 1000;
    }

    GiaiMaVideo.KhungGiai? ChonKhung(double bayGio)
    {
        if (hang.Count == 0) return null;
        if (!video)
        {
            hang.Clear();
            return null;
        }
        // Tiếng "đang chạy": vừa nhận gói tiếng trong 0,5 s, hoặc mới vào xem chưa tới 0,5 s (chờ tiếng tới để khung
        // đầu cũng được đồng bộ). Không có tiếng -> hiện ngay, không chờ mãi.
        bool tiengChay = lucNhanAm is { } ln ? amThanh && bayGio - ln < 0.5 : amThanh && bayGio - lucChao < 0.5;
        if (dongBo && tiengChay)
        {
            if (DongHoHienTai(bayGio) is not { } am) return null;              // chờ đồng hồ tiếng (bộ đệm jitter đang nạp)
            GiaiMaVideo.KhungGiai? c = null;
            while (hang.Count > 0 && hang.First!.Value.Ts <= am)
            {
                if (c != null) boCu++;                                          // khung quá cũ: bỏ
                c = hang.First.Value;
                hang.RemoveFirst();
            }
            return c;
        }
        var chon = hang.Last!.Value;                                            // không đồng bộ: hiện ngay khung mới nhất
        boCu += hang.Count - 1;
        hang.Clear();
        return chon;
    }

    /// <summary>Giờ (đơn điệu) mà âm thanh có nhãn tsMs được/sẽ được phát – nội suy theo lịch sử phát.</summary>
    double? GioAm(double tsMs)
    {
        foreach (var (tAm, gio) in lichSu.Reverse())
            if (tAm <= tsMs) return gio + (tsMs - tAm) / 1000;
        return null;
    }

    void Hien(GiaiMaVideo.KhungGiai k, double bayGio)
    {
        hien = new KhungHien(k.W, k.H, k.Anh, k.Ts, k.So, DongHoHienTai(bayGio));
        soHien++;
        khungHien++;
        wHien++;
        if (khungDauMs == null)
        {
            khungDauMs = (bayGio - lucMoc) * 1000;
            // Nguồn khung đầu: khung I đã "cũ" (chụp trước lúc vào > 1,5 chu kỳ khung) -> từ bộ đệm GOP;
            // I mới nhưng tới muộn -> phải chờ khung I; I mới tới ngay -> vừa đúng lúc có khung I.
            var (cu, den) = iDau is { } i ? (i.Cu, (double?)i.Den) : (null, null);
            double fps = thongTin != null && So(thongTin, "fps") is { } f && f > 0 ? f : 10;
            double nguong = 1500 / fps;
            string them = "";
            if (cu is { } c && c > nguong) (nguonDau, them) = ("bộ đệm GOP", $" (I cũ {c / 1000:F1} s)");
            else if (den is { } d && d > nguong) nguonDau = "chờ khung I";
            else nguonDau = "khung I trực tiếp";
            string moc = lucMoc > lucChao ? " (từ lúc phát)" : "";
            bao($"Khung đầu sau {khungDauMs:F0} ms{moc}: {nguonDau}{them}");
        }
        if (choHinhLai is { } chl)
        {
            bao($"Hình trở lại sau {(bayGio - chl) * 1000:F0} ms");
            choHinhLai = null;
        }
        bool tiengChay = amThanh && lucPhatAm is { } lp && bayGio - lp < 0.5;
        if (tiengChay && GioAm(k.Ts) is { } gioAm)
        {
            double lech = (bayGio - gioAm) * 1000;
            lechHien = lech;
            giaiDoan[^1].Tong += lech;
            giaiDoan[^1].N++;
            wLech.Add(lech);
        }
        else lechHien = null;
    }

    // ------------------------------------------------------------ điều khiển
    public void DatDongBo(bool bat)
    {
        lock (Khoa)
        {
            if (bat != dongBo)
            {
                dongBo = bat;
                giaiDoan.Add(new GiaiDoan(bat));
            }
        }
        bao("Đồng bộ tiếng-hình: " + (bat ? "BẬT" : "TẮT"));
    }

    public void DangKy(bool am, bool vid)
    {
        lock (Khoa)
        {
            if (!vid && video)
            {
                ghep.BoDangDo();
                hang.Clear();
            }
            if (vid && !video) choHinhLai = Now;
            if (!am && amThanh)
            {
                boDem.Xoa();
                lichSu.Clear();
                lucPhatAm = lucNhanAm = null;
            }
            if (am && !amThanh)
            {
                matCu += boDem.Mat;
                boDem = new BoDemJitter(doSau);
            }
            (amThanh, video) = (am, vid);
        }
        Gui(new JsonObject { ["loai"] = "dang_ky", ["am_thanh"] = am, ["video"] = vid });
        string ten = am && vid ? "âm thanh+video" : am ? "âm thanh" : "video";
        bao($"Đăng ký nhận: {ten}");
    }

    public void DatDoSau(int n)
    {
        lock (Khoa)
        {
            doSau = Math.Clamp(n, 1, 25);
            boDem.DatDoSau(doSau);
        }
    }

    public bool AmThanh
    {
        get { lock (Khoa) return amThanh; }
    }

    public bool Video
    {
        get { lock (Khoa) return video; }
    }

    public bool DongBo
    {
        get { lock (Khoa) return dongBo; }
    }

    public int DoSau
    {
        get { lock (Khoa) return doSau; }
    }

    public int BoGiaLap => boGiaLap;

    /// <summary>Khung đang hiện và số thứ tự lần hiện (giao diện chỉ vẽ lại khi số đổi).</summary>
    public (KhungHien? Hien, long So) LayHien()
    {
        lock (Khoa) return (hien, soHien);
    }

    /// <summary>(khung đang hiện, lệch ms|null) hoặc null nếu chưa có hình.</summary>
    public (KhungHien K, double? Lech)? Chup()
    {
        lock (Khoa) return hien == null ? null : (hien, lechHien);
    }

    // ------------------------------------------------------------ số liệu
    /// <summary>Gọi mỗi 1 s: số liệu của giây vừa qua (rồi xóa bộ đếm cửa sổ).</summary>
    public (int Am, long Mat, int Hien, double? Tre, double? Lech, double Kbps) ThongKeGiay()
    {
        lock (Khoa)
        {
            var r = (wAm, matCu + boDem.Mat, wHien, wTre.Count > 0 ? wTre.Average() : (double?)null,
                     wLech.Count > 0 ? wLech.Average() : (double?)null, wByte * 8 / 1000.0);
            wAm = wHien = 0;
            wByte = 0;
            (wTre, wLech) = ([], []);
            return r;
        }
    }

    public TrangThaiNguoiXem TrangThai()
    {
        lock (Khoa)
        {
            double giay = Math.Max(Now - lucChao, 0.001);
            return new(DaKetNoi, Ma, thongTin, goiAm, matCu + boDem.Mat, nTre > 0 ? tongTre / nTre : null, treHien, khungI, khungP,
                       khungHien, hongCu + ghep.KhungHong, boPCu + giaiMa.BoP, boCu, dungHinh, pliGui, khungDauMs, nguonDau,
                       lechHien, giaiDoan.Select(g => g.Sao()).ToList(), byteNhan * 8 / 1000.0 / giay, dongBo, amThanh, video,
                       doSau, mucAm, soHien, hien, boGiaLap);
        }
    }

    public void Dong()
    {
        song = false;
        Ngat();
        Loa?.Dispose();
    }

    /// <summary>Các dòng TỔNG KẾT của người xem (dùng chung cho dòng lệnh).</summary>
    public static List<string> DongTongKet(TrangThaiNguoiXem tt, double treVao)
    {
        var ds = new List<string>
        {
            "===== TỔNG KẾT =====",
            (treVao > 0 ? $"vào muộn {treVao:F1} s; " : "vào ngay; ")
            + (tt.KhungDauMs is { } kd ? $"khung đầu sau {kd:F0} ms" : "chưa có khung nào"),
            $"  nguồn khung đầu: {(tt.NguonDau != "" ? tt.NguonDau : "–")}",
            $"tiếng: nhận {tt.GoiAm} gói, mất {tt.MatAm}, trễ TB {ChuMs(tt.TreTb)} ms",
            "  (trễ đầu-cuối: giả định cùng đồng hồ)",
            $"hình: nhận {tt.KhungI + tt.KhungP} khung (I {tt.KhungI}, P {tt.KhungP}), hỏng {tt.Hong}",
            $"  P bỏ chờ I {tt.BoP}; hiện {tt.KhungHien}, bỏ cũ {tt.BoCu}",
            $"đứng hình {tt.DungHinh}, xin khung khóa {tt.PliGui}",
            "lệch tiếng-hình TB (+ = hình chậm hơn tiếng):",
        };
        for (int i = 0; i < tt.GiaiDoan.Count; i++)
        {
            var g = tt.GiaiDoan[i];
            string bt = g.DongBo ? "BẬT" : "TẮT";
            ds.Add(g.N > 0 ? $"  {i + 1}. đồng bộ {bt}: {ChuMs(g.Tong / g.N, true)} ms (n={g.N})"
                           : $"  {i + 1}. đồng bộ {bt}: – (không có khung)");
        }
        ds.Add($"nhận TB {tt.Kbps:F0} kbit/s");
        return ds;
    }

    /// <summary>Ảnh chụp có chú thích: khung đang hiện phóng ×2 + dải chữ nhãn hình, đồng hồ tiếng, lệch.
    /// Trả về (rộng, cao, rgb).</summary>
    public static (int W, int H, byte[] Rgb) AnhChup(KhungHien k, double? lech)
    {
        int W = 2 * k.W, H = 2 * k.H + 62;
        var ra = new byte[W * H * 3];
        PhongTo(k.Anh, k.W, k.H, 2).CopyTo(ra, 0);
        ToMau(ra, W, 0, 2 * k.H, W, 62, (20, 20, 20));
        VeChu(ra, W, H, 6, 2 * k.H + 6, $"HINH  T={k.Ts / 1000.0:F3} #{k.So}", 2);
        VeChu(ra, W, H, 6, 2 * k.H + 24, "TIENG " + (k.Am is { } am ? $"T={am / 1000:F3}" : "-"), 2);
        string chu;
        (byte, byte, byte) mau;
        if (lech is not { } l) (chu, mau) = ("LECH -", (200, 200, 200));
        else
        {
            chu = $"LECH {ChuMs(l, true)} MS" + (l < -40 ? " HINH SOM" : l > 40 ? " HINH TRE" : "");
            mau = Math.Abs(l) <= 40 ? ((byte)120, (byte)255, (byte)120) : ((byte)255, (byte)120, (byte)120);
        }
        VeChu(ra, W, H, 6, 2 * k.H + 42, chu, 2, mau);
        ToMau(ra, W, 0, 2 * k.H, W, 2, (255, 255, 0));
        return (W, H, ra);
    }
}

/// <summary>Người xem phát trực tiếp (bản dòng lệnh). Mỗi giây in: số gói tiếng nhận, tổng gói mất, trễ đầu-cuối TB,
/// số khung hiện và lệch tiếng-hình TB trong giây đó; cuối cùng in TỔNG KẾT. Không phát ra loa (bản giao diện mới
/// phát loa) – đồng hồ phát vẫn chạy đúng nhịp 20 ms.
/// Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy):
///   chup &lt;nhãn&gt;   dong_bo bat|tat   dang_ky am_thanh|video|ca_hai   thoat</summary>
static class Xem
{
    public static int Run(string[] a)
    {
        string ten = a[0], host = a[1];
        int cong = int.Parse(a[2]);
        double thoiGian = 10.0, tre = 0.0, matGoi = 0.0;
        int dem = 5;
        bool khongDongBo = false, chiAm = false, chiVideo = false;
        string? ghep = null;
        for (int i = 3; i < a.Length; i++)
        {
            string V() => i + 1 < a.Length ? a[++i] : throw new ArgumentException($"thiếu giá trị cho {a[i]}");
            double D() => double.Parse(V(), CultureInfo.InvariantCulture);
            switch (a[i])
            {
                case "--thoi-gian": thoiGian = D(); break;
                case "--tre": tre = D(); break;
                case "--dem": dem = int.Parse(V()); break;
                case "--khong-dong-bo": khongDongBo = true; break;
                case "--chi-am-thanh": chiAm = true; break;
                case "--chi-video": chiVideo = true; break;
                case "--mat-goi": matGoi = D(); break;
                case "--ghep": ghep = V(); break;
                default: Console.WriteLine($"Tùy chọn không hợp lệ: {a[i]}"); return 2;
            }
        }
        var khoaIn = new object();
        void Log(string text)
        {
            lock (khoaIn) Console.WriteLine($"[xem {ten}] {text}");      // nhiều luồng cùng in: mỗi dòng in trọn vẹn
        }
        double t0 = Now;
        var nx = new NguoiXem(ten, Log, Math.Clamp(dem, 1, 25), !khongDongBo, !chiVideo, !chiAm, matGoi);
        var daChup = new List<(int W, int H, byte[] Rgb, string Nhan)>();
        var xong = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            xong.Set();
        };

        void Lenh(string dong)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            Log($"> {dong}");
            if (p[0] == "dong_bo" && p.Length == 2 && p[1] is "bat" or "tat") nx.DatDongBo(p[1] == "bat");
            else if (p[0] == "dang_ky" && p.Length == 2 && p[1] is "am_thanh" or "video" or "ca_hai") nx.DangKy(p[1] != "video", p[1] != "am_thanh");
            else if (p[0] == "chup")
            {
                if (nx.Chup() is not { } c)
                {
                    Log("chưa có hình để chụp");
                    return;
                }
                var (w, h, rgb) = NguoiXem.AnhChup(c.K, c.Lech);
                Directory.CreateDirectory("out");
                string duong = $"out/{ten}_chup_{daChup.Count + 1}.png";
                GhiPng(duong, rgb, w, h);
                daChup.Add((w, h, rgb, p.Length > 1 ? string.Join(' ', p[1..]) : duong));
                Log($"Đã chụp {duong}: hình {c.K.Ts / 1000.0:F3} s, tiếng " + (c.K.Am is { } am ? $"{am / 1000:F3} s" : "–"));
            }
            else if (p[0] == "thoat") xong.Set();
            else Log("lệnh không hợp lệ");
        }

        new Thread(() => DocLenhStdin(t0, Lenh)) { IsBackground = true }.Start();
        if (tre > 0)
        {
            Log($"Chờ {tre:F1} s rồi mới vào xem (vào muộn)");
            Thread.Sleep(TimeSpan.FromSeconds(tre));
        }
        try { nx.KetNoi(host, cong); }
        catch (Exception e) when (e is IOException or SocketException)
        {
            Log($"Không kết nối được: {e.Message}");
            return 1;
        }
        string nhan = nx.AmThanh && nx.Video ? "tiếng+hình" : nx.AmThanh ? "tiếng" : "hình";
        Log($"Nhận {nhan}, đồng bộ {(nx.DongBo ? "BẬT" : "TẮT")}, jitter {nx.DoSau * 20} ms"
            + (matGoi > 0 ? $", mất {(matGoi * 100).ToString("0.######", CultureInfo.InvariantCulture)}%" : ""));
        Log("Mỗi giây: gói tiếng, mất, trễ ms | khung hiện, lệch ms");
        int giay = 0;
        double moc = Now;
        while (Now - t0 < thoiGian && !xong.IsSet && nx.DaKetNoi)
        {
            if (Now - moc >= 1.0)
            {
                moc += 1.0;
                giay++;
                var r = nx.ThongKeGiay();
                Log($"{giay,2}s tiếng {r.Am,2} mất {r.Mat} trễ {ChuMs(r.Tre),3} | hình {r.Hien,2} lệch {ChuMs(r.Lech, true),4}");
            }
            Thread.Sleep(20);
        }
        var tt = nx.TrangThai();
        nx.Dong();
        foreach (var d in NguoiXem.DongTongKet(tt, tre)) Log(d);
        if (matGoi > 0) Log($"giả lập mất {tt.BoGiaLap} gói nhận");
        if (ghep != null && daChup.Count > 0)
        {
            var (W, H) = GhepAnh(ghep, daChup);
            Log($"Đã ghép {daChup.Count} ảnh vào {ghep} ({W}x{H})");
        }
        return 0;
    }
}
