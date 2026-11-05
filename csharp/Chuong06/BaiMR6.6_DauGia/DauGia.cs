using System.Text.Json.Nodes;

namespace BaiMR6_6;

// Bài mở rộng MR6.6 – Người tham gia đấu giá (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến phiên (giá mới, bị trả giá cao hơn, gia hạn, người thắng) và nhận lệnh:
//   tra <giá>       trả đúng giá này                tang [n]    trả giá hiện tại + n bước giá (mặc định 1)
//   thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- dau_gia <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiThamGia
{
    readonly string ten;
    JsonObject tt = [];                                  // cap_nhat mới nhất
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiThamGia(string ten)
    {
        this.ten = ten;
        Kn = new KetNoiKhach(CapNhat, TinKhac, MatKetNoi);
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        var mon = (JsonObject)tt["mon"]!;
        string con = Chung.LaNull(tt["con_lai_ms"]) ? "không giới hạn" : $"còn {Chung.F(Chung.SoD(tt, "con_lai_ms") / 1000, "F1")} s";
        Log($"Vào phòng: {Chung.Chuoi(mon, "ten")} ({Chung.Chuoi(tt, "che_do")})");
        Log($"  giá {Chung.Tien(Chung.SoL(tt, "gia"))}, bước {Chung.Tien(Chung.SoL(mon, "buoc_gia"))}, {con}");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        JsonObject cu;
        lock (khoa) (cu, tt) = (tt, moi);
        string? ai = Chung.Chuoi(moi, "nguoi_dan_dau"), aiCu = Chung.Chuoi(cu, "nguoi_dan_dau");
        if (Chung.SoL(moi, "gia") != Chung.SoL(cu, "gia") || ai != aiCu)
        {
            Log($"Giá mới {Chung.Tien(Chung.SoL(moi, "gia"))} – {ai ?? "None"} dẫn đầu");
            if (aiCu == ten && ai != ten) Log("  Bạn bị trả giá cao hơn!");
        }
        if (Chung.Dung(moi, "ket_thuc") && !Chung.Dung(cu, "ket_thuc"))
            Log(ai == ten ? "  BẠN THẮNG!" : "  Bạn không thắng phiên này");
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    void MatKetNoi() => Log("Mất kết nối máy chủ");

    // ------------------------------------------------------------ gửi
    void Tra(long gia, string ghiChu = "")
    {
        Log($"> trả {Chung.Tien(gia)}{ghiChu}");
        Kn.GuiYc("tra_gia", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                string them = Chung.Dung(tin, "gia_han") ? ", phiên được GIA HẠN" : "";
                Log($"  CHẤP NHẬN ({Chung.F(rtt, "F1")} ms){them}");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, new JsonObject { ["gia"] = gia });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p[0] == "tra" && p.Length == 2) Tra(long.Parse(p[1], Chung.Ic));
            else if (p[0] == "tang" && p.Length <= 2)
            {
                long n = p.Length == 2 ? long.Parse(p[1], Chung.Ic) : 1;
                long gia;
                lock (khoa) gia = Chung.SoL(tt, "gia") + n * Chung.SoL(tt["mon"] as JsonObject, "buoc_gia");
                Tra(gia, $" (+{n} bước)");
            }
            else if (p[0] == "thoat")
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: tra <giá> | tang [n] | thoat");
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
            Console.WriteLine("Dùng: dau_gia <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiThamGia(a.ViTri[0]);
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
