namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Máy chủ quay số trúng thưởng (bản dòng lệnh).
//
// Lõi ở MayChu.cs (MayChuQuaySo, khung MayChuCoSo trong Chung.cs, dùng chung với bản giao diện GiaoDien/); tệp
// này chỉ in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới) và nhận lệnh bàn phím.
//
// Lệnh (stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chạy):
//   che_do khong_khoa|khoa|lac_quan|hang_doi     tre <ms>     dat_lai [khoa=giá_trị …]     thong_ke   thoat
//   (tham số đặt lại: so_qua (1–10), p (xác suất trúng mỗi lượt, ví dụ 0.0005 = 1/2000))
//
// Chạy:  dotnet run -- server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]
// Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).
static class Server
{
    static void Log(string text) => InDong.In("server", text);

    public static int Run(string[] args)
    {
        var a = new DoiSo(args);
        string host = a.VT(0, "127.0.0.1");
        int cong = int.Parse(a.VT(1, MoPhong.CongMacDinh.ToString(Chung.Ic)), Chung.Ic);
        string cheDo = a.Chuoi("--che-do", "khoa");
        var sv = new MayChuQuaySo(cheDo, a.Double("--tre-ms", 2.0), Log, new OrderedDictionary<string, object>
        {
            ["so_qua"] = a.Long("--so-qua", 10), ["p"] = a.Double("--p", 0.0005),
        });
        sv.BatDau(host, cong);
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
