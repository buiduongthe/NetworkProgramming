using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_8;

// Bài mở rộng MR6.8 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba ứng dụng đặt vé An, Bình, Chi. Kịch bản bấm nút
// như người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–3. chế độ khoa: cả ba chọn E7 rồi CÙNG bấm "GIỮ CHỖ" -> một người giữ được, người đó thanh toán;
//   4.   chế độ khong_khoa, trễ 0–20 ms: cả ba cùng giữ E8 -> máy chủ trả lời "ĐÃ GIỮ" cho nhiều người, sau cap_nhat
//        người thua thấy "Ghế E8 đã bị … lấy mất!" -> SAI PHẠM; chụp ket_qua_gui_tranh_chap.png;
//   5.   khoa: So sánh 4 chế độ (500 người cùng nhắm ghế nóng E7) -> bảng, dòng khong_khoa tô đỏ;
//   6.   giữ chỗ 6 s: An giữ F5, F6 không thanh toán -> hết hạn, ghế trống lại; Bình mua F5, F6; Chi mua G7, G8;
//        chụp 4 cửa sổ -> ket_qua_gui.png.
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

    static void InCuaSo(IEnumerable<FormDatVe> ds)
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

    public static async Task RunAsync(FormServerDatVe sv, List<FormDatVe> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa, giữ chỗ 30 s)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        await Ngu(1.0);
        Log("2. Cả ba chọn ghế E7 rồi CÙNG bấm 'GIỮ CHỖ'");
        foreach (var cs in ds) cs.Bam("E7");
        foreach (var cs in ds) cs.BtnGiu.PerformClick();
        await Cho(() => ds.All(cs => !cs.LblThongBao.Text.Contains("Đang giữ chỗ")));
        await Ngu(0.4);
        InCuaSo(ds);
        InServer(sv);
        var duoc = ds.First(cs => cs.Giu.Count > 0);
        Log($"3. {duoc.Ten} bấm 'THANH TOÁN'");
        duoc.BtnTt.PerformClick();
        await Cho(() => duoc.VeCuaToi.Count > 0);
        await Ngu(0.4);
        Log($"   {duoc.Ten}: {duoc.LblThongBao.Text}");
        Log($"   máy chủ: {Chung.Chuoi(sv.Lop.ThongKeDayDu(), "tom_tat")}");
        Log("4. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms (CSDL chậm)");
        sv.TxtTre.Text = "20";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa"));
        await Ngu(0.5);
        Log("   Cả ba chọn E8 rồi CÙNG bấm 'GIỮ CHỖ'");
        var truoc = ds.ToDictionary(cs => cs.Ten, cs => cs.SoGiu);
        foreach (var cs in ds) cs.Bam("E8");
        foreach (var cs in ds) cs.BtnGiu.PerformClick();
        await Cho(() => ds.All(cs => !cs.LblThongBao.Text.Contains("Đang giữ chỗ")));
        foreach (var cs in ds) Log($"   {cs.Ten}: máy chủ trả lời " + (cs.SoGiu > truoc[cs.Ten] ? "ĐÃ GIỮ E8" : "từ chối"));
        await Ngu(0.6);
        Log("   …sau cap_nhat của máy chủ:");
        InCuaSo(ds);
        InServer(sv);
        Log($"   E8 thuộc: {Chung.Chuoi(sv.Lop.TrangThai()["chu"] as JsonObject, "E8") ?? "None"}");
        var nanNhan = ds.FirstOrDefault(cs => cs.LblThongBao.Text.Contains("lấy mất")) ?? binh;
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(nanNhan)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB)");
        Log("5. khoa, trễ 0–2 ms; So sánh 4 chế độ (ghế nóng E7)");
        sv.TxtTre.Text = "2";
        sv.BtnTre.PerformClick();
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 120);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("6. Đặt lại: giữ chỗ 6 s; An giữ F5, F6 (không thanh toán)");
        sv.DatThamSo(("ttl_s", 6));
        sv.BtnDatLai.PerformClick();
        await Cho(() => ds.All(cs => Chung.SoD(cs.Tt, "ttl_s") == 6 && Chung.SoL(cs.Tt?["dem"] as JsonObject, "ban") == 0));
        await Ngu(0.8);
        an.Bam("F5");
        an.Bam("F6");
        an.BtnGiu.PerformClick();
        await Cho(() => an.Giu.Count > 0);
        await Ngu(0.5);
        binh.Bam("F6");
        Log($"   Bình bấm F6: {binh.LblThongBao.Text}");
        await Cho(() => an.Giu.Count == 0);
        Log($"   hết hạn – An: {an.LblThongBao.Text}");
        await Ngu(0.3);
        Log("   Bình giữ F5, F6 rồi thanh toán; Chi mua G7, G8");
        foreach (var g in new[] { "F5", "F6" }) binh.Bam(g);
        foreach (var g in new[] { "G7", "G8" }) chi.Bam(g);
        binh.BtnGiu.PerformClick();
        chi.BtnGiu.PerformClick();
        await Cho(() => binh.Giu.Count > 0 && chi.Giu.Count > 0);
        await Ngu(0.4);
        binh.BtnTt.PerformClick();
        chi.BtnTt.PerformClick();
        await Cho(() => binh.VeCuaToi.Count > 0 && chi.VeCuaToi.Count > 0);
        await Ngu(0.4);
        Log("   An giữ E5, E6; Chi chọn H6");
        an.Bam("E5");
        an.Bam("E6");
        an.BtnGiu.PerformClick();
        chi.Bam("H6");
        await Cho(() => an.Giu.Count > 0);
        await Ngu(1.5);
        InCuaSo(ds);
        Log($"   An: đồng hồ giữ chỗ {an.LblDongHo.Text}");
        Log($"   máy chủ: {Chung.Chuoi(sv.Lop.ThongKeDayDu(), "tom_tat")}");
        InServer(sv);
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
