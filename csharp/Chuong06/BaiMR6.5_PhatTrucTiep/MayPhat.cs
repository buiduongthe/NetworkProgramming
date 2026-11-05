using System.Globalization;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using static BaiMR6_5.Chung;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Lõi máy phát (camera + micro -> máy chủ), dạng lớp, không chạm giao diện.
//
// Dùng chung cho bản dòng lệnh (lớp Phat cuối tệp) và bản giao diện (GiaoDien/FormPhat.cs). Báo sự kiện bằng
// `bao(text)`; giao diện đọc số liệu (xem trước, mức micro, thống kê, danh sách người xem) qua TrangThai().
//
// Đồng hồ phương tiện chung: t = 0 khi bấm Phát (đồng hồ đơn điệu Stopwatch – Chung.Now).
//   - Âm thanh: luồng 20 ms; gói thứ n được gửi lúc t0 + n·20 ms (bám đồng hồ, không trôi), nhãn RTP
//     ts = n·320 (= số mẫu đã gửi, 16 kHz).
//   - Video: luồng theo fps; nhãn = ms phương tiện lúc chụp khung. Khung 0 là I; I lặp lại mỗi `gop` khung hoặc
//     NGAY khung kế tiếp sau khi nhận xin_khung_khoa (đếm GOP lại từ khung I đó).
//   - Mỗi 1 s gửi {"loai":"dong_ho","ts":ms phương tiện,"gio":ms Unix} (như RTCP Sender Report – RFC 3550 mục
//     6.4.1): người xem ánh xạ nhãn thời gian <-> giờ thực để đo độ trễ đầu-cuối.

/// <summary>Ảnh chụp trạng thái máy phát.</summary>
record TrangThaiMayPhat(bool DangPhat, bool DaKetNoi, int? Ma, int SoI, int SoP, double TbI, double TbP, int PMin, int PMax,
                        long GoiAm, int PliNhan, double KbpsAm, double KbpsHinh, List<string> NguoiXem, double MucMic,
                        (int W, int H, byte[] Anh, long Ts)? XemTruoc, int Fps, int Gop, string Camera, string Mic);

sealed class MayPhat
{
    public readonly string Ten;
    readonly Action<string> bao;
    readonly object khoa = new(), khoaTcp = new();
    Socket? tcp, udp;
    public volatile bool DaKetNoi;
    public int? Ma;
    bool dangPhat;
    INguonHinh? camera;
    INguonMic? mic;
    int fps = FpsMacDinh, gop = GopMacDinh;
    List<string> nguoiXem = [];                       // tên người xem (theo thông điệp nguoi_xem)
    (int W, int H, byte[] Anh, long Ts)? xemTruoc;   // khung vừa gửi
    volatile float mucMic = float.NegativeInfinity;
    readonly double matGoi;
    readonly Random rng = new(1);
    public int BoGiaLap;
    double t0;
    long so, seq, nAm;
    long? soICuoi;
    byte[]? truoc;
    bool canI;
    string lyDoI = "";
    int soI, soP, pliNhan;
    long byteI, byteP, goiAm;
    int? pMin, pMax;
    int demI, demP;
    long demByteAm, demByteHinh;
    double kbpsAm, kbpsHinh;
    volatile bool song = true;

    public MayPhat(string ten, Action<string>? bao = null, double matGoi = 0.0)
    {
        (Ten, this.bao, this.matGoi) = (ten, bao ?? (_ => { }), matGoi);
        new Thread(VongAm) { IsBackground = true, Name = "vong-am" }.Start();
        new Thread(VongHinh) { IsBackground = true, Name = "vong-hinh" }.Start();
    }

    void LamMoiThongKe()
    {
        soI = soP = pliNhan = 0;
        byteI = byteP = goiAm = 0;
        pMin = pMax = null;
        demI = demP = 0;
        demByteAm = demByteHinh = 0;
        kbpsAm = kbpsHinh = 0;
    }

    // ------------------------------------------------------------ kết nối
    /// <summary>Kết nối TCP, chào với vai trò "phat". Lỗi (kể cả server từ chối) -> IOException hoặc SocketException.</summary>
    public int KetNoi(string host, int cong)
    {
        var (t, ip) = KetNoiTcp(host, cong);
        Socket u;
        DocDong doc;
        JsonObject tl;
        try
        {
            doc = new DocDong(new NetworkStream(t));
            GuiJson(t, new JsonObject { ["loai"] = "chao", ["vai_tro"] = "phat", ["ten"] = Ten });
            tl = doc.Doc() ?? [];
            if (Chuoi(tl, "loai") == "loi") throw new IOException($"server từ chối: {Chuoi(tl, "ly_do")}");
            if (Chuoi(tl, "loai") != "chao") throw new IOException("server không trả lời 'chao'");
            u = new Socket(ip.AddressFamily, SocketType.Dgram, ProtocolType.Udp) { SendBufferSize = 1 << 20 };
            TatLoiUdpReset(u);
            u.Connect(ip, (int)(So(tl, "cong_udp") ?? throw new IOException("thiếu cong_udp")));
        }
        catch (Exception e) when (e is SocketException or IOException or InvalidDataException)
        {
            t.Close();
            throw new IOException(e.Message);
        }
        int ma = (int)(So(tl, "ma") ?? 0);
        lock (khoa) (tcp, udp, Ma, DaKetNoi) = (t, u, ma, true);
        u.Send(GoiGiu(ma));                                          // đăng ký địa chỉ UDP ngay
        new Thread(() => DocTcp(doc)) { IsBackground = true, Name = "doc-tcp" }.Start();
        return ma;
    }

    public void Gui(JsonObject obj)
    {
        var t = tcp;
        if (t == null) return;
        try { lock (khoaTcp) GuiJson(t, obj); }
        catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
    }

    void DocTcp(DocDong doc)
    {
        try
        {
            while (true)
            {
                var tin = doc.Doc();
                if (tin == null) break;
                string? loai = Chuoi(tin, "loai");
                if (loai == "xin_khung_khoa")
                    lock (khoa)
                    {
                        pliNhan++;
                        lyDoI = $"PLI từ {Chuoi(tin, "tu") ?? "?"}";
                        canI = true;
                    }
                else if (loai == "nguoi_xem")
                    lock (khoa) nguoiXem = (tin["ds"] as JsonArray)?.Select(x => x?.ToString() ?? "").ToList() ?? [];
                else if (loai == "su_kien") bao($"Sự kiện: {Chuoi(tin, "noi_dung")}");
                else if (loai == "loi") bao($"Lỗi từ server: {Chuoi(tin, "ly_do")}");
            }
        }
        catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ObjectDisposedException) { }
        if (DaKetNoi)
        {
            bao("Mất kết nối server");
            Ngat();
        }
    }

    public void Ngat()
    {
        Socket? t, u;
        lock (khoa)
        {
            (t, u) = (tcp, udp);
            (tcp, udp) = (null, null);
            DaKetNoi = false;
            dangPhat = false;
            nguoiXem = [];
        }
        try { t?.Shutdown(SocketShutdown.Both); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        t?.Close();
        u?.Close();
    }

    // ------------------------------------------------------------ phát / dừng
    JsonObject ThongTin() => new()
    {
        ["loai"] = "thong_tin", ["rong"] = Rong, ["cao"] = Cao, ["fps"] = fps, ["gop"] = gop, ["tan_so_mau"] = TanSoMau,
    };

    public void BatDauPhat(INguonHinh cam, INguonMic m, int? fpsMoi = null, int? gopMoi = null)
    {
        JsonObject tt;
        lock (khoa)
        {
            (camera, mic) = (cam, m);
            fps = Math.Clamp(fpsMoi ?? fps, 1, 30);
            gop = Math.Clamp(gopMoi ?? gop, 1, 300);
            tt = ThongTin();
        }
        // thong_tin đi TCP, gói phương tiện đi UDP trên luồng khác: gửi thong_tin trước và chờ 20 ms để máy chủ kịp
        // chuyển sang "đang phát" – nếu không, khung I #0 có thể tới trước, bị bỏ vì "chưa phát", và bộ đệm GOP rỗng
        // tới khung I kế tiếp.
        Gui(tt);
        Thread.Sleep(20);
        lock (khoa)
        {
            t0 = Now;
            so = seq = nAm = 0;
            (soICuoi, truoc) = (null, null);
            (canI, lyDoI) = (false, "");
            LamMoiThongKe();
            dangPhat = true;
            cam.DatFps(fps);
        }
        bao($"Bắt đầu phát {Rong}x{Cao} {fps} fps GOP {gop}");
    }

    public void DatThamSo(int fpsMoi, int gopMoi)
    {
        bool phat;
        JsonObject tt;
        lock (khoa)
        {
            (fps, gop) = (Math.Clamp(fpsMoi, 1, 30), Math.Clamp(gopMoi, 1, 300));
            camera?.DatFps(fps);
            phat = dangPhat;
            tt = ThongTin();
        }
        if (phat) Gui(tt);
        bao($"Tham số: {fps} fps, GOP {gop}");
    }

    public void KhungKhoaNgay(string lyDo = "lệnh khung khóa")
    {
        lock (khoa) (canI, lyDoI) = (true, lyDo);
    }

    public bool DangPhat
    {
        get { lock (khoa) return dangPhat; }
    }

    public void DungPhat()
    {
        INguonHinh? cam;
        INguonMic? m;
        lock (khoa)
        {
            if (!dangPhat) return;
            dangPhat = false;
            (cam, m, camera, mic) = (camera, mic, null, null);
        }
        Gui(new JsonObject { ["loai"] = "dung_phat" });
        cam?.Dispose();
        m?.Dispose();
        bao("Dừng phát");
    }

    /// <summary>Gửi một datagram; --mat-goi giả lập mạng làm rơi gói ở chiều lên.</summary>
    void GuiUdp(byte[] goi)
    {
        if (matGoi > 0 && rng.NextDouble() < matGoi)
        {
            BoGiaLap++;
            return;
        }
        var u = udp;
        if (u == null) return;
        try { u.Send(goi); } catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
    }

    // ------------------------------------------------------------ luồng âm thanh 20 ms
    void VongAm()
    {
        double hanGiu = Now;
        while (song)
        {
            double bayGio = Now;
            bool phat;
            INguonMic? m;
            int? ma;
            Socket? u;
            double gocT0;
            lock (khoa) (phat, m, ma, u, gocT0) = (dangPhat, mic, Ma, udp, t0);
            if (u != null && bayGio >= hanGiu)
            {
                GuiUdp(GoiGiu(ma ?? 0));                               // giữ kết nối mỗi 1 s
                hanGiu = bayGio + 1.0;
            }
            if (!(phat && m != null && u != null))
            {
                mucMic = float.NegativeInfinity;
                Ngu(0.02);
                continue;
            }
            double han = gocT0 + nAm * KhungMs / 1000.0;
            if (bayGio < han)
            {
                Ngu(Math.Min(han - bayGio, 0.02));
                continue;
            }
            if (nAm % 50 == 0)                                         // báo cáo đồng hồ mỗi 1 s
                Gui(new JsonObject { ["loai"] = "dong_ho", ["ts"] = (long)((bayGio - gocT0) * 1000), ["gio"] = (long)GioUnixMs });
            var khung = m.LayKhung();
            mucMic = (float)MucDbfs(khung);
            var goi = DongGoiRtp(seq, nAm * MauMoiKhung, (uint)(ma ?? 0), PcmSangByte(khung));
            GuiUdp(goi);
            lock (khoa)
            {
                seq++;
                nAm++;
                goiAm++;
                demByteAm += goi.Length;
            }
        }
    }

    // ------------------------------------------------------------ luồng video theo fps
    void VongHinh()
    {
        double? han = null;
        while (song)
        {
            double bayGio = Now;
            bool phat;
            INguonHinh? cam;
            int? ma;
            Socket? u;
            double gocT0;
            int f;
            lock (khoa) (phat, cam, ma, u, gocT0, f) = (dangPhat, camera, Ma, udp, t0, fps);
            if (!(phat && cam != null && u != null))
            {
                han = null;
                Ngu(0.01);
                continue;
            }
            if (han == null || han < gocT0) han = gocT0;
            if (bayGio < han)
            {
                Ngu(Math.Min(han.Value - bayGio, 0.01));
                continue;
            }
            han += 1.0 / f;
            if (han < bayGio - 0.2) han = bayGio;                       // tụt quá xa (máy bận): bắt nhịp lại
            if (cam.Loi is { } loi)                                     // ffmpeg dừng/lỗi: lùi về giả lập
            {
                bao($"Camera lỗi, dùng giả lập: {(loi.Length > 28 ? loi[..28] : loi)}");
                cam.Dispose();
                cam = new CameraGiaLap(Ten, ma ?? 1, f);
                lock (khoa) camera = cam;
            }
            long tsMs = (long)((bayGio - gocT0) * 1000);
            long s;
            lock (khoa) s = so;
            var anh = cam.LayKhung(s, tsMs);
            if (anh == null || anh.Length != Rong * Cao * 3) continue;   // camera chưa có khung
            bool laI;
            string lyDo;
            lock (khoa)
            {
                laI = soICuoi == null || s - soICuoi >= gop || canI;
                lyDo = canI ? lyDoI : "";
                (canI, lyDoI) = (false, "");
            }
            byte loai = laI ? LoaiI : LoaiP;
            var duLieu = laI ? MaHoaI(anh) : MaHoaP(anh, truoc!);
            var manh = ChiaManh(loai, ma ?? 0, s, Rong, Cao, tsMs, duLieu);
            foreach (var g in manh) GuiUdp(g);
            if (lyDo != "") bao($"{lyDo} -> gửi khung I #{s}");
            lock (khoa)
            {
                truoc = anh;
                so++;
                if (laI)
                {
                    soICuoi = s;
                    soI++;
                    byteI += duLieu.Length;
                    demI++;
                }
                else
                {
                    soP++;
                    byteP += duLieu.Length;
                    pMin = pMin == null ? duLieu.Length : Math.Min(pMin.Value, duLieu.Length);
                    pMax = pMax == null ? duLieu.Length : Math.Max(pMax.Value, duLieu.Length);
                    demP++;
                }
                demByteHinh += manh.Sum(g => g.Length);
                xemTruoc = (Rong, Cao, truoc, tsMs);
            }
        }
    }

    // ------------------------------------------------------------ số liệu
    /// <summary>Gọi mỗi 1 s (dòng lệnh hoặc giao diện): trả về và xóa bộ đếm của giây vừa qua.</summary>
    public (int I, int P, double KbpsAm, double KbpsHinh) ThongKeGiay()
    {
        lock (khoa)
        {
            var r = (demI, demP, demByteAm * 8 / 1000.0, demByteHinh * 8 / 1000.0);
            (kbpsAm, kbpsHinh) = (r.Item3, r.Item4);
            demI = demP = 0;
            demByteAm = demByteHinh = 0;
            return r;
        }
    }

    public TrangThaiMayPhat TrangThai()
    {
        lock (khoa)
            return new(dangPhat, DaKetNoi, Ma, soI, soP, soI > 0 ? (double)byteI / soI : 0.0, soP > 0 ? (double)byteP / soP : 0.0,
                       pMin ?? 0, pMax ?? 0, goiAm, pliNhan, kbpsAm, kbpsHinh, [.. nguoiXem], mucMic, xemTruoc, fps, gop,
                       camera?.MoTa ?? "", mic?.MoTa ?? "");
    }

    public void Dong()
    {
        DungPhat();
        song = false;
        Ngat();
    }

    /// <summary>Các dòng tổng kết của máy phát (dùng chung cho dòng lệnh).</summary>
    public static List<string> DongTongKet(TrangThaiMayPhat tt, double giay) =>
    [
        $"===== TỔNG KẾT sau {giay:F1} s =====",
        $"khung I {tt.SoI}, P {tt.SoP}; TB I {tt.TbI:F0} B, P {tt.TbP:F0} B",
        $"P nhỏ nhất {tt.PMin} B, lớn nhất {tt.PMax} B (khung chớp)",
        $"I/thô {tt.TbI / (Rong * Cao * 3) * 100:F1} %, P TB/I TB {(tt.TbI > 0 ? tt.TbP / tt.TbI * 100 : 0):F1} %",
        $"tiếng {tt.GoiAm} gói; PLI nhận {tt.PliNhan}",
    ];
}

/// <summary>Máy phát trực tiếp (bản dòng lệnh): camera + micro -> máy chủ.
/// Mặc định camera giả lập "bảng clapper" (đồng hồ, số khung, chớp trắng mỗi giây tròn) và micro giả lập
/// (440 Hz + bíp 1000 Hz mỗi giây tròn).
///   --camera testsrc     ảnh mẫu thử của ffmpeg (cần ffmpeg)
///   --camera &lt;thiết bị&gt;  camera thật qua ffmpeg (/dev/video0, "Tên camera" trên Windows, 0 trên macOS)
///   --mic &lt;chỉ số&gt;       micro thật qua winmm (chỉ Windows)
///   --tep &lt;tệp video&gt;    phát lại hình + tiếng của tệp (mp4, mkv…) qua ffmpeg, lặp lại khi hết tệp;
///                        thêm --mic gia_lap|&lt;chỉ số&gt; để thay tiếng của tệp bằng micro
///   --tao-tep-mau        tạo trước tệp mẫu 10 s (chớp trắng + bíp mỗi giây) tại đường dẫn của --tep
/// Thiết bị thật không mở được -> tự lùi về nguồn giả lập (có báo). Lệnh stdin: khung_khoa   dung</summary>
static class Phat
{
    public static int Run(string[] a)
    {
        string ten = a[0], host = a[1];
        int cong = int.Parse(a[2]);
        string camera = CameraFfmpeg.GiaLap;
        string? micro = null, tep = null;                      // micro mặc định: gia_lap (hoặc tiếng của tệp khi có --tep)
        bool taoTepMau = false;
        int fps = FpsMacDinh, gop = GopMacDinh;
        double thoiGian = 30.0, matGoi = 0.0;
        for (int i = 3; i < a.Length; i++)
        {
            string V() => i + 1 < a.Length ? a[++i] : throw new ArgumentException($"thiếu giá trị cho {a[i]}");
            switch (a[i])
            {
                case "--camera": camera = V(); break;
                case "--mic": micro = V(); break;
                case "--tep": tep = V(); break;
                case "--tao-tep-mau": taoTepMau = true; break;
                case "--fps": fps = int.Parse(V()); break;
                case "--gop": gop = int.Parse(V()); break;
                case "--thoi-gian": thoiGian = double.Parse(V(), CultureInfo.InvariantCulture); break;
                case "--mat-goi": matGoi = double.Parse(V(), CultureInfo.InvariantCulture); break;
                default: Console.WriteLine($"Tùy chọn không hợp lệ: {a[i]}"); return 2;
            }
        }
        var khoaIn = new object();
        void Log(string text)
        {
            lock (khoaIn) Console.WriteLine($"[phat {ten}] {text}");     // nhiều luồng cùng in: mỗi dòng in trọn vẹn
        }
        var mp = new MayPhat(ten, Log, matGoi);
        int ma;
        try { ma = mp.KetNoi(host, cong); }
        catch (Exception e) when (e is IOException or SocketException)
        {
            Log($"Không kết nối được: {e.Message}");
            return 1;
        }
        INguonHinh? cam = null;
        INguonMic? mic = null;
        if (tep != null)
        {
            try
            {
                if (taoTepMau)
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(tep))!);
                    TepVideo.TaoTepMau(tep, 10);
                    Log($"Đã tạo tệp mẫu {Path.GetFileName(tep)} (10 s)");
                }
                (cam, mic) = TepVideo.MoTep(tep, fps, layTieng: micro == null);
            }
            catch (IOException e)
            {
                Log($"Không mở được tệp video: {e.Message}");
                (cam, mic) = (null, null);
            }
        }
        if (cam == null)
        {
            if (camera != CameraFfmpeg.GiaLap && CameraFfmpeg.CoFfmpeg()) cam = new CameraFfmpeg(camera, camera, fps);
            else
            {
                if (camera != CameraFfmpeg.GiaLap) Log("Không có ffmpeg: dùng camera giả lập");
                cam = new CameraGiaLap(ten, ma, fps);
            }
        }
        if (mic == null)
        {
            string tenMic = micro ?? "gia_lap";
            int? chiSo = tenMic != "gia_lap" && int.TryParse(tenMic, out int cs) ? cs : null;
            string tenHien = chiSo is { } c0 ? ThietBiAmThanh.LietKeMicro().FirstOrDefault(m => m.ChiSo == c0).Ten ?? tenMic : tenMic;
            string loi;
            (mic, loi) = ThietBiAmThanh.MoMic(chiSo, tenHien);
            if (loi != "" || (tenMic != "gia_lap" && chiSo == null)) Log("Không mở được micro thật: dùng giả lập");
        }
        Log($"Mã {ma}; camera {cam.MoTa}");
        Log($"Micro {mic.MoTa}");
        var xong = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            xong.Set();
        };

        void Lenh(string dong)
        {
            Log($"> {dong}");
            if (dong == "khung_khoa") mp.KhungKhoaNgay("lệnh khung_khoa");
            else if (dong == "dung") xong.Set();
            else Log("lệnh không hợp lệ");
        }

        double t0 = Now;
        new Thread(() => DocLenhStdin(t0, Lenh)) { IsBackground = true }.Start();
        mp.BatDauPhat(cam, mic, fps, gop);
        Log("Mỗi giây: khung I/P, kbit/s hình, tiếng (cả header)");
        int giay = 0;
        double moc = Now;
        while (Now - t0 < thoiGian && !xong.IsSet && mp.DaKetNoi)
        {
            if (Now - moc >= 1.0)
            {
                moc += 1.0;
                giay++;
                var r = mp.ThongKeGiay();
                Log($"{giay,2}s khung I {r.I} P {r.P,2} | hình {r.KbpsHinh,4:F0} tiếng {r.KbpsAm,3:F0} kbit/s");
            }
            Thread.Sleep(20);
        }
        double tong = Now - t0;
        var tt = mp.TrangThai();
        mp.Dong();
        foreach (var d in MayPhat.DongTongKet(tt, tong)) Log(d);
        if (matGoi > 0) Log($"giả lập mất {mp.BoGiaLap} gói gửi");
        return 0;
    }
}
