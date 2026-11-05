using System.Text.Json.Nodes;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Người tham gia vòng quay may mắn (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (ai trúng gì, số quà còn lại) và nhận lệnh:
//   quay        quay một lượt
//   quay x2     "bấm đúp": gửi HAI yêu cầu quay liên tiếp không chờ (hai ma_yc khác nhau) – máy chủ phải chấp
//               nhận đúng một (idempotency theo tên người)
//   gui_lai [k] gửi lại ĐÚNG ma_yc k (mặc định: của lượt quay gần nhất), như khi mạng chập chờn client gửi lại –
//               máy chủ không quay lại mà trả lại kết quả cũ kèm "lap_lai": true (idempotency theo ma_yc)
//   thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- quay_so <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiQuay
{
    public static string MoTaKq(JsonObject tin)
    {
        if (Chung.Dung(tin, "ok")) return Chung.Dung(tin, "trung") ? $"TRÚNG {Chung.Chuoi(tin, "ten_qua")}!" : "Chúc may mắn lần sau";
        return $"TỪ CHỐI: {Chung.ChuoiPy(tin["ly_do"])}";
    }

    readonly string ten;
    JsonObject tt = [];
    long? maCuoi;                                        // ma_yc của lượt quay gần nhất (cho gui_lai)
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiQuay(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(CapNhat, TinKhac, () => Log("Mất kết nối máy chủ"));
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        Log($"Vào vòng quay ({Chung.Chuoi(tt, "che_do")}): còn {Chung.SoL(tt, "con_lai")}/{Chung.SoL(tt, "so_qua")} quà,"
            + $" p {Qua.ChuP(Chung.SoD(tt, "p"))}");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        JsonObject cu;
        lock (khoa) (cu, tt) = (tt, moi);
        if (Chung.SoL(moi, "con_lai") != Chung.SoL(cu, "con_lai"))
            Log($"  còn {Chung.SoL(moi, "con_lai")}/{Chung.SoL(moi, "so_qua")} quà");
    }

    void TinKhac(JsonObject tin)
    {
        string? loai = Chung.Chuoi(tin, "loai");
        if (loai == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
        else if (loai == "kq" && Chung.Dung(tin, "lap_lai")) Log($"  (ma_yc {Chung.ChuoiPy(tin["ma_yc"])}, lap_lai) {MoTaKq(tin)}");
    }

    // ------------------------------------------------------------ gửi
    void Quay()
    {
        long ma = Kn.GuiYc("quay", (tin, rtt) => Log($"  ma_yc {Chung.ChuoiPy(tin["ma_yc"])}: {MoTaKq(tin)} ({Chung.F(rtt, "F1")} ms)"));
        lock (khoa) maCuoi = ma;
    }

    /// <summary>Gửi lại một ma_yc cũ: trường "ma_yc" truyền vào đè lên số mà KetNoiKhach tự đánh, nên dòng gửi đi
    /// mang đúng ma_yc k (như gửi thô). Trả lời "lap_lai" không khớp yêu cầu đang chờ nào -> đi vào TinKhac.</summary>
    void GuiLai(long? ma)
    {
        lock (khoa) ma ??= maCuoi;
        if (ma == null)
        {
            Log("chưa quay lượt nào");
            return;
        }
        Log($"> gửi lại ma_yc {ma}");
        Kn.GuiYc("quay", null, new JsonObject { ["ma_yc"] = ma });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p is ["quay"])
            {
                Log("> quay");
                Quay();
            }
            else if (p is ["quay", "x2"])
            {
                Log("> quay x2 (bấm đúp: 2 yêu cầu liền nhau)");
                Quay();
                Quay();
            }
            else if (p[0] == "gui_lai" && p.Length <= 2) GuiLai(p.Length == 2 ? long.Parse(p[1], Chung.Ic) : null);
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: quay | quay x2 | gui_lai [k] | thoat");
        }
        catch (Exception e) when (e is FormatException or OverflowException or IOException or System.Net.Sockets.SocketException)
        {
            Log($"lỗi: {e.Message}");
        }
    }

    public static int Run(string[] args)
    {
        var a = new DoiSo(args);
        if (a.ViTri.Count < 1)
        {
            Console.WriteLine("Dùng: quay_so <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiQuay(a.ViTri[0]);
        ng.KetNoi(a.VT(1, "127.0.0.1"), int.Parse(a.VT(2, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic));
        var hetStdin = new ManualResetEventSlim();
        var dung = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            dung.Set();
        };
        new Thread(() => Chung.DocLenhStdin(t0, ng.Lenh, hetStdin.Set)) { IsBackground = true }.Start();
        if (thoiGian > 0)                                // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
        {
            while (ng.Kn.Mo && Chung.Now - t0 < thoiGian && !dung.IsSet) Thread.Sleep(100);
        }
        else                                             // tới khi hết stdin / thoat
        {
            while (ng.Kn.Mo && !hetStdin.Wait(100) && !dung.IsSet) { }
            Thread.Sleep(500);                           // chờ trả lời của lệnh cuối
        }
        ng.Kn.KhiDong = () => { };
        ng.Kn.Dong();
        return 0;
    }
}
