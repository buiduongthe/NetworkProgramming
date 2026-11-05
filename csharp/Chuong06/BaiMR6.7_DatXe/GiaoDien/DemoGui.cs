using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_7;

// Bài mở rộng MR6.7 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba ứng dụng gọi xe An, Bình, Chi. Kịch bản bấm nút
// như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–3. chế độ khoa, 1 xe: ba người CÙNG bấm "ĐẶT XE" -> một người được, hai người "Hết chỗ"; người được hủy;
//   4.   chế độ khong_khoa, trễ 0–20 ms: ba người cùng bấm -> cả ba "Đặt thành công" cùng một tài xế, sau cap_nhat
//        hai người thấy "Tài xế … đã nhận khách khác!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   5.   khoa, 3 xe: So sánh 4 chế độ (mô phỏng 1000 người) -> bảng, dòng khong_khoa tô đỏ;
//   6–7. đặt lại 3 xe, ba người cùng đặt -> mỗi người một xe, xe chạy hoạt hình; chụp 4 cửa sổ -> ket_qua_gui.png.
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

    static void InCuaSo(IEnumerable<FormGoiXe> ds)
    {
        foreach (var cs in ds) Log($"   {cs.Ten}: {cs.LblTt.Text}");
    }

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
    }

    static void CungBam(IEnumerable<FormGoiXe> ds)
    {
        foreach (var cs in ds) cs.BtnDat.PerformClick();          // ba lần bấm liền nhau trên luồng giao diện
    }

    public static async Task RunAsync(FormServerDatXe sv, List<FormGoiXe> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa, 1 xe)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        await Ngu(1.0);
        Log("2. An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        CungBam(ds);
        await Cho(() => ds.All(cs => cs.Trang != "dang_tim"));
        await Ngu(0.5);
        InCuaSo(ds);
        InServer(sv);
        var duoc = ds.First(cs => cs.Trang == "da_dat");
        await Ngu(1.0);
        Log($"3. {duoc.Ten} bấm 'Hủy chuyến'");
        duoc.BtnHuy.PerformClick();
        await Cho(() => duoc.Trang == "xong");
        await Ngu(0.5);
        Log($"   {duoc.Ten}: {duoc.LblTt.Text}");
        Log($"   máy chủ: {Chung.Chuoi(sv.Lop.ThongKeDayDu(), "tom_tat")}");
        Log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        sv.TxtTre.Text = "20";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa"));
        await Ngu(0.5);
        Log("   An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        CungBam(ds);
        await Cho(() => ds.All(cs => cs.Trang != "dang_tim"));
        foreach (var cs in ds) Log($"   {cs.Ten} nhận: {cs.LblTt.Text}");
        await Ngu(0.6);
        Log("   …sau cap_nhat của máy chủ:");
        InCuaSo(ds);
        InServer(sv);
        var xe1 = (sv.Lop.TrangThai()["xe"] as JsonArray)?[0] as JsonObject;
        Log($"   xe 1 chở: {Chung.Chuoi(xe1, "khach") ?? "None"}");
        var nanNhan = ds.FirstOrDefault(cs => cs.Trang == "mat_xe") ?? binh;
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(nanNhan)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("5. Máy chủ: khoa, trễ 0–2 ms, đặt lại 3 xe");
        Log("   So sánh 4 chế độ (1000 người)");
        sv.TxtTre.Text = "2";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khoa");
        sv.DatSoXe(3);
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        sv.BtnDatLai.PerformClick();
        sv.TxtN.Text = "1000";
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 120);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("6. Đặt lại (3 xe); An, Bình, Chi CÙNG bấm 'ĐẶT XE'");
        sv.BtnDatLai.PerformClick();
        await Cho(() => ds.All(cs => Chung.SoL(cs.Tt, "trong") == 3));
        await Ngu(0.8);
        CungBam(ds);
        await Cho(() => ds.All(cs => cs.Trang != "dang_tim"));
        await Ngu(1.6);
        InCuaSo(ds);
        foreach (var cs in ds)
            if (cs.Chuyen != null)
                Log($"   {cs.Ten}: {cs.LblTx.Text.Split(' ')[0]} {Chung.Chuoi(cs.Chuyen, "bien_so")}, đến sau {Chung.SoL(cs.Chuyen, "phut")} phút");
        Log($"   máy chủ: {Chung.Chuoi(sv.Lop.ThongKeDayDu(), "tom_tat")}");
        Log("7. Chụp 4 cửa sổ");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        await Cho(() => ds.All(cs => cs.HoatHinh == null || cs.DaToi()));
        await Ngu(0.3);
        InCuaSo(ds);
        Log("8. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   Bình: {binh.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
