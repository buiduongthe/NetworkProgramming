using System.Text.Json.Nodes;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Khách hàng ngân hàng (bản dòng lệnh).
//
// Đăng nhập một tài khoản (mặc định tài khoản mang tên mình, không có thì TK01), in tiền đến và nhận lệnh:
//   chuyen <TK> <số tiền> [nội dung]     chuyển khoản          so_du     xem số dư          thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class KhachHang
{
    readonly string ten;
    string? tk;
    JsonObject tt = [];
    HashSet<long> daThay = [];                           // ma_gd đã biết (không báo tiền đến hai lần)
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public KhachHang(string ten, string? tk = null)
    {
        (this.ten, this.tk) = (ten, tk);
        Kn = new KetNoiKhach(CapNhat, TinKhac, () => Log("Mất kết nối máy chủ"));
    }

    void Log(string text) => InDong.In(ten, text);

    static IEnumerable<JsonObject> TaiKhoan(JsonObject tt) => (tt["tai_khoan"] as JsonArray ?? []).OfType<JsonObject>();

    long SoDu(JsonObject? t = null) => Chung.SoL(TaiKhoan(t ?? tt).FirstOrDefault(x => Chung.Chuoi(x, "ma") == tk), "so_du");

    string Chu(string? ma) => Chung.Chuoi(TaiKhoan(tt).FirstOrDefault(x => Chung.Chuoi(x, "ma") == ma), "chu") ?? "?";

    static IEnumerable<JsonObject> LichSu(JsonObject tt) => (tt["lich_su"] as JsonArray ?? []).OfType<JsonObject>();

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        tk ??= Chung.Chuoi(TaiKhoan(tt).FirstOrDefault(x => Chung.Chuoi(x, "chu") == ten) ?? TaiKhoan(tt).First(), "ma");   // tài khoản mang tên mình, không có thì TK01
        daThay = [.. LichSu(tt).Select(g => Chung.SoL(g, "ma"))];
        Log($"Đăng nhập {tk} – {Chu(tk)} ({Chung.Chuoi(tt, "che_do")})");
        Log($"  số dư {Chung.Tien(SoDu())}");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        List<JsonObject> dsMoi;
        lock (khoa)
        {
            tt = moi;
            dsMoi = [.. LichSu(moi).Reverse().Where(g => !daThay.Contains(Chung.SoL(g, "ma")))];
            daThay.UnionWith(dsMoi.Select(g => Chung.SoL(g, "ma")));
        }
        foreach (var g in dsMoi)
            if (Chung.Chuoi(g, "den") == tk)
                Log($"+ Nhận {Chung.Tien(Chung.SoL(g, "so_tien"))} từ {Chung.Chuoi(g, "tu")} ({Chu(Chung.Chuoi(g, "tu"))}),"
                    + $" số dư {Chung.Tien(SoDu(moi))}");
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    // ------------------------------------------------------------ gửi
    void Chuyen(string den, long soTien, string noiDung = "")
    {
        Log($"> chuyển {Chung.Tien(soTien)} {tk}→{den}");
        Kn.GuiYc("chuyen", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                lock (khoa) daThay.Add(Chung.SoL(tin, "ma_gd"));
                Log($"  THÀNH CÔNG ({Chung.F(rtt, "F1")} ms), số dư {Chung.Tien(Chung.SoL(tin, "so_du"))}");
            }
            else Log($"  TỪ CHỐI ({Chung.F(rtt, "F0")} ms): {Chung.ChuoiPy(tin["ly_do"])}");
        }, new JsonObject { ["tu"] = tk, ["den"] = den, ["so_tien"] = soTien, ["noi_dung"] = noiDung });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p[0] == "chuyen" && p.Length >= 3) Chuyen(p[1], long.Parse(p[2], Chung.Ic), string.Join(' ', p[3..]));
            else if (p[0] == "so_du")
            {
                lock (khoa) Log($"Số dư {tk}: {Chung.Tien(SoDu())}");
            }
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: chuyen <TK> <số tiền> [nội dung] | so_du | thoat");
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
            Console.WriteLine("Dùng: ngan_hang <tên> [host] [cổng] [--tk TK01] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var kh = new KhachHang(a.ViTri[0], a.Lay("--tk"));
        kh.KetNoi(a.VT(1, "127.0.0.1"), int.Parse(a.VT(2, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic));
        var hetStdin = new ManualResetEventSlim();
        var dung = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            dung.Set();
        };
        new Thread(() => Chung.DocLenhStdin(t0, kh.Lenh, hetStdin.Set)) { IsBackground = true }.Start();
        if (thoiGian > 0)                                // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
        {
            while (kh.Kn.Mo && Chung.Now - t0 < thoiGian && !dung.IsSet) Thread.Sleep(100);
        }
        else                                             // tới khi hết stdin / thoat
        {
            while (kh.Kn.Mo && !hetStdin.Wait(100) && !dung.IsSet) { }
            Thread.Sleep(300);                           // chờ trả lời của lệnh cuối
        }
        kh.Kn.KhiDong = () => { };
        kh.Kn.Dong();
        return 0;
    }
}
