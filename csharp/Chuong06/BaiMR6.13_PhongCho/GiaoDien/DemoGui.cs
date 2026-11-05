using System.Text.Json.Nodes;

namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba trang bán vé An, Bình, Chi. Kịch bản bấm nút như
// người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–3. khoa, phòng chờ bật (2000 vé, 50 người/s): Chi vào hàng đầu tiên, tới lượt ngay; 300 người ảo ùa vào; An,
//        Bình vào hàng sau họ -> thấy số thứ tự, số người phía trước, thời gian chờ; chụp ket_qua_gui_phong_cho.png;
//        An tới lượt, mua 3 vé;
//   4.   khong_khoa, trễ 0–20 ms: Chi đăng nhập lại tên "An" (tab thứ hai của An); hai tab cùng vào hàng, cùng tới lượt,
//        cùng bấm THANH TOÁN -> cả hai đơn được nhận (sai lượt); chụp ket_qua_gui_tranh_chap.png;
//   5.   khoa: So sánh 4 chế độ (1000 người) và So sánh bật/tắt phòng chờ -> hai bảng;
//   6–7. 2000 vé, 20 người/s, thùng 2: Bình, Chi vào hàng, Chi mua 1 vé; 400 người ùa vào, An vào hàng sau họ; chụp
//        4 cửa sổ ghép thành ket_qua_gui.png.
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

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        Log($"   {Chung.Chuoi(tk, "tom_tat")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
    }

    static void Tre(FormServerCoSo sv, int ms)
    {
        sv.TxtTre.Text = $"{ms}";
        sv.BtnTre.PerformClick();
    }

    /// <summary>Bấm "Bắn đồng loạt" với n người trên k kết nối.</summary>
    static void Ban(FormServerCoSo sv, int n, int k)
    {
        (sv.TxtN.Text, sv.TxtK.Text) = ($"{n}", $"{k}");
        sv.BtnBan.PerformClick();
    }

    static void DatLai(FormServerPhongCho sv, List<FormPhongCho> ds, params (string, object)[] ts)
    {
        sv.DatThamSo(ts);
        sv.BtnDatLai.PerformClick();
        foreach (var cs in ds) cs.VeSuKien();
    }

    static long Ts(FormServerCoSo sv, string k) => Chung.SoL(sv.Lop.TrangThai(), k);

    static long TongVe(FormPhongCho cs) => Chung.SoL(cs.Tt?["su_kien"] as JsonObject, "tong_ve");

    public static async Task RunAsync(FormServerPhongCho sv, List<FormPhongCho> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (khoa, phòng chờ bật)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        await Ngu(0.8);
        Log("2. Đặt lại: 2000 vé, cấp lượt 50 người/s, thùng 50");
        DatLai(sv, ds, ("tong_ve", 2000L), ("toc_do", 50.0), ("dung_luong", 50L), ("phong_cho", 1L));
        await Cho(() => TongVe(chi) == 2000);
        await Ngu(0.4);
        Log("   Chi: VÀO HÀNG CHỜ (người đầu tiên)");
        chi.BtnVao.PerformClick();
        await Cho(() => chi.Trang == chi.TrangMua);
        Log($"   Chi: {chi.LblBangLuot.Text}");
        Log("   Mô phỏng: 300 người ùa vào hàng (Bắn đồng loạt)");
        Ban(sv, 300, 100);
        await Cho(() => Ts(sv, "so_cap") >= 250);
        Log("   An, Bình: VÀO HÀNG CHỜ");
        an.BtnVao.PerformClick();
        binh.BtnVao.PerformClick();
        await Cho(() => an.Trang == an.TrangCho && binh.Trang == binh.TrangCho);
        await Ngu(0.5);
        Log("   Chi: chọn 2 vé, THANH TOÁN");
        chi.Mua(2);
        await Ngu(1.0);
        Log($"   Chi: {chi.LblKq.Text}");
        Log($"   An: {an.LblSo.Text}");
        Log($"   An: {an.LblTruoc.Text}");
        string ra = Path.Combine(thuMuc, "ket_qua_gui_phong_cho.png");
        GD.Ghep(ra, [[GD.Chup(sv)], [GD.Chup(an), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        await Cho(() => an.Trang == an.TrangMua);
        await Ngu(0.5);
        Log($"3. An tới lượt: đồng hồ {an.LblDongHo.Text}; chọn 3 vé, THANH TOÁN");
        an.Mua(3);
        await Cho(() => an.SoDon == 1);
        Log($"   An: {an.LblKq.Text}");
        await Cho(() => !sv.DangMoPhong);
        InServer(sv);
        Log("4. Máy chủ: khong_khoa, trễ 0–20 ms; Đặt lại (500 vé)");
        Tre(sv, 20);
        sv.ChonCheDo("khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa"));
        DatLai(sv, ds, ("tong_ve", 500L));
        await Cho(() => TongVe(an) == 500 && Chung.SoL(an.Tt, "da_cap_luot") == 0);
        Log("   Chi: Rời, đăng nhập lại tên 'An' (tab thứ hai của An)");
        chi.BtnRoi.PerformClick();
        chi.TxtTen.Text = "An";
        chi.BtnKetNoi.PerformClick();
        await Cho(() => chi.Kn != null && chi.Tt != null);
        await Ngu(0.5);
        Log("   Hai tab của An cùng VÀO HÀNG CHỜ");
        an.BtnVao.PerformClick();
        chi.BtnVao.PerformClick();
        await Cho(() => an.Trang == an.TrangMua && chi.Trang == chi.TrangMua);
        Log("   Hai tab cùng bấm THANH TOÁN 2 vé (một lượt)");
        int dem = an.SoKq + chi.SoKq;
        an.Mua(2);
        chi.Mua(2);
        await Cho(() => an.SoKq + chi.SoKq == dem + 2);
        await Ngu(0.5);
        Log($"   tab 1: {an.LblKq.Text}");
        Log($"   tab 2: {chi.LblKq.Text}");
        InServer(sv);
        ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("5. khoa, trễ 0–2 ms; Chi vào lại; So sánh 4 chế độ");
        chi.BtnRoi.PerformClick();
        chi.TxtTen.Text = "Chi";
        chi.BtnKetNoi.PerformClick();
        Tre(sv, 2);
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa" && chi.Kn != null);
        (sv.TxtN.Text, sv.TxtK.Text) = ("1000", "200");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        Log("   So sánh bật/tắt phòng chờ (khoa, 1000 người)");
        sv.BtnSoSanhPc.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in MoPhong.BangPhongCho(sv.HangPc)) Log(d);
        Log("6. Đặt lại: 2000 vé, 20 người/s, thùng 2");
        DatLai(sv, ds, ("tong_ve", 2000L), ("toc_do", 20.0), ("dung_luong", 2L), ("phong_cho", 1L));
        await Cho(() => TongVe(an) == 2000 && Chung.SoD(an.Tt, "toc_do") == 20);
        await Ngu(0.4);
        Log("   Bình, Chi vào hàng; Chi mua 1 vé");
        binh.BtnVao.PerformClick();
        chi.BtnVao.PerformClick();
        await Cho(() => binh.Trang == binh.TrangMua && chi.Trang == chi.TrangMua);
        chi.Mua(1);
        Log("   400 người ùa vào; An vào hàng sau họ");
        Ban(sv, 400, 100);
        await Cho(() => Ts(sv, "so_cap") >= 350);
        an.BtnVao.PerformClick();
        await Cho(() => an.Trang == an.TrangCho);
        await Ngu(2.0);
        Log($"   An: {an.LblTruoc.Text}");
        Log($"   Bình: đồng hồ {binh.LblDongHo.Text}");
        Log($"   Chi: {chi.LblKq.Text}");
        Log("7. Chụp 4 cửa sổ");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        Log("8. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   Bình: {binh.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
