using System.Text.Json.Nodes;

namespace BaiMR6_11;

// Bài mở rộng MR6.11 – Người mua flash sale (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (mở bán, hết hàng, đơn của người khác) và nhận lệnh:
//   mua [n]     bấm "MUA NGAY" n sản phẩm (mặc định 1)          thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- flash_sale <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiMua
{
    readonly string ten;
    JsonObject tt = [];
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiMua(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(CapNhat, TinKhac, () => Log("Mất kết nối máy chủ"));
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        var sp = tt["sp"] as JsonObject;
        Log($"Vào trang sale ({Chung.Chuoi(tt, "che_do")}): {Chung.Chuoi(sp, "ten")}");
        Log($"  {Chung.Tien(Chung.SoL(sp, "gia_goc"))} -> {Chung.Tien(Chung.SoL(sp, "gia_sale"))}, kho {Chung.SoL(tt, "ton")}/{Chung.SoL(tt, "ton_dau")}");
        if (!Chung.Dung(tt, "mo_ban")) Log($"  mở bán sau {Chung.F(Chung.SoD(tt, "mo_con_ms") / 1000, "F1")} s");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        JsonObject cu;
        lock (khoa) (cu, tt) = (tt, moi);
        if (Chung.SoL(moi, "ton") == 0 && Chung.SoL(cu, "ton") != 0) Log("  kho: HẾT HÀNG");
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    // ------------------------------------------------------------ gửi
    void Mua(long sl)
    {
        Log($"> MUA NGAY {sl} sản phẩm");
        Kn.GuiYc("mua", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
                Log($"  ĐẶT HÀNG OK: đơn {Chung.Chuoi(tin, "ma_don")}, {Chung.Tien(Chung.SoL(tin, "thanh_tien"))} ({Chung.F(rtt, "F1")} ms)");
            else Log($"  TỪ CHỐI: {Chung.ChuoiPy(tin["ly_do"])}");
        }, new JsonObject { ["so_luong"] = sl });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p[0] == "mua" && p.Length <= 2) Mua(p.Length == 2 ? long.Parse(p[1], Chung.Ic) : 1);
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: mua [n] | thoat");
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
            Console.WriteLine("Dùng: flash_sale <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiMua(a.ViTri[0]);
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
