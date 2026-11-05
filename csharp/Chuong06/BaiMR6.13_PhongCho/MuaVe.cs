using System.Text.Json.Nodes;

namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Người mua vé (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (vị trí trong hàng chờ, tới lượt, kết quả mua) và nhận lệnh:
//   xep_hang     vào phòng chờ (rồi "chờ dài" cho_luot tới khi tới lượt)
//   mua <n>      mua n vé            mua2 <n>   "bấm đúp": gửi HAI yêu cầu mua liên tiếp không chờ          thoat
// --tu-dong: vào hàng ngay khi kết nối (phòng chờ tắt thì mua ngay), tới lượt thì tự mua --so-ve vé.
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]   (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiMuaVe
{
    readonly string ten;
    readonly bool tuDong;
    readonly long soVe;
    JsonObject tt = [];
    long? so;                                            // số thứ tự trong hàng
    long? truocCu;
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiMuaVe(string ten, bool tuDong = false, long soVe = 2)
    {
        (this.ten, this.tuDong, this.soVe) = (ten, tuDong, soVe);
        Kn = new KetNoiKhach(CapNhat, TinKhac, () => Log("Mất kết nối máy chủ"));
    }

    void Log(string text) => InDong.In(ten, text);

    public bool PhongCho
    {
        get { lock (khoa) return Chung.Dung(tt, "phong_cho"); }
    }

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        var sk = tt["su_kien"] as JsonObject;
        Log($"Trang bán vé: {Chung.Chuoi(sk, "ten")} ({Chung.Chuoi(tt, "che_do")})");
        Log($"  còn {Chung.SoL(tt, "con_lai")}/{Chung.SoL(sk, "tong_ve")} vé, {Chung.Tien(Chung.SoL(sk, "gia_ve"))}/vé");
        Log("  phòng chờ " + (Chung.Dung(tt, "phong_cho") ? $"BẬT, cấp lượt {Chung.G(Chung.SoD(tt, "toc_do"))} người/s" : "TẮT"));
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        long truoc;
        bool doi;
        lock (khoa)
        {
            tt = moi;
            if (so is not { } s || truocCu is not { } cu) return;
            truoc = Math.Max(0, s - Chung.SoL(moi, "dang_phuc_vu") - 1);
            (doi, truocCu) = (truoc != cu, truoc);
        }
        if (doi && truoc > 0)
            Log($"  (hàng chờ) còn {truoc} người phía trước, ~{Chung.F((truoc + 1) / Chung.SoD(moi, "toc_do", 1), "F1")} s");
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    // ------------------------------------------------------------ gửi
    public void XepHang()
    {
        Log("> vào phòng chờ");

        void DenLuot(JsonObject tin, double rtt)
        {
            lock (khoa) truocCu = null;
            if (!Chung.Dung(tin, "ok"))
            {
                Log($"  {Chung.ChuoiPy(tin["ly_do"])}");
                return;
            }
            Log($"  ĐẾN LƯỢT! (chờ {Chung.F(rtt / 1000, "F1")} s) còn {Chung.F(Chung.SoD(tin, "han_ms") / 1000, "F1")} s để mua");
            if (tuDong) Mua(soVe);
        }

        Kn.GuiYc("xep_hang", (tin, rtt) =>
        {
            if (!Chung.Dung(tin, "ok"))
            {
                Log($"  TỪ CHỐI: {Chung.ChuoiPy(tin["ly_do"])}");
                return;
            }
            lock (khoa) (so, truocCu) = (Chung.SoL(tin, "so"), Chung.SoL(tin, "truoc"));
            Log($"  số thứ tự #{Chung.SoL(tin, "so")}, còn {Chung.SoL(tin, "truoc")} người phía trước, ~{Chung.F(Chung.SoD(tin, "cho_s"), "F1")} s");
            Kn.GuiYc("cho_luot", DenLuot);
        });
    }

    public void Mua(long n, int lan = 1)
    {
        Log($"> mua {n} vé" + (lan > 1 ? " (bấm đúp)" : ""));
        for (int i = 0; i < lan; i++)                    // bấm đúp: hai yêu cầu, không chờ trả lời
            Kn.GuiYc("mua", (tin, rtt) =>
            {
                if (Chung.Dung(tin, "ok"))
                    Log($"  THÀNH CÔNG ({Chung.F(rtt, "F1")} ms): đơn #{Chung.SoL(tin, "ma_don")}, {Chung.SoL(tin, "so_ve")} vé,"
                        + $" {Chung.Tien(Chung.SoL(tin, "thanh_tien"))}");
                else Log($"  TỪ CHỐI ({Chung.F(rtt, "F1")} ms): {Chung.ChuoiPy(tin["ly_do"])}");
            }, new JsonObject { ["so_ve"] = n });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p[0] == "xep_hang") XepHang();
            else if (p[0] is "mua" or "mua2" && p.Length == 2) Mua(long.Parse(p[1], Chung.Ic), p[0] == "mua2" ? 2 : 1);
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: xep_hang | mua <n> | mua2 <n> | thoat");
        }
        catch (Exception e) when (e is FormatException or OverflowException or IOException or System.Net.Sockets.SocketException)
        {
            Log($"lỗi: {e.Message}");
        }
    }

    public static int Run(string[] args)
    {
        var a = new DoiSo(args, "--tu-dong");
        if (a.ViTri.Count < 1)
        {
            Console.WriteLine("Dùng: mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiMuaVe(a.ViTri[0], a.Co("--tu-dong"), a.Long("--so-ve", 2));
        ng.KetNoi(a.VT(1, "127.0.0.1"), int.Parse(a.VT(2, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic));
        if (a.Co("--tu-dong"))
        {
            if (ng.PhongCho) ng.XepHang();
            else ng.Mua(a.Long("--so-ve", 2));
        }
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
            Thread.Sleep(300);                           // chờ trả lời của lệnh cuối
        }
        ng.Kn.KhiDong = () => { };
        ng.Kn.Dong();
        return 0;
    }
}
