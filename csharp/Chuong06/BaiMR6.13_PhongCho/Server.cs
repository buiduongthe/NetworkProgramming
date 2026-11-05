namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Máy chủ bán vé có phòng chờ ảo (bản dòng lệnh).
//
// Lõi ở MayChu.cs (MayChuPhongCho, khung MayChuCoSo trong Chung.cs, dùng chung với bản giao diện GiaoDien/); tệp
// này chỉ in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
//
// Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
//   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
//   toc_do <người/s>  (đổi tốc độ cấp lượt ngay, không đặt lại)
//   (tham số đặt lại: tong_ve, ve_toi_da, gia_ve, ten_su_kien, phong_cho (0/1), toc_do, dung_luong,
//    luot_s, gioi_han_xu_ly)
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
//                                 [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
// Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).
static class Server
{
    static void Log(string text) => InDong.In("server", text);

    public static int Run(string[] args)
    {
        var a = new DoiSo(args, "--tat-phong-cho");
        string host = a.VT(0, "127.0.0.1");
        int cong = int.Parse(a.VT(1, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic);
        string cheDo = a.Chuoi("--che-do", "khoa");
        bool tat = a.Co("--tat-phong-cho");
        long tongVe = a.Long("--tong-ve", 500);
        double tocDo = a.Double("--toc-do", 50.0);
        var sv = new MayChuPhongCho(cheDo, a.Double("--tre-ms", 2.0), Log, new OrderedDictionary<string, object>
        {
            ["tong_ve"] = tongVe, ["toc_do"] = tocDo, ["dung_luong"] = a.Long("--dung-luong", 50), ["luot_s"] = a.Double("--luot-s", 20.0),
            ["gioi_han_xu_ly"] = a.Long("--gioi-han", 64), ["phong_cho"] = tat ? 0L : 1L,
        });
        sv.BatDau(host, cong);
        Log($"Phòng chờ {(tat ? "TẮT" : "BẬT")}: {tongVe} vé, cấp lượt {Chung.G(tocDo)} người/s");
        var xong = new ManualResetEventSlim();
        Console.CancelKeyPress += (_, e) =>              // Ctrl+C (công cụ demo dừng server bằng SIGINT)
        {
            e.Cancel = true;
            xong.Set();
        };

        void Lenh(string dong)
        {
            var p = dong.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            Log($"> {dong}");
            try
            {
                if (p[0] == "che_do" && p.Length == 2) sv.DatCheDo(p[1]);
                else if (p[0] == "tre" && p.Length == 2) sv.DatTre(double.Parse(p[1], Chung.Ic));
                else if (p[0] == "toc_do" && p.Length == 2) sv.DatTocDo(double.Parse(p[1], Chung.Ic));
                else if (p[0] == "dat_lai") sv.DatLai(Chung.DocThamSo(p[1..]));
                else if (p[0] == "thong_ke")
                    foreach (var d in Chung.DongThongKe(sv.ThongKeDayDu())) Log(d);
                else if (p[0] == "thoat") xong.Set();
                else Log("lệnh không hợp lệ");
            }
            catch (Exception e) when (e is ArgumentException or FormatException)
            {
                Log($"lỗi: {e.Message}");
            }
        }

        double t0 = Chung.Now;
        new Thread(() => Chung.DocLenhStdin(t0, Lenh)) { IsBackground = true }.Start();
        long daIn = 0;
        while (!xong.Wait(TimeSpan.FromSeconds(5)))      // mỗi 5 s: một dòng số liệu nếu có yêu cầu mới
        {
            var tk = sv.ThongKeDayDu();
            if (Chung.SoL(tk, "so_yc") != daIn)
            {
                daIn = Chung.SoL(tk, "so_yc");
                Log($"{Chung.Chuoi(tk, "che_do")}: {daIn} yc, tc {Chung.SoL(tk, "thanh_cong")},"
                    + $" từ chối {Chung.SoL(tk, "tu_choi")}, sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
            }
        }
        Log("Tổng kết (từ lần đặt lại cuối):");
        foreach (var d in Chung.DongThongKe(sv.ThongKeDayDu())) Log("  " + d);
        sv.Dung();
        return 0;
    }
}
