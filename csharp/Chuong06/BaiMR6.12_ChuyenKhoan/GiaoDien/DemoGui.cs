using System.Text.Json.Nodes;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba ứng dụng ngân hàng An (TK01), Bình (TK02), Chi
// (TK03). Kịch bản bấm nút như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in
// `[demo] …` – cùng các bước với demo_gui.py của bản Python:
//   1–2. khoa (khóa theo thứ tự mã): An→Bình và Bình→An CÙNG LÚC -> cả hai thành công;
//   3.   bật "khóa ngây thơ", trễ 0–50 ms: lặp lại -> KHÓA CHẾT, chờ khóa 2 s rồi hủy giao dịch; chụp
//        ket_qua_gui_khoa_chet.png;
//   4.   tắt "khóa ngây thơ": lặp lại -> hết khóa chết;
//   5.   khong_khoa, trễ 0–20 ms: An→Bình, Bình→Chi, Chi→An cùng lúc -> mất cập nhật, TỔNG TIỀN LỆCH; chụp
//        ket_qua_gui_tranh_chap.png;
//   6–8. khoa: So sánh 4 chế độ -> bảng; đặt lại, An chuyển cho Chi; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
static class DemoGui
{
    static void Log(string s) => Console.WriteLine($"[demo] {s}");

    /// <summary>Chờ tới khi điều kiện đúng (tối đa hetGio giây), vòng lặp thông điệp vẫn chạy trong lúc chờ.</summary>
    static async Task Cho(Func<bool> dk, double hetGio = 90.0)
    {
        for (double con = hetGio; !dk(); con -= 0.05)
        {
            if (con <= 0) throw new TimeoutException("Kịch bản chờ quá lâu – dừng");
            await Task.Delay(50);
        }
    }

    static Task Ngu(double giay) => Task.Delay(TimeSpan.FromSeconds(giay));

    static void InCuaSo(IEnumerable<FormNganHang> ds)
    {
        foreach (var cs in ds) Log($"   {cs.Ten}: " + cs.ThongBao.Split('\n')[0]);
    }

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        Log($"   {Chung.Chuoi(tk, "tom_tat")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
    }

    static void Chuyen(FormNganHang cs, string den, long soTien)
    {
        cs.ChonNguoiNhan(den);
        cs.DatTien(soTien);
        cs.BtnChuyen.PerformClick();
    }

    static void Tre(FormServerCoSo sv, int ms)
    {
        sv.TxtTre.Text = $"{ms}";
        sv.BtnTre.PerformClick();
    }

    /// <summary>An→Bình 500.000 đ và Bình→An 300.000 đ trong cùng một nhịp giao diện (cùng lúc).</summary>
    static Func<bool> Cheo(FormNganHang an, FormNganHang binh)
    {
        int dem = an.SoKq + binh.SoKq;
        Chuyen(an, "TK02", 500_000);
        Chuyen(binh, "TK01", 300_000);
        return () => an.SoKq + binh.SoKq == dem + 2;
    }

    public static async Task RunAsync(FormServerNganHang sv, List<FormNganHang> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (khoa, khóa theo thứ tự mã)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối (TK01, TK02, TK03)");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        await Ngu(1.0);
        Log("2. An→Bình 500.000 đ và Bình→An 300.000 đ CÙNG LÚC");
        await Cho(Cheo(an, binh));
        await Ngu(0.4);
        InCuaSo(ds[..2]);
        Log("3. Bật 'khóa ngây thơ', Đặt lại; trễ 0–50 ms");
        sv.DatKhoaNgayTho(true);
        sv.BtnDatLai.PerformClick();
        Tre(sv, 50);
        await Cho(() => Chung.Dung(an.Tt, "khoa_ngay_tho") && Chung.SoL(an.Tt, "so_gd") == 0);
        await Ngu(0.5);
        Log("   An→Bình và Bình→An cùng lúc…");
        await Cho(Cheo(an, binh));
        await Ngu(0.4);
        InCuaSo(ds[..2]);
        Log($"   máy chủ: khóa chết phát hiện {Chung.SoL(sv.Lop.TrangThai(), "khoa_chet")}");
        string ra = Path.Combine(thuMuc, "ket_qua_gui_khoa_chet.png");
        GD.Ghep(ra, [[GD.Chup(sv)], [GD.Chup(an), GD.Chup(binh)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("4. Tắt 'khóa ngây thơ', Đặt lại; lặp lại chuyển chéo");
        sv.DatKhoaNgayTho(false);
        sv.BtnDatLai.PerformClick();
        await Cho(() => !Chung.Dung(an.Tt, "khoa_ngay_tho") && Chung.SoL(an.Tt, "so_gd") == 0);
        await Ngu(0.5);
        await Cho(Cheo(an, binh));
        await Ngu(0.4);
        InCuaSo(ds[..2]);
        Log("5. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại");
        Tre(sv, 20);
        sv.ChonCheDo("khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa"));
        sv.BtnDatLai.PerformClick();
        await Cho(() => Chung.SoL(an.Tt, "so_gd") == 0);
        await Ngu(0.5);
        Log("   An→Bình, Bình→Chi, Chi→An 1.000.000 đ cùng lúc");
        int dem = ds.Sum(cs => cs.SoKq);
        string[] den = ["TK02", "TK03", "TK01"];
        for (int i = 0; i < ds.Count; i++) Chuyen(ds[i], den[i], 1_000_000);
        await Cho(() => ds.Sum(cs => cs.SoKq) == dem + 3);
        await Ngu(0.6);
        InCuaSo(ds);
        InServer(sv);
        ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("6. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
        Tre(sv, 2);
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("7. Đặt lại; An chuyển 200.000 đ cho Chi");
        sv.BtnDatLai.PerformClick();
        await Cho(() => Chung.SoL(an.Tt, "so_gd") == 0);
        await Ngu(0.5);
        int demAn = an.SoKq;
        Chuyen(an, "TK03", 200_000);
        await Cho(() => an.SoKq == demAn + 1);
        await Ngu(0.8);
        InCuaSo([an]);
        Log($"   Chi: {chi.LblDen.Text}");
        Log($"   máy chủ: {Chung.Chuoi(sv.Lop.ThongKeDayDu(), "tom_tat")}");
        Log("8. Chụp 4 cửa sổ");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        Log("9. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   Bình: {binh.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
