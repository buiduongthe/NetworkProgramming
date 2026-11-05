using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json.Nodes;
using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Server phòng hội thoại âm thanh nhiều client (trộn tiếng "mix-minus").
// Hai kênh:
//   - TCP (kênh điều khiển, tin cậy): mỗi dòng một thông điệp JSON – vào phòng, tự tắt/bật mic,
//     báo âm lượng nghe; server gửi trạng thái mic, độ lợi, sự kiện, lời từ chối.
//   - UDP (kênh dữ liệu, thời gian thực): gói RTP chứa khung PCM 20 ms. Mất một gói chỉ mất
//     20 ms âm thanh; truyền lại như TCP sẽ đến quá muộn, vô ích với âm thanh trực tiếp.
// Bộ trộn chạy mỗi 20 ms: lấy một khung từ bộ đệm jitter của từng client, nhân độ lợi đầu vào,
// cộng tất cả (tổng T); mỗi người nghe k nhận T - (tiếng của chính k) – "mix-minus", để không nghe
// tiếng vọng của mình – nhân âm lượng chung, cắt ngọn về phạm vi 16 bit rồi gửi đi.
// Quy tắc quyền điều khiển:
//   1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
//   2. Server tắt (khóa) thắng client: khi đang bị khóa, client xin bật mic bị từ chối.
//   3. Server bật = gỡ khóa, KHÔNG bật hộ mic mà người dùng đã tự tắt (quyền riêng tư).
//   4. Độ lợi đầu vào từng client và âm lượng chung do server đặt, áp dụng khi trộn; mỗi lần đổi độ lợi
//      (và một lần sau "chao") server gửi {"loai":"do_loi","gia_tri":g} để client hiển thị.
//   5. Âm lượng nghe do từng client tự đặt và áp dụng tại client (client chỉ báo để server hiển thị).
//   6. Server tự thực thi, không tin client: gói âm thanh từ client đang tắt mic bị bỏ, gói UDP
//      không khớp địa chỉ đã đăng ký bị bỏ.
// Phần xử lý phòng nằm trong lớp PhongAmThanh, dùng chung cho bản dòng lệnh (lớp ServerAmThanh bên dưới)
// và bản giao diện (thư mục GiaoDien). Bản giao diện còn cho server tự nói (micro máy server là một nguồn
// trong bộ trộn, mã SSRC_SERVER, mọi client đều nghe) và tự nghe (tổng tiếng các client phát ra loa server).
// Lệnh bảng điều khiển (stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ khi server chạy):
//   tat_mic <tên|tat_ca>    bat_mic <tên|tat_ca>    do_loi <tên> <hệ số>
//   am_luong_chung <hệ số>  trang_thai               thoat
class Khach(uint ma, string ten, Socket conn, IPAddress ip)
{
    public readonly uint Ma = ma;
    public readonly string Ten = ten;
    public readonly Socket Conn = conn;
    public readonly IPAddress Ip = ip;
    readonly object khoaGui = new();          // nhiều luồng có thể cùng gửi trên TCP
    public IPEndPoint? Udp;                   // địa chỉ UDP học được từ gói đầu tiên
    public bool TuTat;                        // client tự tắt mic
    public bool ServerKhoa;                   // server khóa mic
    public double DoLoi = 1.0;                // độ lợi đầu vào (gain)
    public double AmLuongNghe = 1.0;          // client báo (chỉ để hiển thị)
    public readonly BoDemJitter BoDem = new(3);
    public long SeqRa, TsRa;                  // số thứ tự, nhãn thời gian của luồng trộn gửi cho client
    public double MucVao = double.NegativeInfinity;
    public int BiChan, SaiNguon, CatNgonDem;  // gói bị bỏ vì mic tắt / sai nguồn; số mẫu bị cắt ngọn

    public bool MicMo => !TuTat && !ServerKhoa;          // quy tắc 1

    public string LyDo() => MicMo ? "mở" : TuTat && ServerKhoa ? "tắt (cả hai)" : ServerKhoa ? "tắt (server)" : "tắt (tự tắt)";

    public void Gui(JsonObject obj)
    {
        try { lock (khoaGui) GuiJson(Conn, obj); }
        catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
    }

    public void GuiMic()
    {
        Gui(new JsonObject { ["loai"] = "mic", ["mo"] = MicMo, ["ly_do"] = LyDo() });
        if (!MicMo) BoDem.Xoa();               // bỏ phần âm thanh còn trong bộ đệm
    }

    /// <summary>Quy tắc 4: báo cho client độ lợi server đang áp cho mic của client đó (để hiện "Server Vol").</summary>
    public void GuiDoLoi() => Gui(new JsonObject { ["loai"] = "do_loi", ["gia_tri"] = DoLoi });
}

/// <summary>Một phòng hội thoại: nhận kết nối, nhận RTP, trộn mix-minus, thực thi quyền điều khiển.
/// Mọi trạng thái dùng chung giữa các luồng được bảo vệ bằng Khoa (bản giao diện cũng khóa Khoa khi đọc).</summary>
class PhongAmThanh(Action<string> log)
{
    public readonly object Khoa = new();
    public readonly SortedDictionary<uint, Khach> DsKhach = [];   // mã client (SSRC) -> Khach
    public readonly List<Khach> DaRoi = [];                       // client đã rời phòng (giữ lại để in tổng kết)
    uint maMoi;
    public double AmLuongChung = 1.0;
    readonly Stopwatch t0 = Stopwatch.StartNew();
    TcpListener? srv;
    Socket? udp;
    volatile bool dung;                                           // đặt khi Dung() – các luồng nền tự thoát
    public int CongUdp { get; private set; }

    // ---- Tiếng của chính server (chỉ bản giao diện dùng; bản dòng lệnh để null)
    /// <summary>Gọi mỗi 20 ms trong khóa: trả về 320 mẫu micro của server, hoặc null nếu server không nói.</summary>
    public Func<short[]?>? LayKhungServer { get; set; }
    public double DoLoiServer = 1.0;                              // "Mic Volume" của server
    public double MucServer = double.NegativeInfinity;            // mức micro server (dBFS) để hiển thị
    /// <summary>Gọi mỗi 20 ms NGOÀI khóa với tổng tiếng các client (không gồm tiếng server) – để phát ra loa server.</summary>
    public Action<short[]>? NgheServer { get; set; }

    public double Now => t0.Elapsed.TotalSeconds;
    void Log(string text) => log(text);

    /// <summary>Báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi).</summary>
    void PhatSuKien(string text)
    {
        Log($"  => {text}");
        foreach (var k in DsKhach.Values.ToList()) k.Gui(new JsonObject { ["loai"] = "su_kien", ["noi_dung"] = text });
    }

    // ---------------------------------------------------------------- Khởi động / dừng
    public void BatDau(string host, int congTcp, int congUdp)
    {
        CongUdp = congUdp;
        srv = new TcpListener(IPAddress.Parse(host), congTcp);
        srv.Start();
        udp = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp);
        try { udp.Bind(new IPEndPoint(IPAddress.Parse(host), congUdp)); }
        catch { srv.Stop(); udp.Close(); throw; }
        Log($"Điều khiển: TCP {host}:{congTcp}; âm thanh RTP: UDP {host}:{congUdp}");
        Log($"PCM 16 bit mono 16 kHz; khung {KhungMs} ms = {MauMoiKhung} mẫu = {ByteMoiKhung} B");
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
                c.NoDelay = true;                          // tin điều khiển đi ngay
                Bg(() => PhucVu(c));
            }
        });
        Bg(() => NhanUdp(u));
        Bg(() => VongTron(u));
    }

    /// <summary>Dừng phòng (bản giao diện: nút Stop Server): đóng cổng nghe, socket UDP và mọi kết nối client.</summary>
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
            k.Gui(new JsonObject { ["loai"] = "chao", ["ma"] = k.Ma, ["cong_udp"] = CongUdp, ["tan_so_mau"] = 16000, ["khung_ms"] = KhungMs });
            k.GuiDoLoi();                                                     // quy tắc 4: một lần sau "chao"
            Log($"#{k.Ma} {k.Ten} vào phòng từ {addr.Address}:{addr.Port}");
            lock (Khoa)
            {
                k.GuiMic();
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
                                  e is InvalidOperationException || e is ObjectDisposedException) { }
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
            case "tu_tat_mic":
                k.TuTat = true;
                k.GuiMic();
                PhatSuKien($"{k.Ten} tự tắt mic");
                break;
            case "tu_bat_mic":
                k.TuTat = false;
                if (k.ServerKhoa)                                             // quy tắc 2
                {
                    Log($"{k.Ten} xin bật mic -> từ chối (server đang khóa)");
                    k.Gui(new JsonObject { ["loai"] = "tu_choi", ["ly_do"] = "server đang khóa mic của bạn" });
                }
                else
                {
                    k.GuiMic();
                    PhatSuKien($"{k.Ten} tự bật mic");
                }
                break;
            case "am_luong_nghe":                                             // quy tắc 5: chỉ ghi nhận
                k.AmLuongNghe = tin["gia_tri"]?.GetValue<double>() ?? 1.0;
                Log($"{k.Ten} tự đặt âm lượng nghe {F(k.AmLuongNghe, "F2")}");
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
            catch (SocketException) when (!dung) { continue; }   // Windows: ICMP "port unreachable" từ client đã thoát
            catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { return; }   // đã Dung()
            var addr = (IPEndPoint)from;
            var r = TachRtp(buf.AsSpan(0, n));
            if (r == null) continue;
            var (pt, seq, ts, ssrc, payload) = r.Value;
            lock (Khoa)
            {
                if (!DsKhach.TryGetValue(ssrc, out var k)) continue;
                if (k.Udp == null && addr.Address.Equals(k.Ip))          // đăng ký địa chỉ UDP lần đầu
                {
                    k.Udp = addr;
                    Log($"#{k.Ma} {k.Ten} đăng ký UDP {addr.Address}:{addr.Port}");
                }
                if (!addr.Equals(k.Udp))                                 // quy tắc 6: sai nguồn
                {
                    k.SaiNguon++;
                    continue;
                }
                if (pt != PtAmThanh || payload.Length != ByteMoiKhung) continue;   // gói giữ kết nối hoặc sai dạng
                if (!k.MicMo)                                            // quy tắc 6: mic tắt
                {
                    k.BiChan++;
                    continue;
                }
                k.BoDem.NhanGoi(seq, ts, payload, Now);
            }
        }
    }

    /// <summary>Mỗi 20 ms: trộn và gửi cho từng người nghe. Hẹn giờ tuyệt đối để không trôi nhịp.</summary>
    void VongTron(Socket sock)
    {
        double keTiep = Now;
        while (!dung)
        {
            short[]? choLoa = null;
            lock (Khoa)
            {
                var ds = DsKhach.Values.ToList();
                // 0. Tiếng của chính server (bản giao diện): một nguồn như mọi client, mã SSRC_SERVER
                var tong = new double[MauMoiKhung];
                double[]? tiengServer = null;
                var ks = LayKhungServer?.Invoke();
                MucServer = double.NegativeInfinity;
                if (ks != null && ks.Length == MauMoiKhung)
                {
                    tiengServer = ks.Select(x => DoLoiServer * x).ToArray();
                    MucServer = MucDbfs(ks);
                    for (int i = 0; i < MauMoiKhung; i++) tong[i] += tiengServer[i];
                }
                // 1. Lấy một khung từ mỗi client đang mở mic, nhân độ lợi đầu vào, cộng vào tổng T
                var vao = new Dictionary<uint, double[]>();
                foreach (var k in ds)
                {
                    var p = k.MicMo ? k.BoDem.Lay() : null;
                    if (p == null)
                    {
                        k.MucVao = double.NegativeInfinity;
                        continue;
                    }
                    var mau = ByteSangPcm(p);
                    k.MucVao = MucDbfs(mau);
                    var v = mau.Select(x => k.DoLoi * x).ToArray();
                    vao[k.Ma] = v;
                    for (int i = 0; i < MauMoiKhung; i++) tong[i] += v[i];
                }
                // 2. Mỗi người nghe: T - tiếng của chính mình, nhân âm lượng chung, cắt ngọn
                foreach (var k in ds)
                {
                    if (k.Udp == null) continue;
                    vao.TryGetValue(k.Ma, out var rieng);
                    var ra = new short[MauMoiKhung];
                    for (int i = 0; i < MauMoiKhung; i++)
                    {
                        double v = Math.Round((tong[i] - (rieng?[i] ?? 0.0)) * AmLuongChung);   // làm tròn kiểu "chẵn" như Python
                        if (v > 32767 || v < -32768)
                        {
                            k.CatNgonDem++;
                            v = v > 0 ? 32767 : -32768;
                        }
                        ra[i] = (short)v;
                    }
                    var goi = DongGoiRtp(PtAmThanh, k.SeqRa, k.TsRa, SsrcServer, PcmSangByte(ra));
                    k.SeqRa++;
                    k.TsRa += MauMoiKhung;
                    try { sock.SendTo(goi, k.Udp); } catch (Exception e) when (e is SocketException || e is ObjectDisposedException) { }
                }
                // 3. Server nghe: T - tiếng của chính server (tức tổng các client)
                if (NgheServer != null)
                {
                    choLoa = new short[MauMoiKhung];
                    for (int i = 0; i < MauMoiKhung; i++) choLoa[i] = CatNgon(Math.Round(tong[i] - (tiengServer?[i] ?? 0.0)));
                }
            }
            if (choLoa != null) NgheServer?.Invoke(choLoa);           // ngoài khóa: phát loa không giữ khóa phòng
            keTiep += KhungMs / 1000.0;
            double cho = keTiep - Now;
            if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
        }
    }

    // ---------------------------------------------------------------- Lệnh điều khiển (bảng điều khiển / nút bấm)
    /// <summary>Danh sách client theo tên, hoặc tất cả nếu ten = tat_ca.</summary>
    public List<Khach> Chon(string ten)
    {
        lock (Khoa) return ten == "tat_ca" ? DsKhach.Values.ToList() : DsKhach.Values.Where(k => k.Ten == ten).ToList();
    }

    /// <summary>Server khóa (khoa = true) hoặc gỡ khóa mic của các client ds; ai: tên hiển thị trong sự kiện.</summary>
    public void DatKhoaMic(List<Khach> ds, bool khoa, string ai)
    {
        lock (Khoa)
        {
            foreach (var k in ds)
            {
                k.ServerKhoa = khoa;
                k.GuiMic();
            }
            if (khoa) PhatSuKien($"server tắt mic {ai}");
            else
            {
                var vanTat = ds.Where(k => k.TuTat).Select(k => k.Ten).ToList();   // quy tắc 3
                string them = vanTat.Count > 0 ? $" ({string.Join(", ", vanTat)} vẫn tự tắt)" : "";
                PhatSuKien($"server bật mic {ai}{them}");
            }
        }
    }

    public void DatDoLoi(List<Khach> ds, double g, string ai)
    {
        lock (Khoa)
        {
            foreach (var k in ds)
            {
                k.DoLoi = g;
                k.GuiDoLoi();
            }
            PhatSuKien($"server đặt độ lợi {ai} = {F(g, "F2")}");
        }
    }

    public void DatAmLuongChung(double v)
    {
        lock (Khoa)
        {
            AmLuongChung = v;
            PhatSuKien($"server đặt âm lượng chung {F(AmLuongChung, "F2")}");
        }
    }

    /// <summary>Thực hiện một dòng lệnh bảng điều khiển; trả false khi gặp "thoat".</summary>
    public bool Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (p.Length == 0) return true;
        Log($"t={F(Now, "F1")}s > {dong}");
        lock (Khoa)
        {
            if ((p[0] == "tat_mic" || p[0] == "bat_mic") && p.Length == 2)
            {
                var ds = Chon(p[1]);
                if (ds.Count == 0)
                {
                    Log($"không có client {p[1]}");
                    return true;
                }
                DatKhoaMic(ds, p[0] == "tat_mic", p[1] == "tat_ca" ? "tất cả" : p[1]);
            }
            else if (p[0] == "do_loi" && p.Length == 3)
                DatDoLoi(Chon(p[1]), double.Parse(p[2], System.Globalization.CultureInfo.InvariantCulture), p[1]);
            else if (p[0] == "am_luong_chung" && p.Length == 2)
                DatAmLuongChung(double.Parse(p[1], System.Globalization.CultureInfo.InvariantCulture));
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
            Log($"Trạng thái: {DsKhach.Count} client, âm lượng chung {F(AmLuongChung, "F2")}");
            Log(" #  tên mic          lợi   mức vào            mất  jit.");
            foreach (var k in DsKhach.Values)
                Log($"{k.Ma,2}  {k.Ten,-3} {k.LyDo(),-12} {F(k.DoLoi, "F2"),4} {ChuDb(k.MucVao)} " +
                    $"{ThanhMuc(k.MucVao, 10)} {k.BoDem.Mat,3} {F(k.BoDem.JitterMs, "F1"),4}ms");
        }
    }

    public void InTongKet()
    {
        lock (Khoa)
        {
            Log("Tổng kết (gói vào: từ client tới server):");
            Log(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn");
            foreach (var k in DsKhach.Values.Concat(DaRoi).OrderBy(k => k.Ma))
            {
                var b = k.BoDem;
                Log($"{k.Ma,2}  {k.Ten,-3} {b.Nhan,7} {b.Mat,3} {b.Tre,4} {b.Thieu,5} {b.Can,3} {k.BiChan,7} {k.CatNgonDem,8}");
            }
        }
    }
}

/// <summary>Bản dòng lệnh: một phòng + bảng điều khiển đọc lệnh từ stdin.</summary>
static class ServerAmThanh
{
    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int congTcp = a.Length > 1 ? int.Parse(a[1]) : 41620;
        int congUdp = a.Length > 2 ? int.Parse(a[2]) : 41621;
        void Log(string text) => Console.WriteLine($"[server] {text}");
        var phong = new PhongAmThanh(Log);
        phong.BatDau(host, congTcp, congUdp);
        var dung = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; dung.Set(); };
        new Thread(() =>                                   // bảng điều khiển: đọc lệnh từ bàn phím/kịch bản
        {
            string? dong;
            while ((dong = Console.ReadLine()) != null)
            {
                dong = dong.Split('#')[0].Trim();
                if (dong.StartsWith('@'))                  // "@t lệnh": chờ tới giây thứ t
                {
                    int sp = dong.IndexOf(' ');
                    string moc = sp < 0 ? dong : dong[..sp];
                    dong = sp < 0 ? "" : dong[(sp + 1)..];
                    double cho = double.Parse(moc[1..], System.Globalization.CultureInfo.InvariantCulture) - phong.Now;
                    if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
                }
                if (dong != "" && !phong.Lenh(dong))
                {
                    dung.Set();
                    return;
                }
            }                                              // hết stdin: tiếp tục phục vụ tới khi Ctrl+C
        }) { IsBackground = true }.Start();
        dung.Wait();
        Log("Dừng server (Ctrl+C)");
        phong.InTongKet();
        return 0;
    }
}
