using System.Text.Json.Nodes;

namespace BaiMR6_7;

// Bài mở rộng MR6.7 – Người gọi xe (bản dòng lệnh).
//
// Kết nối máy chủ, in diễn biến (số xe trống, xe của mình bị "nhận khách khác") và nhận lệnh:
//   dat            đặt xe từ điểm đón mặc định của mình (theo tên) tới điểm đến mặc định
//   huy            hủy chuyến đã đặt                       thoat
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
// Chạy:  dotnet run -- dat_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)
sealed class NguoiGoiXe
{
    /// <summary>Điểm đón và điểm đến cố định theo tên (để mỗi người một chỗ đứng khác nhau).</summary>
    public static ((double X, double Y) Don, (double X, double Y) Den) DiemMacDinh(string ten)
    {
        long s = ten.EnumerateRunes().Sum(r => (long)r.Value);
        var nt = BanDoPho.NgaTu;
        var don = nt[s * 7 % nt.Length];
        var den = nt[(s * 11 + 9) % nt.Length];
        if (BanDoPho.Km(don, den) < 5) den = (don.X < 0.5 ? 0.9 : 0.1, don.Y < 0.5 ? 0.9 : 0.1);   // quá gần: chọn góc xa nhất
        return (don, den);
    }

    readonly string ten;
    readonly (double X, double Y) don, den;
    JsonObject tt = [];                                  // cap_nhat mới nhất
    // xeToi, phienToi chỉ được đọc/ghi trên luồng đọc của kết nối (gọi lại kq và cap_nhat) – không cần khóa
    long? xeToi;                                         // mã xe mình được báo đặt thành công (null: chưa có)
    long phienToi;                                       // số thứ tự lần ghi (kq) của chuyến mình đặt
    volatile bool dangHuy;
    readonly object khoa = new();
    public readonly KetNoiKhach Kn;

    public NguoiGoiXe(string ten)
    {
        this.ten = ten;
        (don, den) = DiemMacDinh(ten);
        Kn = new KetNoiKhach(CapNhat, TinKhac, MatKetNoi);
    }

    void Log(string text) => InDong.In(ten, text);

    public void KetNoi(string host, int cong)
    {
        Kn.KetNoi(host, cong, ten);
        tt = Kn.TrangThai!;
        Log($"Mở ứng dụng ({Chung.Chuoi(tt, "che_do")}): {Chung.SoL(tt, "trong")}/{Chung.SoL(tt, "so_xe")} xe trống");
        Log($"  đón: {BanDoPho.TenDiem(don)}");
        Log($"  đến: {BanDoPho.TenDiem(den)}");
        Kn.BatDauDoc();
    }

    // ------------------------------------------------------------ nhận
    void CapNhat(JsonObject moi)
    {
        JsonObject cu;
        lock (khoa) (cu, tt) = (tt, moi);
        if (Chung.SoL(moi, "trong") != Chung.SoL(cu, "trong"))
            Log($"Xe trống: {Chung.SoL(moi, "trong")}/{Chung.SoL(moi, "so_xe")}");
        if (xeToi is { } ma && !dangHuy && Chung.SoL(moi, "phien") >= phienToi)   // bỏ qua ảnh chụp trước lần ghi của mình
        {
            var xe = (moi["xe"] as JsonArray ?? []).OfType<JsonObject>().FirstOrDefault(x => Chung.SoL(x, "ma") == ma);
            string? khach = Chung.Chuoi(xe, "khach");
            if (xe != null && khach != null && khach != ten)
            {
                Log($"  Tài xế {Chung.Chuoi(xe, "tai_xe")} đã nhận khách khác ({khach})!");
                xeToi = null;
            }
            else if (xe == null || khach == null)
            {
                Log("  Chuyến đã kết thúc (máy chủ đặt lại)");
                xeToi = null;
            }
        }
    }

    void TinKhac(JsonObject tin)
    {
        if (Chung.Chuoi(tin, "loai") == "su_kien") Log($"* {Chung.Chuoi(tin, "noi_dung")}");
    }

    void MatKetNoi() => Log("Mất kết nối máy chủ");

    // ------------------------------------------------------------ gửi
    void Dat()
    {
        Log($"> Đặt xe – đang tìm tài xế… (dự kiến {Chung.Tien(BanDoPho.Cuoc(don, den))})");
        Kn.GuiYc("dat_xe", (tin, rtt) =>
        {
            if (Chung.Dung(tin, "ok"))
            {
                (xeToi, phienToi) = (Chung.SoL(tin, "xe"), Chung.SoL(tin, "phien"));
                Log($"  ĐẶT THÀNH CÔNG – tài xế {Chung.Chuoi(tin, "tai_xe")} ({Chung.F(rtt, "F1")} ms)");
                Log($"  {Chung.Chuoi(tin, "bien_so")}, xe {Chung.Chuoi(tin, "mau")}, đến sau {Chung.SoL(tin, "phut")} phút,"
                    + $" cước {Chung.Tien(Chung.SoL(tin, "cuoc"))}");
            }
            else if (Chung.Chuoi(tin, "ma_loi") == "het_cho") Log($"  HẾT CHỖ: {Chung.Chuoi(tin, "ly_do")}");
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        }, new JsonObject { ["don"] = BanDoPho.Json(don), ["den"] = BanDoPho.Json(den) });
    }

    void Huy()
    {
        Log("> Hủy chuyến");
        dangHuy = true;
        Kn.GuiYc("huy", (tin, _) =>
        {
            dangHuy = false;
            if (Chung.Dung(tin, "ok"))
            {
                xeToi = null;
                Log($"  Đã hủy, xe {Chung.SoL(tin, "xe")} ({Chung.Chuoi(tin, "tai_xe")}) trống");
            }
            else Log($"  TỪ CHỐI: {Chung.Chuoi(tin, "ly_do")}");
        });
    }

    public void Lenh(string dong)
    {
        var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        try
        {
            if (p is ["dat"]) Dat();
            else if (p is ["huy"]) Huy();
            else if (p is ["thoat"])
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
            else Log("lệnh: dat | huy | thoat");
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
            Console.WriteLine("Dùng: dat_xe <tên> [host] [cổng] [--thoi-gian 0]");
            return 2;
        }
        double thoiGian = a.Double("--thoi-gian", 0.0);
        double t0 = Chung.Now;
        var ng = new NguoiGoiXe(a.ViTri[0]);
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
