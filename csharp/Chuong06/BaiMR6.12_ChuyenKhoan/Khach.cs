using System.Diagnostics;
using System.Net.Sockets;
using System.Text.Json.Nodes;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Lõi client dùng chung (giao diện người dùng, bộ mô phỏng, quản trị) + dòng lệnh quản trị.
//
// Tệp này GIỐNG NHAU ở các bài tranh chấp (MR6.6–MR6.13, chỉ khác dòng namespace). Không có gì về nghiệp vụ.
//
// KetNoiKhach – một kết nối TCP:
//   KetNoi(...)       chào, đọc trả lời chào và cap_nhat đầu tiên (đồng bộ) -> TrangThai
//   GuiYc(loai, khiKq, truong)   gửi một yêu cầu (tự đánh ma_yc), KHÔNG chờ: khiKq(tin, rttMs) được gọi khi có
//                     trả lời cùng ma_yc (pipelining: gửi tiếp được ngay)
//   GuiLo([...])      gửi nhiều yêu cầu trong MỘT lần Send (bộ mô phỏng "bắn đồng loạt")
//   Hoi(loai, truong) gửi rồi CHỜ trả lời (cần luồng đọc chạy nền) – dùng cho quản trị
//   BatDauDoc()       luồng đọc chạy nền (giao diện);  VongDoc(xong)  đọc ngay trên luồng gọi (mô phỏng)
// Gọi lại (từ luồng đọc): KhiCapNhat(tin), KhiTin(tin) (su_kien, loi, trả lời lạ), KhiDong().
// QuanTri – kết nối vai_tro "quan_tri": CheDo(), Tre(), DatLai(ts), ThongKe().
//
// Dòng lệnh quản trị:  dotnet run -- khach <host> <cổng> che_do khoa | tre 2 | dat_lai k=v … | thong_ke
class KetNoiKhach(Action<JsonObject>? khiCapNhat = null, Action<JsonObject>? khiTin = null, Action? khiDong = null)
{
    public Action<JsonObject> KhiCapNhat = khiCapNhat ?? (_ => { });
    public Action<JsonObject> KhiTin = khiTin ?? (_ => { });
    public Action KhiDong = khiDong ?? (() => { });
    Socket? sock;
    DocDong? doc;
    volatile bool mo;
    public JsonObject? Chao;
    public volatile JsonObject? TrangThai;
    long ma;
    readonly Dictionary<long, (Action<JsonObject, double>? KhiKq, long T0)> cho = [];   // ma_yc -> (gọi lại, lúc gửi)
    readonly object khoa = new();                       // bảo vệ cho và việc gửi

    public bool Mo => mo;

    // ------------------------------------------------------------ kết nối
    public JsonObject KetNoi(string host, int cong, string ten, string vaiTro = "nguoi_dung", long? soNguoi = null,
                             double choS = 5.0)
    {
        var ip = Chung.DiaChiIp(host);
        var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        try
        {
            if (!s.ConnectAsync(ip, cong).Wait(TimeSpan.FromSeconds(choS))) throw new IOException("hết thời gian kết nối");
            s.ReceiveTimeout = (int)(choS * 1000);
            var chao = new JsonObject { ["loai"] = "chao", ["ten"] = ten, ["vai_tro"] = vaiTro };
            if (soNguoi != null) chao["so_nguoi"] = soNguoi.Value;
            Chung.GuiJson(s, chao);
            var d = new DocDong(new NetworkStream(s, ownsSocket: false));
            var tin = d.Doc();
            if (tin == null || Chung.Chuoi(tin, "loai") != "chao")
                throw new IOException(Chung.Chuoi(tin, "ly_do") ?? "máy chủ đóng kết nối");
            Chao = tin;
            TrangThai = d.Doc();                          // máy chủ gửi ngay một cap_nhat sau lời chào
            s.ReceiveTimeout = 0;
            (sock, doc, mo) = (s, d, true);
            return tin;
        }
        catch (AggregateException e)
        {
            s.Close();
            throw new IOException(e.InnerException?.Message ?? e.Message);
        }
        catch
        {
            s.Close();
            throw;
        }
    }

    public void Dong()
    {
        mo = false;
        if (sock == null) return;
        try { sock.Shutdown(SocketShutdown.Both); }
        catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        sock.Close();
    }

    // ------------------------------------------------------------ gửi
    JsonObject DangKy(string loai, Action<JsonObject, double>? khiKq, JsonObject? truong)
    {
        long m = ++ma;
        cho[m] = (khiKq, Stopwatch.GetTimestamp());
        var tin = new JsonObject { ["loai"] = loai, ["ma_yc"] = m };
        if (truong != null)
            foreach (var (k, v) in truong) tin[k] = v?.DeepClone();
        return tin;
    }

    public long GuiYc(string loai, Action<JsonObject, double>? khiKq = null, JsonObject? truong = null)
    {
        lock (khoa)
        {
            var tin = DangKy(loai, khiKq, truong);
            Chung.GuiJson(sock!, tin);
            return ma;
        }
    }

    /// <summary>ds: danh sách (loai, truong, khiKq). Một lần Send cho cả lô.</summary>
    public void GuiLo(IEnumerable<(string Loai, JsonObject Truong, Action<JsonObject, double>? KhiKq)> ds)
    {
        lock (khoa)
        {
            var ms = new MemoryStream();
            foreach (var (loai, truong, khiKq) in ds) ms.Write(Chung.MaHoa(DangKy(loai, khiKq, truong)));
            Chung.GuiHet(sock!, ms.ToArray());
        }
    }

    public JsonObject Hoi(string loai, JsonObject? truong = null, double choS = 30.0)
    {
        var xong = new ManualResetEventSlim();
        JsonObject? kq = null;
        GuiYc(loai, (tin, _) =>
        {
            kq = tin;
            xong.Set();
        }, truong);
        if (!xong.Wait(TimeSpan.FromSeconds(choS))) throw new TimeoutException($"không có trả lời cho {loai}");
        return kq!;
    }

    // ------------------------------------------------------------ đọc
    public void BatDauDoc() => new Thread(() => VongDoc()) { IsBackground = true, Name = "doc" }.Start();

    /// <summary>Đọc và phân phát tới khi kết nối đóng, hoặc (nếu có) xong() đúng sau một thông điệp.</summary>
    public void VongDoc(Func<bool>? xong = null)
    {
        try
        {
            while (true)
            {
                if (xong != null && xong()) return;      // đủ việc: kết nối vẫn mở, người gọi tự đóng
                var tin = doc!.Doc();
                if (tin == null) break;
                PhanPhat(tin);
            }
        }
        catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ObjectDisposedException) { }
        mo = false;
        KhiDong();
    }

    void PhanPhat(JsonObject tin)
    {
        (Action<JsonObject, double>? KhiKq, long T0)? c = null;
        if (Chung.LaSoNguyen(tin["ma_yc"], out long m) && !Chung.Dung(tin, "lap_lai"))
            lock (khoa)
                if (cho.Remove(m, out var x)) c = x;
        if (c is { } cc)
            cc.KhiKq?.Invoke(tin, Stopwatch.GetElapsedTime(cc.T0).TotalMilliseconds);
        else if (Chung.Chuoi(tin, "loai") == "cap_nhat")
        {
            TrangThai = tin;
            KhiCapNhat(tin);
        }
        else KhiTin(tin);
    }
}

/// <summary>Kết nối quản trị (đồng bộ): mỗi lệnh chờ trả lời; bị từ chối -> ArgumentException.</summary>
sealed class QuanTri : KetNoiKhach
{
    public QuanTri(string host, int cong, string ten = "quan_tri")
    {
        KetNoi(host, cong, ten, "quan_tri");
        BatDauDoc();
    }

    JsonObject Lenh(string loai, JsonObject? truong = null)
    {
        var kq = Hoi(loai, truong);
        if (kq["ok"] is not null && !Chung.Dung(kq, "ok")) throw new ArgumentException(Chung.Chuoi(kq, "ly_do") ?? "bị từ chối");
        return kq;
    }

    public JsonObject CheDo(string giaTri) => Lenh("che_do", new JsonObject { ["gia_tri"] = giaTri });

    public JsonObject Tre(double ms) => Lenh("tre", new JsonObject { ["ms"] = ms });

    public JsonObject DatLai(IEnumerable<KeyValuePair<string, object>> thamSo)
    {
        var o = new JsonObject();
        foreach (var (k, v) in thamSo)
            o[k] = v switch { long l => JsonValue.Create(l), double d => JsonValue.Create(d), _ => JsonValue.Create(Chung.Repr(v)) };
        return Lenh("dat_lai", o);
    }

    public JsonObject ThongKe() => Lenh("thong_ke");

    /// <summary>dotnet run -- khach &lt;host&gt; &lt;cổng&gt; che_do &lt;x&gt; | tre &lt;ms&gt; | dat_lai k=v … | thong_ke</summary>
    public static int Run(string[] a)
    {
        if (a.Length < 3)
        {
            Console.WriteLine("Dùng: khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke");
            return 2;
        }
        string host = a[0], lenh = a[2];
        int cong = int.Parse(a[1], Chung.Ic);
        var ts = a[3..];
        static void Bao(string text) => Console.WriteLine($"[quan_tri] {text}");
        var qt = new QuanTri(host, cong);
        try
        {
            Bao($"> {string.Join(' ', [lenh, .. ts])}");
            if (lenh == "che_do" && ts.Length == 1) qt.CheDo(ts[0]);
            else if (lenh == "tre" && ts.Length == 1) qt.Tre(double.Parse(ts[0], Chung.Ic));
            else if (lenh == "dat_lai") qt.DatLai(Chung.DocThamSo(ts));
            else if (lenh != "thong_ke")
            {
                Bao("lệnh không hợp lệ");
                return 2;
            }
            Bao("OK");
            if (lenh == "thong_ke")
                foreach (var d in Chung.DongThongKe(qt.ThongKe())) Bao(d);
        }
        catch (Exception e) when (e is ArgumentException or FormatException)
        {
            Bao($"Bị từ chối: {e.Message}");
        }
        finally { qt.Dong(); }
        return 0;
    }
}
