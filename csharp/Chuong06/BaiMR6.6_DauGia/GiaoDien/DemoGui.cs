using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_6;

// Bài mở rộng MR6.6 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba phòng đấu giá An, Bình, Chi. Kịch bản bấm nút
// như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–3. chế độ khoa: Bình và Chi cùng bấm "+1 bước" -> một người bị từ chối "giá đã thay đổi";
//   4.   chế độ khong_khoa, trễ 0–20 ms: ba người cùng bấm "+1 bước" -> cả ba cùng "được chấp nhận" -> SAI PHẠM;
//        chụp ket_qua_gui_tranh_chap.png;
//   5.   So sánh 4 chế độ (mô phỏng 500 người × 5 lần, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   6–7. phiên 8 s, gia hạn 4 s: trả giá phút cuối -> gia hạn hai lần -> kết thúc, công bố người thắng;
//        chụp 4 cửa sổ ghép thành ket_qua_gui.png.
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

    static void InCuaSo(IEnumerable<FormDauGia> ds)
    {
        foreach (var cs in ds) Log($"   {cs.Ten}: {cs.LblThongBao.Text}");
    }

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
    }

    public static async Task RunAsync(FormServerDauGia sv, List<FormDauGia> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa, phiên 120 s)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        await Ngu(1.0);
        Log("2. An bấm '+1 bước'");
        an.NutBuoc[0].Nut.PerformClick();
        await Ngu(0.8);
        Log("3. Bình và Chi CÙNG bấm '+1 bước' (cùng giá)");
        binh.NutBuoc[0].Nut.PerformClick();
        chi.NutBuoc[0].Nut.PerformClick();
        await Ngu(0.8);
        InCuaSo(ds);
        InServer(sv);
        Log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        sv.TxtTre.Text = "20";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa"));
        await Ngu(0.5);
        Log("   An, Bình, Chi CÙNG bấm '+1 bước'");
        foreach (var cs in ds) cs.NutBuoc[0].Nut.PerformClick();
        await Ngu(0.8);
        InCuaSo(ds);
        InServer(sv);
        Log("   Lịch sử trả giá (mới nhất trước):");
        foreach (var x in (sv.Lop.TrangThai()["lich_su"] as JsonArray ?? []).OfType<JsonObject>().Take(4))
            Log($"     {Chung.Chuoi(x, "gio")} {Chung.Chuoi(x, "nguoi"),-5} {Chung.Tien(Chung.SoL(x, "gia"))}");
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(binh)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("5. Máy chủ: khoa, trễ 0–2 ms; So sánh 4 chế độ");
        sv.TxtTre.Text = "2";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khoa");
        sv.TxtN.Text = "500";
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 120);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("6. Đặt lại: phiên 8 s, gia hạn 4 s; An '+1 bước'");
        sv.DatThamSo(("thoi_gian_s", 8), ("gia_han_s", 4));
        sv.BtnDatLai.PerformClick();
        await Ngu(1.0);
        an.NutBuoc[0].Nut.PerformClick();
        await Ngu(4.2);
        Log($"   Bình '+2 bước' khi còn {an.LblDongHo.Text}");
        binh.NutBuoc[1].Nut.PerformClick();
        await Ngu(0.5);
        Log($"   -> đồng hồ An: {an.LblDongHo.Text} {an.LblGiaHan.Text}");
        await Cho(() => an.HetHan is { } h && h - Chung.Now < 0.8);
        Log($"7. An '+1 bước' khi còn {an.LblDongHo.Text}");
        an.NutBuoc[0].Nut.PerformClick();
        await Ngu(0.5);
        Log($"   -> đồng hồ Bình: {binh.LblDongHo.Text} {binh.LblGiaHan.Text}");
        await Cho(() => Chung.Dung(an.Tt, "ket_thuc"));
        await Ngu(0.8);
        InCuaSo(ds);
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
