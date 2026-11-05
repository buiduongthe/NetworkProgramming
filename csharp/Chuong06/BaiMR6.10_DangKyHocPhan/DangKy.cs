using System.Text.Json.Nodes;

namespace BaiMR6_10;

// Bài mở rộng MR6.10 – Sinh viên đăng ký học phần (bản dòng lệnh).
//
// Kết nối máy chủ, in bảng lớp (mã lớp, tên, đã đăng ký/sĩ số) và nhận lệnh:
//   dk <mã lớp>       đăng ký lớp            dk <mã lớp> x2    "bấm đúp": gửi HAI yêu cầu liên tiếp không chờ
//   huy <mã lớp>      hủy lớp đã đăng ký     xem               in lại bảng lớp            thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- dang_ky <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class SinhVien
{
    readonly string ten;
    JsonObject tt = [];
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public SinhVien(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(moi => { lock (khoa) tt = moi; }, TinKhac, () => Log("Mất kết nối máy chủ"));
    }

    void Log(string text) => InDong.In(ten, text);

    JsonObject Tt
    {
        get { lock (khoa) return tt; }
    }

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        Log($"Vào cổng đăng ký ({Chung.Chuoi(tt, "che_do")}), sĩ số {Chung.SoL(tt, "si_so")}");
        Xem();
        Kn.BatDauDoc();
    }

    void Xem()
    {
        foreach (var x in (Tt["lop"] as JsonArray ?? []).OfType<JsonObject>())
        {
            bool cuaToi = (x["sv"] as JsonArray ?? []).Any(s => Chung.ChuoiPy(s) == ten);
            Log($"  {Chung.Chuoi(x, "ma")} {Chung.Chuoi(x, "ten"),-15} {Chung.SoL(x, "da_dk"),3}/{Chung.SoL(x, "si_so")}{(cuaToi ? " *" : "")}");
        }
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    // ------------------------------------------------------------ gửi
    void Gui(string loai, string ma)
    {
        Kn.GuiYc(loai, (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                string viec = loai == "dang_ky" ? "ĐĂNG KÝ" : "HỦY";
                Log($"  {viec} {ma} thành công ({Chung.SoL(tin, "da_dk")}/{Chung.SoL(Tt, "si_so")}, {Chung.F(rtt, "F1")} ms)");
            }
            else Log($"  {ma}: TỪ CHỐI – {Chung.ChuoiPy(tin["ly_do"])}");
        }, new JsonObject { ["ma_lop"] = ma });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p[0] == "dk" && p.Length == 2)
            {
                Log($"> đăng ký {p[1]}");
                Gui("dang_ky", p[1]);
            }
            else if (p[0] == "dk" && p.Length == 3 && p[2] == "x2")
            {
                Log($"> đăng ký {p[1]} (bấm đúp: 2 yêu cầu)");
                Gui("dang_ky", p[1]);
                Gui("dang_ky", p[1]);
            }
            else if (p[0] == "huy" && p.Length == 2)
            {
                Log($"> hủy {p[1]}");
                Gui("huy", p[1]);
            }
            else if (p[0] == "xem") Xem();
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: dk <mã> [x2] | huy <mã> | xem | thoat");
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
            Console.WriteLine("Dùng: dang_ky <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var sv = new SinhVien(a.ViTri[0]);
        sv.KetNoi(a.VT(1, "127.0.0.1"), int.Parse(a.VT(2, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic));
        var hetStdin = new ManualResetEventSlim();
        var dung = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true;
            dung.Set();
        };
        new Thread(() => Chung.DocLenhStdin(t0, sv.Lenh, hetStdin.Set)) { IsBackground = true }.Start();
        if (thoiGian > 0)                                // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
        {
            while (sv.Kn.Mo && Chung.Now - t0 < thoiGian && !dung.IsSet) Thread.Sleep(100);
        }
        else                                             // tới khi hết stdin / thoat
        {
            while (sv.Kn.Mo && !hetStdin.Wait(100) && !dung.IsSet) { }
            Thread.Sleep(500);                           // chờ trả lời của lệnh cuối
        }
        sv.Kn.KhiDong = () => { };
        sv.Kn.Dong();
        return 0;
    }
}
