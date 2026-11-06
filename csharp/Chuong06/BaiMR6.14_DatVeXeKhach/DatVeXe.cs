using System.Text.Json.Nodes;

namespace BaiMR6_14;

// Bài mở rộng MR6.14 – Người đặt vé xe khách (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (chỗ mình giữ hết hạn, bị người khác lấy) và nhận lệnh:
//   giu SG DL D01 [D02 …]   giữ chỗ đoạn Sài Gòn → Đà Lạt (mã trạm SG, BL, DL, NT; có thời hạn)
//   tt                      thanh toán các chỗ vừa giữ          nha       nhả các chỗ vừa giữ
//   xem                     in sơ đồ xe (mỗi chỗ 3 ký tự, một ký tự mỗi chặng)            thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiDatVeXe
{
    /// <summary>Người giữ / mua ô "D01#1" theo cap_nhat (null = trống).</summary>
    public static string? ChuO(JsonObject? tt, string o) => Chung.Chuoi(tt?["chu"] as JsonObject, o);

    /// <summary>Các dòng sơ đồ xe. Mỗi chỗ: mã rồi một ký tự mỗi chặng (. trống, g người khác giữ, b đã bán, G mình
    /// giữ, V vé của mình).</summary>
    public static List<string> SoDoChu(JsonObject tt, string ten)
    {
        var ghe = (tt["ghe"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        var soDo = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        var boTri = (tt["bo_tri"] as JsonArray ?? []).Select(x => (x as JsonArray ?? []).Select(v => (int)(Chung.SoLN(v) ?? 0)).ToArray()).ToList();
        var o = new Dictionary<string, string>();
        int rong = 0, soTang = 1, soCot = 1;
        for (int i = 0; i < Math.Min(ghe.Count, Math.Min(soDo.Count, boTri.Count)); i++)
        {
            string g = ghe[i], dong = soDo[i];
            var b = new System.Text.StringBuilder(g + ":");
            for (int k = 0; k < dong.Length; k++)
            {
                char x = dong[k];
                bool cuaToi = ChuO(tt, $"{g}#{k}") == ten;
                b.Append(cuaToi ? x switch { 'g' => 'G', 'b' => 'V', _ => x } : x);
            }
            o[g] = b.ToString();
            rong = Math.Max(rong, b.Length);
            soTang = Math.Max(soTang, boTri[i][0] + 1);
            soCot = Math.Max(soCot, boTri[i][2] + 1);
        }
        var chang = Enumerable.Range(0, XeKhach.SoChang).Select(k => $"{XeKhach.MaTram[k]}-{XeKhach.MaTram[k + 1]}");
        var ds = new List<string> { $"  {Chung.Chuoi(tt, "ten_xe")} – chặng: {string.Join(" | ", chang)}" };
        for (int tang = 0; tang < soTang; tang++)
        {
            if (soTang > 1) ds.Add(tang == 0 ? "  Tầng dưới:" : "  Tầng trên:");
            var hang = new SortedDictionary<int, Dictionary<int, string>>();
            for (int i = 0; i < o.Count; i++)
            {
                if (boTri[i][0] != tang) continue;
                if (!hang.TryGetValue(boTri[i][1], out var h)) hang[boTri[i][1]] = h = [];
                h[boTri[i][2]] = o[ghe[i]];
            }
            foreach (var h in hang.Values)
            {
                var b = new System.Text.StringBuilder("   ");
                for (int c = 0; c < soCot; c++)
                {
                    string x = h.GetValueOrDefault(c, "");
                    b.Append(x).Append(' ', rong - x.Length).Append(c + 1 < soCot ? " " : "");
                }
                ds.Add(b.ToString().TrimEnd());
            }
        }
        return ds;
    }

    sealed record Gio(List<string> Ghe, int Tu, int Den);

    readonly string ten;
    JsonObject tt = [];                                  // cap_nhat mới nhất
    Gio? gioHang;                                        // chỗ vừa giữ được (để "tt"/"nha" không cần gõ lại)
    long phienGio;                                       // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
    HashSet<string> daBao = [];                          // ô trong giỏ đã báo hết hạn / bị lấy
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiDatVeXe(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(CapNhat, TinKhac, MatKetNoi);
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        JsonObject t;
        lock (khoa) t = tt = Kn.TrangThai!;
        var d = t["dem"] as JsonObject;
        Log($"Vào {Chung.Chuoi(t, "nha_xe")}: chuyến {Chung.Chuoi(t, "chuyen")}, {Chung.Chuoi(t, "ten_xe")?.ToLowerInvariant()}"
            + $" ({Chung.Chuoi(t, "che_do")})");
        var tram = (t["tram"] as JsonArray ?? []).Select(Chung.ChuoiPy);
        Log($"  tuyến {string.Join(" – ", tram)}; chỗ-chặng trống {Chung.SoL(d, "trong")}, giữ {Chung.SoL(d, "giu")},"
            + $" bán {Chung.SoL(d, "ban")}; giữ chỗ {Chung.G(Chung.SoD(t, "ttl_s"))} s");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    /// <summary>Ô trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat chụp
    /// SAU lần giữ của mình (tt["phien"] ≥ phien của kq).</summary>
    void CapNhat(JsonObject moi)
    {
        List<string> o;
        lock (khoa)
        {
            tt = moi;
            if (gioHang == null || Chung.SoL(moi, "phien") < phienGio) return;
            o = [.. XeKhach.OCua(gioHang.Ghe, gioHang.Tu, gioHang.Den).Where(x => !daBao.Contains(x))];
        }
        var bao = new List<(string G, string? Ai)>();
        foreach (var x in o)
        {
            string? ai = ChuO(moi, x);
            if (ai == ten) continue;
            string g = x.Split('#')[0];
            if (!bao.Any(b => b.G == g)) bao.Add((g, ai));
            lock (khoa) daBao.Add(x);
        }
        foreach (var (g, ai) in bao)
            Log(ai == null ? $"  Hết thời gian giữ chỗ: {g} đã được nhả" : $"  Chỗ {g} đã bị {ai} lấy mất!");
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    void MatKetNoi() => Log("Mất kết nối máy chủ");

    static List<string> Ds(JsonObject tin, string k) => [.. (tin[k] as JsonArray ?? []).Select(Chung.ChuoiPy)];

    // ------------------------------------------------------------ gửi
    static JsonObject Truong(Gio g) => new() { ["ghe"] = XeKhach.Json(g.Ghe), ["tu"] = g.Tu, ["den"] = g.Den };

    void Giu(Gio g)
    {
        Log($"> giữ {string.Join(", ", g.Ghe)} {XeKhach.TenDoan(g.Tu, g.Den)}");
        Kn.GuiYc("giu_cho", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                lock (khoa)
                {
                    gioHang = new Gio(Ds(tin, "ghe"), g.Tu, g.Den);
                    (phienGio, daBao) = (Chung.SoL(tin, "phien"), []);
                }
                Log($"  ĐÃ GIỮ ({Chung.F(rtt, "F1")} ms), {Chung.Tien(Chung.SoL(tin, "tien"))},"
                    + $" thanh toán trong {Chung.F(Chung.SoD(tin, "het_han_ms") / 1000, "F0")} s");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, Truong(g));
    }

    void ThanhToan(Gio g)
    {
        Log($"> thanh toán {string.Join(", ", g.Ghe)} {XeKhach.TenDoan(g.Tu, g.Den)}");
        Kn.GuiYc("thanh_toan", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                lock (khoa) gioHang = null;
                Log($"  ĐẶT VÉ THÀNH CÔNG ({Chung.F(rtt, "F1")} ms): {Chung.Tien(Chung.SoL(tin, "tien"))}");
                Log($"  mã vé {string.Join(", ", Ds(tin, "ma_ve"))}");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, Truong(g));
    }

    void Nha(Gio g)
    {
        Log($"> nhả {string.Join(", ", g.Ghe)} {XeKhach.TenDoan(g.Tu, g.Den)}");
        Kn.GuiYc("nha", (tin, _) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                lock (khoa) gioHang = null;
                Log($"  Đã nhả {string.Join(", ", Ds(tin, "ghe"))}");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, Truong(g));
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            Gio? gio;
            lock (khoa) gio = gioHang;
            int tu = p.Length >= 4 ? Array.IndexOf(XeKhach.MaTram, p[1].ToUpperInvariant()) : -1;
            int den = p.Length >= 4 ? Array.IndexOf(XeKhach.MaTram, p[2].ToUpperInvariant()) : -1;
            if (p[0] == "giu" && tu >= 0 && den >= 0) Giu(new Gio([.. p[3..].Select(g => g.ToUpperInvariant())], tu, den));
            else if (p[0] is "tt" or "nha")
            {
                if (gio == null) Log("chưa giữ chỗ nào");
                else if (p[0] == "tt") ThanhToan(gio);
                else Nha(gio);
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
            else Log($"lệnh: giu SG DL D01 [D02 …] | tt | nha | xem | thoat   (trạm: {string.Join(", ", XeKhach.MaTram)})");
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
            Console.WriteLine("Dùng: dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiDatVeXe(a.ViTri[0]);
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
