using System.Drawing;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_11;

// Bài mở rộng MR6.11 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba trang flash sale An, Bình, Chi. Kịch bản bấm nút
// như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–2. chế độ khoa, kho 3 sản phẩm, mở bán sau 3 s: nút MUA NGAY khóa khi đếm ngược; mở bán thì An (2), Bình (1),
//        Chi (2) CÙNG bấm -> bán đúng 3, người đến sau "chỉ còn …"/"hết hàng";
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> cả ba đều "đặt hàng thành công" -> bán vượt kho (tồn âm),
//        SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   4.   khoa, trễ 0–1 ms, kho 100: So sánh 4 chế độ -> bảng, dòng khong_khoa tô đỏ;
//   5–7. kho 100, mở bán sau 4 s: chụp Chi lúc đếm ngược; mở bán: An mua 2, 50 người ảo cùng mua, Bình mua 2; chụp
//        4 cửa sổ ghép thành ket_qua_gui.png.
static class DemoGui
{
    static void Log(string s) => Console.WriteLine($"[demo] {s}");

    /// <summary>Chờ tới khi điều kiện đúng (tối đa hetGio giây), vòng lặp thông điệp vẫn chạy trong lúc chờ.</summary>
    static async Task Cho(Func<bool> dk, double hetGio = 120.0)
    {
        for (double con = hetGio; !dk(); con -= 0.05)
        {
            if (con <= 0) throw new TimeoutException("Kịch bản chờ quá lâu – dừng");
            await Task.Delay(50);
        }
    }

    static Task Ngu(double giay) => Task.Delay(TimeSpan.FromSeconds(giay));

    static void InCuaSo(IEnumerable<FormFlashSale> ds)
    {
        foreach (var cs in ds) Log($"   {cs.Ten}: {cs.LblThongBao.Text}");
    }

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
        Log($"   {Chung.Chuoi(tk, "tom_tat")}");
    }

    static void DatLai(FormServerFlashSale sv, params (string, object)[] ts)
    {
        sv.DatThamSo(ts);
        sv.BtnDatLai.PerformClick();
    }

    static void DatTre(FormServerCoSo sv, int ms)
    {
        sv.TxtTre.Text = $"{ms}";
        sv.BtnTre.PerformClick();
    }

    /// <summary>Hiện lại bảng so sánh 4 chế độ (sau "Bắn đồng loạt", dòng của chế độ đang chạy bị thay bằng lượt
    /// mới) – như sv._xong_mo_phong(sv.hang_so_sanh) của bản Python.</summary>
    static void HienLaiBang(FormServerCoSo sv)
    {
        sv.Bang.Items.Clear();
        foreach (var h in sv.HangSoSanh)
        {
            var item = new ListViewItem([h.CheDo, $"{h.ThanhCong}", $"{h.SaiPham}", $"{h.XungDot}", $"{h.YcS}", h.P99Ms is { } p ? Chung.F(p, "F1") : "–"]);
            (item.BackColor, item.ForeColor) = h.SaiPham > 0 ? (GD.Hex("#ffc8c8"), GD.DoDam) : (GD.Hex("#d8f5d8"), Color.Black);
            sv.Bang.Items.Add(item);
        }
        var sai = sv.HangSoSanh.Where(h => h.SaiPham > 0).Select(h => h.CheDo).ToList();
        var dung = sv.HangSoSanh.Where(h => h.SaiPham == 0).Select(h => h.CheDo).ToList();
        sv.LblKetLuan.Text = (sai.Count > 0 ? $"Sai phạm: {string.Join(", ", sai)}. " : "")
                             + (dung.Count > 0 ? $"Không sai phạm: {string.Join(", ", dung)}." : "");
    }

    public static async Task RunAsync(FormServerFlashSale sv, List<FormFlashSale> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        int[] soLuong = [2, 1, 2];
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        Log("2. Đặt lại: kho 3 sản phẩm, mở bán sau 3 s");
        DatLai(sv, ("ton_dau", 3L), ("mo_sau_s", 3.0));
        await Cho(() => ds.All(cs => Chung.SoL(cs.Tt, "ton_dau") == 3));
        await Ngu(0.5);
        Log($"   An thấy: {an.LblDongHo.Text}, nút '{an.BtnMua.Text}' ({(an.BtnMua.Enabled ? "normal" : "disabled")})");
        await Cho(() => ds.All(cs => cs.BtnMua.Enabled));
        Log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
        for (int i = 0; i < ds.Count; i++) ds[i].Mua(soLuong[i]);
        await Cho(() => ds.All(cs => !cs.DangCho));
        await Ngu(0.5);
        InCuaSo(ds);
        InServer(sv);
        Log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; đặt lại");
        DatTre(sv, 20);
        DatLai(sv, ("ton_dau", 3L), ("mo_sau_s", 2.0));
        sv.ChonCheDo("khong_khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa" && cs.BtnMua.Enabled));
        Log("   Mở bán: An (2), Bình (1), Chi (2) CÙNG bấm MUA NGAY");
        for (int i = 0; i < ds.Count; i++) ds[i].Mua(soLuong[i]);
        await Cho(() => ds.All(cs => !cs.DangCho));
        await Ngu(0.6);
        InCuaSo(ds);
        InServer(sv);
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("4. Máy chủ: khoa, trễ 0–1 ms, kho 100; So sánh 4 chế độ");
        DatTre(sv, 1);
        DatLai(sv, ("ton_dau", 100L), ("mo_sau_s", 30.0));
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("5. Đặt lại: kho 100, mở bán sau 4 s");
        DatLai(sv, ("ton_dau", 100L), ("mo_sau_s", 4.0));
        await Cho(() => ds.All(cs => Chung.SoL(cs.Tt, "ton") == 100 && !Chung.Dung(cs.Tt, "mo_ban")));
        await Ngu(1.5);
        Log($"   Chi thấy: {chi.LblTrangThaiSale.Text} {chi.LblDongHo.Text}, nút '{chi.BtnMua.Text}'");
        var anhChi = GD.Chup(chi);                       // Chi: lúc đếm ngược
        await Cho(() => an.BtnMua.Enabled);
        Log("6. Mở bán: An mua 2; 50 người ảo cùng mua; Bình mua 2");
        an.Mua(2);
        await Cho(() => !an.DangCho);
        sv.TxtN.Text = "50";
        sv.BtnBan.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong);
        HienLaiBang(sv);                                 // bảng: hiện lại kết quả so sánh 4 chế độ
        binh.Mua(2);
        await Cho(() => !binh.DangCho);
        await Ngu(0.6);
        InCuaSo([an, binh]);
        InServer(sv);
        Log("7. Chụp cửa sổ (Chi: lúc đếm ngược)");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), anhChi]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        Log("8. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   An: {an.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
