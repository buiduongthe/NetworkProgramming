// Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ (bản dòng lệnh; bản giao diện WinForms: thư mục GiaoDien/).
//
// Bài mở rộng, không có trong giáo trình; minh họa Chương 5 (luồng, bất đồng bộ) và Chương 6 (máy chủ đa
// luồng, nhóm luồng): cùng ba tác vụ T1, T2, T3 (CPU → chờ I/O → CPU) và một việc khẩn U, chạy theo bốn chế độ
// đơn luồng/đa luồng × đồng bộ/bất đồng bộ trên bộ lập lịch mô phỏng (LapLich.cs). Đầu ra trùng từng ký tự
// với bản Python (mo_phong.py).
//
// Cách dùng:
//     dotnet run -- <chế_độ> [--t1 4] [--t2 3] [--t3 2] [--loi 2] [--lat 0.5] [--khan 0.3]
//                   [--khong-khan] [--khong-phu-thuoc] [--khong-chia]
//     chế_độ: don_dong_bo | da_dong_bo | don_bat_dong_bo | da_bat_dong_bo | so_sanh | tat_ca
using System.Globalization;
using System.Text;
using BaiMR5_6;
using L = BaiMR5_6.LapLich;

static class MoPhong
{
    static readonly Dictionary<string, string[]> NhanXet = new()
    {
        ["don_dong_bo"] = ["Nhận xét: một luồng, lời gọi chặn: khi chờ I/O luồng đứng",
                           "  yên (~), lõi CPU bỏ không; tác vụ làm lần lượt theo thứ tự",
                           "  đến nên việc khẩn U phải chờ tất cả."],
        ["da_dong_bo"] = ["Nhận xét: mỗi tác vụ một luồng: I/O của luồng này chồng lên",
                          "  CPU của luồng khác; luồng nhiều hơn lõi phải chờ lõi (-);",
                          "  hệ điều hành chia lát thời gian và cho U (ưu tiên cao) chen",
                          "  ngang ngay; T3 chờ join (.); nửa sau của T1 chạy song song",
                          "  trên luồng T1b."],
        ["don_bat_dong_bo"] = ["Nhận xét: một luồng vẫn chồng được các lần chờ I/O (hàng",
                               "  I/O chờ có nhiều việc cùng lúc) mà không cần thêm luồng;",
                               "  nhưng bước CPU chạy hết mới nhường (hợp tác): U phải xếp",
                               "  hàng, hai nửa của T1 chạy lần lượt – bất đồng bộ không",
                               "  phải là song song."],
        ["da_bat_dong_bo"] = ["Nhận xét: nhóm thợ + bất đồng bộ: I/O không giữ luồng, bước",
                              "  CPU giao cho thợ rảnh nên một tác vụ có thể chạy tiếp trên",
                              "  luồng khác sau khi chờ I/O; hai nửa của T1 chạy song song",
                              "  trên 2 thợ; số luồng chỉ bằng số lõi."],
    };

    static readonly TextWriter Out = Console.Out;

    static void P(string s = "") => Out.Write(s + "\n");      // luôn xuống dòng bằng \n như bản Python trên Linux

    static void InThamSo(ThamSo ts)
    {
        var t = ts.D.Select(L.Giay).ToArray();
        P($"Tác vụ: T1 {t[0]} s, T2 {t[1]} s, T3 {t[2]} s; mỗi tác vụ: CPU 1/4 →");
        P($"  chờ I/O 1/2 → CPU phần còn lại; {ts.SoLoi} lõi CPU; lát thời gian {L.Giay(ts.Lat)} s");
        var them = new List<string>();
        if (ts.PhuThuoc) them.Add("T3 chờ T1, T2");
        if (ts.ChiaCpu) them.Add("CPU cuối T1 chia 2 nửa");
        if (ts.Khan) them.Add($"việc khẩn U lúc {L.Giay(ts.TKhan)} s");
        P("  " + (them.Count > 0 ? string.Join("; ", them) : "không phụ thuộc, không chia, không việc khẩn"));
    }

    static KetQua ChayMot(string cheDo, ThamSo ts)
    {
        P($"=== {L.CheDo[cheDo]} ===");
        InThamSo(ts);
        var kq = L.Chay(cheDo, ts);
        P();
        P("Ký hiệu: 1 2 3 U tác vụ đang dùng CPU; ~ luồng bị chặn chờ I/O;");
        P("  - sẵn sàng nhưng chờ lõi; . chờ join; I/O chờ: số I/O bất đồng");
        P("  bộ đang chờ (không giữ luồng nào)");
        foreach (var d in L.Gantt(kq)) P(d);
        P();
        P("Nhật ký:");
        foreach (var (t, text) in kq.NhatKy)
            P($"  {L.Phai(L.Giay(t), 4)} s  {text}");
        P();
        P("  Tác vụ   Đến  Bắt đầu   Xong  Thời gian");
        foreach (var v in kq.Viec)
            P($"  {L.Trai(v.Ten, 6)} {L.Phai(L.Giay(v.Den), 5)} {L.Phai(L.Giay(v.BatDau), 8)} " +
              $"{L.Phai(L.Giay(v.Xong), 6)} {L.Phai(L.Giay(v.Xong - v.Den), 9)} s");
        var s = L.TinhChiSo(kq);
        P($"  Hoàn tất lúc {L.Giay(s.Tong)} s; số luồng {s.Luong}; lõi bận {s.Ban} %");
        string dong = $"  Chen ngang {s.Chen} lần; hết lát thời gian {s.Lat} lần";
        if (s.UCho >= 0) dong += $"; U chờ {L.Giay(s.UCho)} s";
        P(dong);
        P();
        foreach (var d in NhanXet[cheDo]) P(d);
        P();
        return kq;
    }

    static void SoSanh(ThamSo ts)
    {
        P("=== So sánh bốn chế độ ===");
        InThamSo(ts);
        P();
        P($"  {L.Trai("Chế độ", 14)} {L.Phai("Hoàn tất", 8)} {L.Phai("Luồng", 5)} {L.Phai("Lõi bận", 7)} " +
          $"{L.Phai("U chờ", 6)} {L.Phai("Chen", 4)} {L.Phai("Hết lát", 7)}");
        foreach (var m in L.DsCheDo)
        {
            var s = L.TinhChiSo(L.Chay(m, ts));
            string u = s.UCho >= 0 ? L.Giay(s.UCho) + " s" : "–";
            P($"  {L.Trai(L.TenNgan[m], 14)} {L.Phai(L.Giay(s.Tong) + " s", 8)} {L.Phai(s.Luong.ToString(), 5)} " +
              $"{L.Phai(s.Ban + " %", 7)} {L.Phai(u, 6)} {L.Phai(s.Chen.ToString(), 4)} {L.Phai(s.Lat.ToString(), 7)}");
        }
        P();
        P("Ghi nhớ: đa luồng hệ điều hành chen ngang được (ưu tiên, lát");
        P("  thời gian) nhưng tốn luồng; bất đồng bộ dùng ít luồng, chồng");
        P("  được I/O nhưng chỉ nhường ở điểm await; nhóm thợ + bất đồng");
        P("  bộ dùng hết các lõi mà số luồng vẫn bằng số lõi.");
    }

    static int Loi(string msg)
    {
        Console.Error.WriteLine("Cách dùng: dotnet run -- {don_dong_bo,da_dong_bo,don_bat_dong_bo,da_bat_dong_bo,so_sanh,tat_ca}\n" +
                                "    [--t1 T1] [--t2 T2] [--t3 T3] [--loi LOI] [--lat LAT] [--khan KHAN]\n" +
                                "    [--khong-khan] [--khong-phu-thuoc] [--khong-chia]");
        Console.Error.WriteLine("Lỗi: " + msg);
        return 2;
    }

    static int Main(string[] args)
    {
        Console.OutputEncoding = new UTF8Encoding(false);         // in tiếng Việt đúng trên cmd, không BOM
        var inv = CultureInfo.InvariantCulture;                   // số thập phân dùng dấu chấm
        double t1 = 4.0, t2 = 3.0, t3 = 2.0, lat = 0.5, khan = 0.3;
        int loi = 2;
        bool khongKhan = false, khongPhuThuoc = false, khongChia = false;
        string? cheDo = null;
        for (int i = 0; i < args.Length; i++)
        {
            string a = args[i], ten = a, giaTri = "";
            bool coGiaTri = false;
            if (a.StartsWith("--") && a.Contains('='))                 // --t1=4 cũng được, như argparse
            {
                ten = a[..a.IndexOf('=')]; giaTri = a[(a.IndexOf('=') + 1)..]; coGiaTri = true;
            }
            switch (ten)
            {
                case "--khong-khan": khongKhan = true; continue;
                case "--khong-phu-thuoc": khongPhuThuoc = true; continue;
                case "--khong-chia": khongChia = true; continue;
                case "--t1" or "--t2" or "--t3" or "--lat" or "--khan" or "--loi":
                    if (!coGiaTri)
                    {
                        if (i + 1 >= args.Length) return Loi($"thiếu giá trị cho {ten}");
                        giaTri = args[++i];
                    }
                    if (ten == "--loi")
                    {
                        if (!int.TryParse(giaTri, NumberStyles.Integer, inv, out loi))
                            return Loi($"{ten}: không phải số nguyên: '{giaTri}'");
                        continue;
                    }
                    if (!double.TryParse(giaTri, NumberStyles.Float, inv, out double x))
                        return Loi($"{ten}: không phải số: '{giaTri}'");
                    switch (ten)
                    {
                        case "--t1": t1 = x; break;
                        case "--t2": t2 = x; break;
                        case "--t3": t3 = x; break;
                        case "--lat": lat = x; break;
                        default: khan = x; break;
                    }
                    continue;
            }
            if (a.StartsWith("--") || cheDo != null) return Loi($"tham số không hợp lệ: {a}");
            cheDo = a;
        }
        if (cheDo == null) return Loi("thiếu chế độ");
        if (!L.CheDo.ContainsKey(cheDo) && cheDo != "so_sanh" && cheDo != "tat_ca")
            return Loi($"chế độ không hợp lệ: '{cheDo}'");
        var ts = new ThamSo(t1, t2, t3, loi, lat, !khongPhuThuoc, !khongChia, !khongKhan, khan);
        if (cheDo == "so_sanh")
            SoSanh(ts);
        else if (cheDo == "tat_ca")
        {
            foreach (var m in L.DsCheDo) ChayMot(m, ts);
            SoSanh(ts);
        }
        else
            ChayMot(cheDo, ts);
        Out.Flush();
        return 0;
    }
}
