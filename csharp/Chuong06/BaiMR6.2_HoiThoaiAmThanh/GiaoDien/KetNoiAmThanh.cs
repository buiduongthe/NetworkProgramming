using System.Diagnostics;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Phần "máy" của client giao diện (không phụ thuộc WinForms): kết nối, gửi/nhận RTP, bộ đệm jitter, đo mức.
// Cùng giao thức với bản dòng lệnh KhachAmThanh.cs. Ba loại luồng nền:
//   - luồng đọc TCP: thông điệp điều khiển từ server (mic, do_loi, su_kien, tu_choi);
//   - luồng nhận UDP: gói RTP luồng trộn -> bộ đệm jitter;
//   - luồng âm thanh, nhịp 20 ms (chạy suốt đời đối tượng, kể cả khi chưa kết nối để đo mức micro):
//       1. lấy một khung từ micro (thật hoặc giả lập) nếu người dùng đã Start Mic, nhân Mic Volume, đo mức;
//          nếu đã kết nối và server cho mic mở -> đóng gói RTP và gửi;
//       2. lấy một khung từ bộ đệm jitter, nhân âm lượng nghe, đo mức; nếu Start Audio -> phát ra loa.
// Luồng giao diện KHÔNG gọi thẳng vào các luồng này: nó đặt cài đặt (trong khóa) và đọc trạng thái mỗi 100 ms.
sealed class KetNoiAmThanh : IDisposable
{
    public readonly object Khoa = new();
    readonly Action<string> log;
    readonly Stopwatch sw = Stopwatch.StartNew();
    double Now => sw.Elapsed.TotalSeconds;

    // ---- Trạng thái kết nối (đọc trong Khoa)
    Socket? tcp, udp;
    public bool DaKetNoi { get; private set; }
    public uint Ma { get; private set; }
    public bool MicMo { get; private set; }             // server cho mic của mình mở (thông điệp "mic")
    public string LyDoMic { get; private set; } = "chưa kết nối";
    public double? ServerVol { get; private set; }       // độ lợi server áp cho mic của mình ("do_loi")
    public string TrangThai { get; private set; } = "Chưa kết nối";
    public bool TrangThaiLoi { get; private set; }
    BoDemJitter boDem = new(3);
    long seq, ts;
    public int GoiGui { get; private set; }
    public int GoiNhan => boDem.Nhan;
    public long MatNhan => boDem.Mat;
    public double JitterMs => boDem.JitterMs;

    // ---- Cài đặt do giao diện đặt (trong Khoa)
    public INguonMic? Mic;                               // null = Stop Mic
    public double AmLuongMic = 1.0, AmLuongNghe = 1.0;
    public Action<short[]>? PhatRaLoa;                   // null = Stop Audio (vẫn nhận và đo mức)
    public readonly DoMuc MucMic = new(), MucNhan = new();

    volatile bool huy;
    readonly Thread luongAm;

    public KetNoiAmThanh(Action<string> log)
    {
        this.log = log;
        luongAm = new Thread(VongAmThanh) { IsBackground = true, Name = "am-thanh-20ms" };
        luongAm.Start();
    }

    void DatTrangThai(string s, bool loi = false)
    {
        lock (Khoa) (TrangThai, TrangThaiLoi) = (s, loi);
        log(s);
    }

    // ---------------------------------------------------------------- Kết nối (gọi trên luồng nền: Task.Run)
    public void KetNoi(string host, int cong, string ten)
    {
        var t = new Socket(SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        try
        {
            if (!t.ConnectAsync(host, cong).Wait(TimeSpan.FromSeconds(3)))
                throw new TimeoutException($"không kết nối được {host}:{cong} sau 3 s");
            t.ReceiveTimeout = 3000;                                     // chờ "chao" tối đa 3 s
            var f = new StreamReader(new NetworkStream(t), Encoding.UTF8);
            GuiJson(t, new JsonObject { ["loai"] = "vao", ["ten"] = ten });
            var chao = JsonNode.Parse(f.ReadLine() ?? "{}")!.AsObject();
            if (chao["loai"]?.GetValue<string>() != "chao") throw new InvalidDataException("server không chào");
            t.ReceiveTimeout = 0;
            var u = new Socket(SocketType.Dgram, ProtocolType.Udp);
            u.Connect(host, chao["cong_udp"]!.GetValue<int>());         // chỉ nhận gói từ đúng server
            bool micBat;
            double nghe;
            lock (Khoa)
            {
                (tcp, udp) = (t, u);
                Ma = chao["ma"]!.GetValue<uint>();
                boDem = new BoDemJitter(3);
                (seq, ts, GoiGui) = (0, 0, 0);
                (MicMo, LyDoMic, ServerVol) = (false, "chờ server", null);
                DaKetNoi = true;
                micBat = Mic != null;
                nghe = AmLuongNghe;
            }
            DatTrangThai($"Đã kết nối {host}:{cong}, mã {Ma}");
            new Thread(() => DocTcp(t, f)) { IsBackground = true, Name = "doc-tcp" }.Start();
            new Thread(() => NhanUdp(u)) { IsBackground = true, Name = "nhan-udp" }.Start();
            u.Send(DongGoiRtp(PtGiuKetNoi, 0, 0, Ma));                   // đăng ký địa chỉ UDP với server
            if (!micBat) Gui(new JsonObject { ["loai"] = "tu_tat_mic" });   // chưa Start Mic: không tự mở mic
            if (nghe != 1.0) GuiAmLuongNghe(nghe);
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
            (MicMo, LyDoMic, ServerVol) = (false, "chưa kết nối", null);
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

    /// <summary>Người dùng bấm Start Mic: mở nguồn rồi xin server bật mic (server có thể từ chối).</summary>
    public void BatMic(INguonMic nguon)
    {
        INguonMic? cu;
        lock (Khoa) (cu, Mic) = (Mic, nguon);
        cu?.Dispose();
        log($"Start Mic: {nguon.MoTa}");
        Gui(new JsonObject { ["loai"] = "tu_bat_mic" });
    }

    /// <summary>Stop Mic: tự tắt có hiệu lực ngay (không cần server đồng ý), rồi báo server.</summary>
    public void TatMic()
    {
        INguonMic? cu;
        lock (Khoa)
        {
            (cu, Mic) = (Mic, null);
            if (DaKetNoi) (MicMo, LyDoMic) = (false, "tắt (tự tắt)");
        }
        cu?.Dispose();
        log("Stop Mic");
        Gui(new JsonObject { ["loai"] = "tu_tat_mic" });
    }

    /// <summary>Âm lượng nghe áp dụng tại client; chỉ báo cho server để hiển thị (quy tắc 5).</summary>
    public void GuiAmLuongNghe(double v) => Gui(new JsonObject { ["loai"] = "am_luong_nghe", ["gia_tri"] = Math.Round(v, 2) });

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
                    case "mic":
                        lock (Khoa) (MicMo, LyDoMic) = (tin["mo"]!.GetValue<bool>(), tin["ly_do"]?.GetValue<string>() ?? "");
                        log($"Mic: {tin["ly_do"]}");
                        break;
                    case "do_loi":                                        // thông điệp mới: độ lợi server đặt cho mic mình
                        lock (Khoa) ServerVol = tin["gia_tri"]!.GetValue<double>();
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
        lock (Khoa) laHienTai = tcp == t;                              // không phải do người dùng bấm Disconnect
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
            catch (SocketException) when (udp == u) { continue; }     // Windows: ICMP "port unreachable"
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }
            var r = TachRtp(buf.AsSpan(0, n));
            if (r is { } g && g.Pt == PtAmThanh && g.Payload.Length == ByteMoiKhung)
                lock (Khoa) boDem.NhanGoi(g.Seq, g.Ts, g.Payload, Now);
        }
    }

    // ---------------------------------------------------------------- Luồng âm thanh 20 ms
    void VongAmThanh()
    {
        double keTiep = Now;
        long n = 0;
        while (!huy)
        {
            n++;
            INguonMic? mic;
            Socket? u;
            bool mo;
            double vMic, vNghe;
            Action<short[]>? loa;
            lock (Khoa) (mic, u, mo, vMic, vNghe, loa) = (Mic, udp, MicMo, AmLuongMic, AmLuongNghe, PhatRaLoa);
            // 1. Thu và gửi
            short[]? khung = null;
            try { khung = mic?.LayKhung(); } catch (ObjectDisposedException) { }
            if (khung != null) khung = AmThanhPhu.NhanHeSo(khung, vMic);
            MucMic.Them(mic != null ? khung : null);
            if (u != null)
            {
                try
                {
                    if (khung != null && mo)
                    {
                        long s;
                        lock (Khoa) s = seq++;
                        u.Send(DongGoiRtp(PtAmThanh, s, ts, Ma, PcmSangByte(khung)));
                        lock (Khoa) GoiGui++;
                    }
                    else if (n % 50 == 0) u.Send(DongGoiRtp(PtGiuKetNoi, 0, 0, Ma));   // mỗi giây giữ kết nối
                }
                catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
                ts += MauMoiKhung;                       // nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
                // 2. Nhận và phát
                byte[]? p;
                lock (Khoa) p = boDem.Lay();
                var mau = p != null ? AmThanhPhu.NhanHeSo(ByteSangPcm(p), vNghe) : new short[MauMoiKhung];
                MucNhan.Them(mau);
                try { loa?.Invoke(mau); } catch (Exception e) { log($"Lỗi loa: {e.Message}"); lock (Khoa) PhatRaLoa = null; }
            }
            else MucNhan.Them(null);
            keTiep += KhungMs / 1000.0;
            double cho = keTiep - Now;
            if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
            else if (cho < -0.2) keTiep = Now;           // bị treo lâu (máy bận): bắt nhịp lại, không gửi dồn
        }
    }

    public void Dispose()
    {
        NgatKetNoi();
        huy = true;
        luongAm.Join(500);
        INguonMic? m;
        lock (Khoa) (m, Mic) = (Mic, null);
        m?.Dispose();
    }
}
