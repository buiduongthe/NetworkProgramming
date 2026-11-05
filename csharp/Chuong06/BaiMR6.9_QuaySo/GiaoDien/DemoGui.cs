using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba vòng quay: An, Bình và "Bình (máy 2)" – cùng
// một người mở trên hai thiết bị. Kịch bản bấm nút như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp
// thông điệp), in `[demo] …` – cùng các bước với demo_gui.py của bản Python:
//   1–2. chế độ khoa, 1 món quà, p = 1: cả ba cùng bấm QUAY -> đúng một người trúng, thiết bị thứ hai của Bình bị
//        từ chối "đã quay";
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> nhiều người "trúng" một món quà -> SAI PHẠM (thừa quà,
//        quay 2 lần); chụp ket_qua_gui_tranh_chap.png;
//   4.   So sánh 4 chế độ (10 000 người, p = 1/20, 200 kết nối) -> bảng, dòng khong_khoa tô đỏ;
//   5–6. 10 quà, p = 0.8: An, Bình quay; Bình (máy 2) quay -> "đã quay"; bắn đồng loạt 500 người -> danh sách người
//        trúng trực tiếp; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
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

    static void InCuaSo(IEnumerable<FormVongQuay> ds)
    {
        foreach (var cs in ds) Log($"   {cs.NhanHien}: {cs.ThongBao}");
    }

    static void InServer(FormServerCoSo sv)
    {
        var tk = sv.Lop.ThongKeDayDu();
        Log($"   máy chủ: {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, từ chối {Chung.SoL(tk, "tu_choi")},"
            + $" sai phạm {Chung.SoL(tk, "sai_pham_tong")}");
        foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) Log($"     ví dụ: {Chung.ChuoiPy(v)}");
        Log($"   {Chung.Chuoi(tk, "tom_tat")}");
    }

    static void DatLai(FormServerQuaySo sv, params (string, object)[] ts)
    {
        sv.DatThamSo(ts);
        sv.BtnDatLai.PerformClick();
    }

    /// <summary>Hiện lại bảng so sánh 4 chế độ (sau "Bắn đồng loạt", dòng của chế độ đang chạy bị thay bằng lượt
    /// mới) – như sv._xong_mo_phong(sv.hang_so_sanh) của bản Python.</summary>
    static void HienLaiBang(FormServerCoSo sv)
    {
        sv.Bang.Items.Clear();
        foreach (var h in sv.HangSoSanh)
        {
            var item = new ListViewItem([h.CheDo, $"{h.ThanhCong}", $"{h.SaiPham}", $"{h.XungDot}", $"{h.YcS}", h.P99Ms is { } p ? Chung.F(p, "F1") : "–"]);
            (item.BackColor, item.ForeColor) = h.SaiPham > 0 ? (GD.Hex("#ffc8c8"), GD.DoDam) : (GD.Hex("#d8f5d8"), System.Drawing.Color.Black);
            sv.Bang.Items.Add(item);
        }
        var sai = sv.HangSoSanh.Where(h => h.SaiPham > 0).Select(h => h.CheDo).ToList();
        var dung = sv.HangSoSanh.Where(h => h.SaiPham == 0).Select(h => h.CheDo).ToList();
        sv.LblKetLuan.Text = (sai.Count > 0 ? $"Sai phạm: {string.Join(", ", sai)}. " : "")
                             + (dung.Count > 0 ? $"Không sai phạm: {string.Join(", ", dung)}." : "");
    }

    static void CungBam(IEnumerable<FormVongQuay> ds)
    {
        foreach (var cs in ds) cs.BtnQuay.PerformClick();
    }

    public static async Task RunAsync(FormServerQuaySo sv, List<FormVongQuay> ds, string anh)
    {
        var (an, binh, binh2) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Bình (máy 2): Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        Log("2. Đặt lại: 1 món quà, p = 1 (lượt đầu chắc trúng)");
        DatLai(sv, ("so_qua", 1L), ("p", 1.0));
        await Cho(() => ds.All(cs => Chung.SoL(cs.Tt, "so_qua") == 1));
        await Ngu(0.5);
        Log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
        CungBam(ds);
        await Ngu(0.3);
        await Cho(() => ds.All(cs => cs.Pha == null));
        InCuaSo(ds);
        InServer(sv);
        Log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
        sv.TxtTre.Text = "20";
        sv.BtnTre.PerformClick();
        DatLai(sv, ("so_qua", 1L), ("p", 1.0));
        sv.ChonCheDo("khong_khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa" && cs.BtnQuay.Enabled));
        await Ngu(0.5);
        Log("   An, Bình, Bình (máy 2) CÙNG bấm QUAY");
        CungBam(ds);
        await Ngu(0.3);
        await Cho(() => ds.All(cs => cs.Pha == null));
        await Ngu(0.5);
        InCuaSo(ds);
        InServer(sv);
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(binh2)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("4. Máy chủ: khoa, trễ 0–2 ms; 10 quà, p = 0.05 (1/20);");
        Log("   So sánh 4 chế độ, 10000 người");
        sv.TxtTre.Text = "2";
        sv.BtnTre.PerformClick();
        DatLai(sv, ("so_qua", 10L), ("p", 0.05));
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        sv.TxtN.Text = "10000";
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("5. Đặt lại: 10 quà, p = 0.8; An quay, rồi Bình quay");
        DatLai(sv, ("so_qua", 10L), ("p", 0.8));
        await Cho(() => ds.All(cs => Chung.SoD(cs.Tt, "p") == 0.8 && cs.BtnQuay.Enabled));
        await Ngu(0.5);
        an.BtnQuay.PerformClick();
        await Ngu(0.4);
        binh.BtnQuay.PerformClick();
        await Ngu(0.3);
        await Cho(() => an.Pha == null && binh.Pha == null);
        Log("   Bình (máy 2) bấm QUAY");
        binh2.BtnQuay.PerformClick();
        await Ngu(0.3);
        await Cho(() => binh2.Pha == null);
        InCuaSo(ds);
        Log("6. Bắn đồng loạt 500 người (chế độ khoa)");
        sv.TxtN.Text = "500";
        sv.BtnBan.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong);
        await Ngu(0.8);
        HienLaiBang(sv);                                 // bảng: hiện lại kết quả so sánh 4 chế độ
        InServer(sv);
        Log("   Người trúng gần nhất (cửa sổ An):");
        foreach (var x in an.Ls.Items.Cast<string>().Take(4)) Log($"     {x}");
        Log("7. Chụp 4 cửa sổ");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), GD.Chup(binh2)]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        Log("8. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   An: {an.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
