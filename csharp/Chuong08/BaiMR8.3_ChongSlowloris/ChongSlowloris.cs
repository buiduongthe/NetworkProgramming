using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR8_3;

// Bài mở rộng MR8.3 – Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu "nhỏ giọt tiêu đề".
//
// Bài mở rộng, KHÔNG có trong giáo trình; nối tiếp Chương 6 (server nhiều client, Bài 6.1/6.2)
// và Chương 8 (vận hành an toàn). Mục đích DUY NHẤT là dạy cách *phòng thủ*: so sánh một server
// HTTP/1.1 nhỏ ở hai chế độ và đo xem một yêu cầu bình thường còn được phục vụ hay không.
//
//   server vulnerable <host> <port>   – nhóm luồng cố định, KHÔNG đặt hạn đọc tiêu đề (dễ bị đánh gục)
//   server protected  <host> <port>   – có hạn đọc tiêu đề, cỡ tiêu đề tối đa, hạn chót rỗi,
//                                        giới hạn kết nối theo IP và trần kết nối toàn cục (trả 503)
//   slow   <host> <port> [N] [nguồn]  – CLIENT THÍ NGHIỆM: mở N kết nối tới 127.0.0.1 và gửi phần đầu
//                                        rất chậm (chỉ cho loopback – từ chối mọi đích khác).
//                                        Mặc định dùng địa chỉ nguồn 127.0.0.2 để tách IP "kẻ gây tải"
//                                        khỏi IP của probe (127.0.0.1), nhờ đó thấy rõ giới hạn theo IP.
//   probe  <host> <port>              – gửi MỘT yêu cầu HTTP bình thường, đo thành công/độ trễ
//
// CẢNH BÁO ĐẠO ĐỨC: vai "slow" chỉ dùng trong phòng thí nghiệm cô lập, chỉ nhắm 127.0.0.1 của
// chính máy bạn, để kiểm chứng biện pháp phòng thủ. Tuyệt đối không nhắm máy/dịch vụ của người khác.

static class ChongSlowloris
{
    // ---------------------------- Tham số phòng thủ (chế độ protected) ----------------------------
    const double HanDocTieuDe = 3.0;         // giây: toàn bộ phần đầu (request head) phải xong trong hạn này
    const double HanChotRoi = 2.0;           // giây: không nhận thêm byte nào trong khoảng này -> coi là rỗi, đóng
    const int CoTieuDeToiDa = 8 * 1024;      // byte: tổng kích thước phần đầu tối đa
    const int GioiHanIp = 10;                // số kết nối đồng thời tối đa cho mỗi địa chỉ IP
    const int TranToanCuc = 64;              // số kết nối đồng thời tối đa toàn server (vượt -> trả 503 rồi đóng)
    const int SoLuongWorker = 40;            // nhóm luồng cố định (giống MaxRequestWorkers của Apache prefork)

    static readonly byte[] TraLoi = Encoding.ASCII.GetBytes(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n" +
        "Connection: close\r\nContent-Length: 20\r\n\r\nXin chao tu server!\n");
    static readonly byte[] TraLoi503 = Encoding.ASCII.GetBytes(
        "HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");

    /// <summary>Host có phải địa chỉ loopback (127.0.0.0/8, ::1) hay không – dùng cho vai 'slow'.</summary>
    static bool LoopbackHayKhong(string host)
    {
        try
        {
            var infos = Dns.GetHostAddresses(host);
            return infos.Length > 0 && infos.All(a => IPAddress.IsLoopback(a));
        }
        catch (SocketException) { return false; }
    }

    // =============================== SERVER ===============================
    /// <summary>Bộ đếm có khóa để các luồng worker cùng ghi.</summary>
    sealed class ThongKe
    {
        public readonly object Khoa = new();
        public int ChapNhan, Xong, DongCham, TuChoi503;
    }

    /// <summary>Chế độ PROTECTED: đọc phần đầu HTTP với ba lớp chặn nhỏ-giọt-tiêu-đề.
    /// (1) hạn chót tuyệt đối HanDocTieuDe cho toàn bộ phần đầu;
    /// (2) hạn chót rỗi HanChotRoi giữa hai lần nhận byte (nhỏ giọt -> hết giờ);
    /// (3) tổng cỡ tiêu đề không vượt CoTieuDeToiDa.
    /// Trả về true nếu nhận đủ phần đầu (kết thúc bằng CRLFCRLF), false nếu phải đóng sớm.</summary>
    static bool DocTieuDeCoHan(Socket conn)
    {
        var du = new List<byte>();
        var sw = Stopwatch.StartNew();
        double hanTuyetDoi = HanDocTieuDe;
        var buf = new byte[1024];
        while (!KetThucTieuDe(du))
        {
            double conLai = hanTuyetDoi - sw.Elapsed.TotalSeconds;
            if (conLai <= 0) return false;                               // (1) quá hạn tổng
            conn.ReceiveTimeout = (int)(Math.Min(HanChotRoi, conLai) * 1000);   // (2) hạn chót rỗi
            int n;
            try { n = conn.Receive(buf); }
            catch (SocketException) { return false; }                   // hết giờ chờ -> SocketException
            if (n == 0) return false;                                   // client đóng giữa chừng
            du.AddRange(buf.AsSpan(0, n).ToArray());
            if (du.Count > CoTieuDeToiDa) return false;                 // (3) tiêu đề quá to
        }
        return true;
    }

    /// <summary>Chế độ VULNERABLE: đọc phần đầu HTTP, KHÔNG đặt hạn -> một client nhỏ giọt giữ worker mãi.</summary>
    static bool DocTieuDeKhongHan(Socket conn)
    {
        conn.ReceiveTimeout = 0;
        var du = new List<byte>();
        var buf = new byte[1024];
        while (!KetThucTieuDe(du))
        {
            int n = conn.Receive(buf);
            if (n == 0) return false;
            du.AddRange(buf.AsSpan(0, n).ToArray());
        }
        return true;
    }

    static bool KetThucTieuDe(List<byte> d)
    {
        for (int i = 3; i < d.Count; i++)
            if (d[i - 3] == '\r' && d[i - 2] == '\n' && d[i - 1] == '\r' && d[i] == '\n') return true;
        return false;
    }

    /// <summary>Một luồng trong nhóm: lặp lấy kết nối từ Accept và phục vụ.</summary>
    static void Worker(Socket sock, string mode, ThongKe stat, Func<bool> dangChay, Dictionary<string, int> demIp)
    {
        while (dangChay())
        {
            Socket conn;
            try { conn = sock.Accept(); }
            catch (SocketException) { return; }
            catch (ObjectDisposedException) { return; }
            lock (stat.Khoa) stat.ChapNhan++;
            string ip = ((IPEndPoint)conn.RemoteEndPoint!).Address.ToString();
            if (mode == "protected")
            {
                // Trần toàn cục và giới hạn theo IP: vượt -> 503 rồi đóng ngay (giải phóng worker).
                bool quaTai;
                lock (stat.Khoa)
                {
                    int tong = demIp.Values.Sum();
                    int rieng = demIp.GetValueOrDefault(ip);
                    quaTai = tong >= TranToanCuc || rieng >= GioiHanIp;
                    if (!quaTai) demIp[ip] = rieng + 1;
                }
                if (quaTai)
                {
                    lock (stat.Khoa) stat.TuChoi503++;
                    try { conn.Send(TraLoi503); } catch (SocketException) { }
                    finally { conn.Close(); }
                    continue;
                }
                try
                {
                    if (DocTieuDeCoHan(conn)) { conn.Send(TraLoi); lock (stat.Khoa) stat.Xong++; }
                    else lock (stat.Khoa) stat.DongCham++;               // đóng kết nối gửi tiêu đề quá chậm
                }
                catch (SocketException) { }
                finally
                {
                    lock (stat.Khoa)
                    {
                        demIp[ip] = demIp.GetValueOrDefault(ip, 1) - 1;
                        if (demIp[ip] <= 0) demIp.Remove(ip);
                    }
                    conn.Close();
                }
            }
            else  // vulnerable
            {
                try
                {
                    if (DocTieuDeKhongHan(conn)) { conn.Send(TraLoi); lock (stat.Khoa) stat.Xong++; }   // KẸT Ở ĐÂY nếu client nhỏ giọt
                }
                catch (SocketException) { }
                finally { conn.Close(); }
            }
        }
    }

    static int ChayServer(string mode, string host, int port)
    {
        var sock = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
        sock.Bind(new IPEndPoint(IPAddress.Parse(host), port));
        sock.Listen(256);
        var stat = new ThongKe();
        bool dangChay = true;
        var demIp = new Dictionary<string, int>();
        Console.WriteLine($"[server:{mode}] Nghe tại {host}:{port}, nhóm {SoLuongWorker} worker. Ctrl+C để dừng.");
        if (mode == "protected")
            Console.WriteLine(string.Format(CultureInfo.InvariantCulture,
                "[server:protected] Phòng thủ: hạn đọc tiêu đề {0}s, hạn rỗi {1}s, tiêu đề <= {2}B, <= {3} kết nối/IP, trần {4} kết nối.",
                HanDocTieuDe, HanChotRoi, CoTieuDeToiDa, GioiHanIp, TranToanCuc));
        else
            Console.WriteLine("[server:vulnerable] KHÔNG có hạn đọc tiêu đề: mỗi kết nối chậm giữ một worker.");

        var threads = new List<Thread>();
        for (int i = 0; i < SoLuongWorker; i++)
        {
            var t = new Thread(() => Worker(sock, mode, stat, () => Volatile.Read(ref dangChay), demIp)) { IsBackground = true };
            t.Start();
            threads.Add(t);
        }
        var stop = new ManualResetEventSlim(false);
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; stop.Set(); };
        stop.Wait();
        Volatile.Write(ref dangChay, false);
        sock.Close();
        Console.WriteLine($"[server:{mode}] Dừng. Tổng kết: chấp nhận {stat.ChapNhan}, phục vụ xong " +
                          $"{stat.Xong}, đóng vì chậm {stat.DongCham}, từ chối 503 {stat.TuChoi503}.");
        return 0;
    }

    // =============================== CLIENT THÍ NGHIỆM (chỉ loopback) ===============================
    /// <summary>Mở n kết nối và gửi phần đầu rất chậm, KHÔNG bao giờ kết thúc phần đầu.
    /// Chỉ chạy được khi host là loopback của chính máy bạn. Giữ kết nối khoảng 9 giây rồi thoát.
    /// Dùng địa chỉ nguồn loopback riêng (mặc định 127.0.0.2) để server phân biệt với probe.</summary>
    static int ChaySlow(string host, int port, int n, string nguon)
    {
        if (!LoopbackHayKhong(host))
        {
            Console.WriteLine($"[slow] TỪ CHỐI: đích {host} không phải loopback. " +
                              "Vai này chỉ dùng cho 127.0.0.1 trong phòng thí nghiệm của chính bạn.");
            return 1;
        }
        if (!IPAddress.IsLoopback(IPAddress.Parse(nguon)))
        {
            Console.WriteLine($"[slow] TỪ CHỐI: địa chỉ nguồn {nguon} không phải loopback.");
            return 1;
        }
        Console.WriteLine($"[slow] (CHỈ THÍ NGHIỆM) mở {n} kết nối gửi chậm tới {host}:{port} từ nguồn {nguon} trong ~9s");
        var socks = new List<Socket>();
        for (int i = 0; i < n; i++)
        {
            try
            {
                var s = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
                s.Bind(new IPEndPoint(IPAddress.Parse(nguon), 0));
                var ar = s.BeginConnect(host, port, null, null);
                if (!ar.AsyncWaitHandle.WaitOne(2000)) { s.Close(); continue; }
                s.EndConnect(ar);
                s.Send(Encoding.ASCII.GetBytes("GET /?slow HTTP/1.1\r\nHost: localhost\r\n"));  // phần đầu CHƯA kết thúc
                socks.Add(s);
            }
            catch (SocketException) { }
        }
        Console.WriteLine($"[slow] đã mở được {socks.Count} kết nối; bắt đầu gửi mỗi ~7s một byte");
        var sw = Stopwatch.StartNew();
        while (sw.Elapsed.TotalSeconds < 9.0)
        {
            Thread.Sleep(7000);
            int song = 0;
            foreach (var s in socks.ToList())
            {
                try { s.Send(Encoding.ASCII.GetBytes("X")); song++; }   // một byte rác, giữ kết nối "còn sống"
                catch (SocketException) { socks.Remove(s); }
            }
            Console.WriteLine($"[slow] còn {song} kết nối đang giữ");
        }
        foreach (var s in socks) s.Close();
        Console.WriteLine("[slow] đóng toàn bộ kết nối thí nghiệm");
        return 0;
    }

    // =============================== PROBE (yêu cầu bình thường) ===============================
    /// <summary>Gửi một yêu cầu HTTP/1.1 bình thường, đo xem có được phục vụ không (hạn 4s).</summary>
    static int ChayProbe(string host, int port)
    {
        var t0 = Stopwatch.StartNew();
        try
        {
            using var s = new Socket(AddressFamily.InterNetwork, SocketType.Stream, ProtocolType.Tcp);
            var ar = s.BeginConnect(host, port, null, null);
            if (!ar.AsyncWaitHandle.WaitOne(4000)) throw new SocketException((int)SocketError.TimedOut);
            s.EndConnect(ar);
            s.ReceiveTimeout = 4000;
            s.Send(Encoding.ASCII.GetBytes("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"));
            var du = new List<byte>();
            var buf = new byte[1024];
            while (!KetThucTieuDe(du))
            {
                int nn = s.Receive(buf);
                if (nn == 0) break;
                du.AddRange(buf.AsSpan(0, nn).ToArray());
            }
            double dt = t0.Elapsed.TotalMilliseconds;
            var data = du.ToArray();
            int crlf = Array.IndexOf(data, (byte)'\r');
            string dongDau = data.Length == 0 ? "(rỗng)"
                : Encoding.Latin1.GetString(data, 0, crlf >= 0 ? crlf : data.Length);
            if (dongDau.StartsWith("HTTP/1.1 200"))
                Console.WriteLine($"[probe] THÀNH CÔNG sau {dt:0} ms: {dongDau}");
            else
                Console.WriteLine($"[probe] phản hồi khác: {dongDau} ({dt:0} ms)");
        }
        catch (SocketException e) when (e.SocketErrorCode is SocketError.TimedOut)
        {
            double dt = t0.Elapsed.TotalMilliseconds;
            Console.WriteLine($"[probe] THẤT BẠI: hết thời gian chờ sau {dt:0} ms – server không phục vụ kịp (bị làm nghẽn).");
        }
        catch (SocketException e)
        {
            Console.WriteLine($"[probe] THẤT BẠI: {e.Message}");
        }
        return 0;
    }

    public static int Run(string[] a)
    {
        if (a.Length >= 4 && a[0] == "server")
            return ChayServer(a[1], a[2], int.Parse(a[3]));
        if (a.Length >= 3 && a[0] == "slow")
            return ChaySlow(a[1], int.Parse(a[2]), a.Length > 3 ? int.Parse(a[3]) : 150, a.Length > 4 ? a[4] : "127.0.0.2");
        if (a.Length >= 3 && a[0] == "probe")
            return ChayProbe(a[1], int.Parse(a[2]));
        Console.WriteLine("Cách dùng:");
        Console.WriteLine("  dotnet run -- server vulnerable|protected <host> <port>");
        Console.WriteLine("  dotnet run -- slow  <host> <port> [N]   (chỉ 127.0.0.1)");
        Console.WriteLine("  dotnet run -- probe <host> <port>");
        return 1;
    }
}
