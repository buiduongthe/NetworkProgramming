using System.Text.Json.Nodes;

namespace BaiMR6_8;

// Bài mở rộng MR6.8 – Người đặt vé xem phim (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (ghế mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
//   giu E7 [E8 …]   giữ chỗ (có thời hạn)          tt [E7 …]    thanh toán (mặc định: các ghế vừa giữ)
//   nha [E7 …]      nhả ghế đang giữ               xem          in sơ đồ ghế          thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- dat_ve <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiDatVe
{
    /// <summary>Các dòng sơ đồ ghế: . trống, g người khác giữ, b đã bán, G mình giữ, V vé của mình.</summary>
    public static List<string> SoDoChu(JsonObject tt, string ten)
    {
        string hang = Chung.Chuoi(tt, "hang") ?? "";
        int cot = (int)Chung.SoL(tt, "cot");
        var chu = tt["chu"] as JsonObject;
        var ds = new List<string> { "      [ MÀN HÌNH ]", "   " + string.Join(" ", Enumerable.Range(1, cot).Select(c => $"{c % 10}")) };
        var soDo = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        for (int i = 0; i < Math.Min(hang.Length, soDo.Count); i++)
        {
            var o = new List<string>();
            for (int c = 0; c < soDo[i].Length; c++)
            {
                char x = soDo[i][c];
                bool cuaToi = Chung.Chuoi(chu, $"{hang[i]}{c + 1}") == ten;
                o.Add(cuaToi ? x switch { 'g' => "G", 'b' => "V", _ => x.ToString() } : x.ToString());
            }
            ds.Add($"{hang[i]}  " + string.Join(" ", o));
        }
        return ds;
    }

    readonly string ten;
    JsonObject tt = [];                                  // cap_nhat mới nhất
    List<string> gioHang = [];                           // các ghế vừa giữ được (để "tt" không cần gõ lại)
    long phienGio;                                       // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    HashSet<string> daBao = [];                          // ghế trong giỏ đã báo hết hạn / bị lấy
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiDatVe(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(CapNhat, TinKhac, MatKetNoi);
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        var d = tt["dem"] as JsonObject;
        Log($"Vào rạp: {Chung.Chuoi(tt, "phim")}, suất {Chung.Chuoi(tt, "suat")} ({Chung.Chuoi(tt, "che_do")})");
        Log($"  trống {Chung.SoL(d, "trong")}, giữ {Chung.SoL(d, "giu")}, bán {Chung.SoL(d, "ban")}; vé {Chung.Tien(Chung.SoL(tt, "gia_ve"))},"
            + $" giữ {Chung.G(Chung.SoD(tt, "ttl_s"))} s");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    /// <summary>Ghế trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat chụp
    /// SAU lần giữ của mình (tt["phien"] ≥ phien của kq). Giỏ vẫn giữ nguyên – lệnh "tt" sau đó sẽ bị máy chủ từ chối,
    /// đúng như khi trang thanh toán mở từ trước bấm "Thanh toán" quá hạn.</summary>
    void CapNhat(JsonObject moi)
    {
        List<string> gio;
        lock (khoa)
        {
            tt = moi;
            gio = [.. gioHang.Where(g => !daBao.Contains(g))];
            if (Chung.SoL(moi, "phien") < phienGio) return;
        }
        var chu = moi["chu"] as JsonObject;
        foreach (var g in gio)
        {
            string? ai = Chung.Chuoi(chu, g);
            if (ai == ten) continue;
            Log(ai == null ? $"  Hết thời gian giữ chỗ: {g} đã được nhả" : $"  Ghế {g} đã bị {ai} lấy mất!");
            lock (khoa) daBao.Add(g);
        }
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    void MatKetNoi() => Log("Mất kết nối máy chủ");

    static List<string> Ds(JsonObject tin, string k) => [.. (tin[k] as JsonArray ?? []).Select(Chung.ChuoiPy)];

    // ------------------------------------------------------------ gửi
    void Giu(List<string> ghe)
    {
        Log($"> giữ {string.Join(", ", ghe)}");
        Kn.GuiYc("giu_cho", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                lock (khoa)
                {
                    gioHang = Ds(tin, "ghe");
                    (phienGio, daBao) = (Chung.SoL(tin, "phien"), []);
                }
                Log($"  ĐÃ GIỮ ({Chung.F(rtt, "F1")} ms), thanh toán trong {Chung.F(Chung.SoD(tin, "het_han_ms") / 1000, "F0")} s");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, new JsonObject { ["ghe"] = PhongChieu.Json(ghe) });
    }

    void ThanhToan(List<string> ghe)
    {
        Log($"> thanh toán {string.Join(", ", ghe)}");
        Kn.GuiYc("thanh_toan", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                var da = Ds(tin, "ghe");
                lock (khoa) gioHang = [.. gioHang.Where(g => !da.Contains(g))];
                Log($"  ĐẶT VÉ THÀNH CÔNG ({Chung.F(rtt, "F1")} ms): {Chung.Tien(Chung.SoL(tin, "tien"))}");
                Log($"  mã vé {string.Join(", ", Ds(tin, "ma_ve"))}");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, new JsonObject { ["ghe"] = PhongChieu.Json(ghe) });
    }

    void Nha(List<string> ghe)
    {
        Log($"> nhả {string.Join(", ", ghe)}");
        Kn.GuiYc("nha", (tin, _) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                var da = Ds(tin, "ghe");
                lock (khoa) gioHang = [.. gioHang.Where(g => !da.Contains(g))];
                Log($"  Đã nhả {string.Join(", ", da)}");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, new JsonObject { ["ghe"] = PhongChieu.Json(ghe) });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            List<string> gio;
            lock (khoa) gio = [.. gioHang];
            if (p[0] == "giu" && p.Length >= 2) Giu([.. p[1..].Select(g => g.ToUpperInvariant())]);
            else if (p[0] is "tt" or "nha")
            {
                List<string> ghe = p.Length > 1 ? [.. p[1..].Select(g => g.ToUpperInvariant())] : gio;
                if (ghe.Count == 0) Log("chưa giữ ghế nào");
                else if (p[0] == "tt") ThanhToan(ghe);
                else Nha(ghe);
            }
            else if (p[0] == "xem")
            {
                JsonObject t;
                lock (khoa) t = tt;
                foreach (var d in SoDoChu(t, ten)) Log(d);
            }
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: giu E7 [E8 …] | tt [ghế…] | nha [ghế…] | xem | thoat");
        }
        catch (Exception e) when (e is IOException or System.Net.Sockets.SocketException)
        {
            Log($"lỗi: {e.Message}");
        }
    }

    public static int Run(string[] args)
    {
        var a = new DoiSo(args);
        if (a.ViTri.Count < 1)
        {
            Console.WriteLine("Dùng: dat_ve <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiDatVe(a.ViTri[0]);
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
            Thread.Sleep(300);                           // chờ trả lời của lệnh cuối
        }
        ng.Kn.KhiDong = () => { };
        ng.Kn.Dong();
        return 0;
    }
}
