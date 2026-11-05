using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_10;

// Bài mở rộng MR6.10 – Kịch bản minh họa tự động cho bản giao diện (vai trò demo, Windows).
//
// Một tiến trình, một luồng giao diện: bảng điều khiển máy chủ và ba cổng đăng ký An, Bình, Chi. Kịch bản bấm nút như
// người dùng (hàm async: await Task.Delay trả luồng cho vòng lặp thông điệp), in `[demo] …` – cùng các bước với
// demo_gui.py của bản Python:
//   1–2. chế độ khoa, 3 lớp sĩ số 1: cả ba CÙNG bấm Đăng ký lớp 123013-01 -> đúng một người được;
//   3.   chế độ khong_khoa, trễ 0–20 ms: cùng thao tác đó -> máy chủ trả lời "thành công" cho nhiều người, nhưng
//        danh sách lớp chỉ có một tên -> người còn lại thấy "Đăng ký … đã bị MẤT"; SAI PHẠM; chụp
//        ket_qua_gui_tranh_chap.png;
//   4.   khoa, 3 lớp × 50: So sánh 4 chế độ -> bảng, dòng khong_khoa tô đỏ;
//   5–6. An đăng ký 2 lớp, Bình 1 lớp; bắn đồng loạt 500 sinh viên ảo; Chi đăng ký lớp đã đủ -> từ chối; An hủy,
//        Chi đăng ký được; chụp 4 cửa sổ ghép thành ket_qua_gui.png.
static class DemoGui
{
    const string Lop1 = "123013-01", Lop2 = "123005-01";

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

    static void InCuaSo(IEnumerable<FormDangKy> ds)
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

    static void InLop(FormServerCoSo sv)
    {
        foreach (var x in (sv.Lop.TrangThai()["lop"] as JsonArray ?? []).OfType<JsonObject>())
        {
            var ten = (x["sv"] as JsonArray ?? []).Take(3).Select(Chung.ChuoiPy);
            Log($"   {Chung.Chuoi(x, "ma")}: {Chung.SoL(x, "da_dk")}/{Chung.SoL(x, "si_so")} {string.Join(" ", ten)}".TrimEnd());
        }
    }

    static void DatLai(FormServerDangKy sv, params (string, object)[] ts)
    {
        sv.DatThamSo(ts);
        sv.BtnDatLai.PerformClick();
    }

    static Func<bool> Xong(IEnumerable<FormDangKy> ds) => () => ds.All(cs => cs.DangCho.Count == 0);

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

    public static async Task RunAsync(FormServerDangKy sv, List<FormDangKy> ds, string anh)
    {
        var (an, binh, chi) = (ds[0], ds[1], ds[2]);
        string thuMuc = Path.GetDirectoryName(Path.GetFullPath(anh))!;
        await Ngu(0.5);
        Log("1. Máy chủ: START SERVER (chế độ khoa)");
        sv.BtnStart.PerformClick();
        await Ngu(0.3);
        Log("   An, Bình, Chi: Kết nối");
        foreach (var cs in ds) cs.BtnKetNoi.PerformClick();
        await Cho(() => ds.All(cs => cs.Tt != null));
        Log("2. Đặt lại: 3 lớp, sĩ số 1 (còn đúng 1 chỗ mỗi lớp)");
        DatLai(sv, ("so_lop", 3L), ("si_so", 1L));
        await Cho(() => ds.All(cs => Chung.SoL(cs.Tt, "si_so") == 1));
        await Ngu(0.5);
        Log($"   An, Bình, Chi CÙNG bấm Đăng ký {Lop1}");
        foreach (var cs in ds) cs.DangKy(Lop1);
        await Cho(Xong(ds));
        await Ngu(0.8);
        InCuaSo(ds);
        InServer(sv);
        Log("3. Máy chủ: khong_khoa, độ trễ xử lý 0–20 ms; Đặt lại");
        sv.TxtTre.Text = "20";
        sv.BtnTre.PerformClick();
        DatLai(sv, ("so_lop", 3L), ("si_so", 1L));
        sv.ChonCheDo("khong_khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khong_khoa");
        await Cho(() => ds.All(cs => Chung.Chuoi(cs.Tt, "che_do") == "khong_khoa" && cs.CuaToi.Count == 0));
        await Ngu(0.5);
        Log($"   An, Bình, Chi CÙNG bấm Đăng ký {Lop1}");
        foreach (var cs in ds) cs.DangKy(Lop1);
        await Cho(Xong(ds));
        Log("   ngay khi có trả lời:");
        InCuaSo(ds);
        await Ngu(1.2);
        Log("   1 giây sau (đối chiếu danh sách lớp):");
        InCuaSo(ds);
        InServer(sv);
        InLop(sv);
        var mat = ds.FirstOrDefault(cs => cs.LblThongBao.Text.Contains("MẤT")) ?? binh;
        string ra = Path.Combine(thuMuc, "ket_qua_gui_tranh_chap.png");
        GD.Ghep(ra, [[GD.Chup(sv), GD.Chup(mat)]]);
        Log($"   Đã ghi {Path.GetFileName(ra)} ({new FileInfo(ra).Length / 1024} KB, cửa sổ {mat.Ten})");
        Log("4. Máy chủ: khoa, trễ 0–2 ms, 3 lớp × 50; So sánh");
        sv.TxtTre.Text = "2";
        sv.BtnTre.PerformClick();
        DatLai(sv, ("so_lop", 3L), ("si_so", 50L));
        sv.ChonCheDo("khoa");
        await Cho(() => sv.Lop.Kho.CheDo == "khoa");
        await Ngu(0.3);
        sv.BtnSoSanh.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong, 240);
        foreach (var d in Chung.BangSoSanh(sv.HangSoSanh)) Log(d);
        foreach (var cau in sv.LblKetLuan.Text.Split(". ")) Log($"   {cau.TrimEnd('.')}");
        await Ngu(0.5);
        Log("5. Đặt lại; An đăng ký 2 lớp, Bình 1 lớp");
        DatLai(sv, ("so_lop", 3L), ("si_so", 50L));
        await Cho(() => ds.All(cs => cs.CuaToi.Count == 0 && cs.Hang.TryGetValue(Lop1, out var h) && h.Nut.Text == "Đăng ký"));
        await Ngu(0.5);
        an.DangKy(Lop1);
        an.DangKy(Lop2);
        binh.DangKy(Lop1);
        await Cho(Xong(ds));
        await Ngu(0.5);
        Log("   Bắn đồng loạt 500 sinh viên ảo (mỗi người 1 lớp)");
        sv.TxtN.Text = "500";
        sv.BtnBan.PerformClick();
        await Ngu(0.3);
        await Cho(() => !sv.DangMoPhong);
        await Ngu(0.8);
        HienLaiBang(sv);                                 // bảng: hiện lại kết quả so sánh 4 chế độ
        InLop(sv);
        Log($"   Chi bấm '{chi.Hang[Lop2].Nut.Text}' ở {Lop2}");
        chi.DangKy(Lop2);
        await Cho(Xong(ds));
        await Ngu(0.5);
        InCuaSo([chi]);
        Log($"6. An bấm Hủy {Lop2}, rồi Chi bấm Đăng ký {Lop2}");
        an.DangKy(Lop2);
        await Cho(Xong(ds));
        await Cho(() => chi.Hang[Lop2].Nut.Text == "Đăng ký");
        chi.DangKy(Lop2);
        await Cho(Xong(ds));
        await Ngu(0.6);
        InCuaSo(ds);
        InServer(sv);
        Log("7. Chụp 4 cửa sổ");
        GD.Ghep(anh, [[GD.Chup(sv), GD.Chup(an)], [GD.Chup(binh), GD.Chup(chi)]]);
        Log($"   Đã ghi {Path.GetFileName(anh)} ({new FileInfo(anh).Length / 1024} KB)");
        Log("8. Máy chủ: STOP SERVER");
        sv.BtnStop.PerformClick();
        await Cho(() => sv.BtnStart.Enabled);
        await Ngu(0.5);
        Log($"   An: {an.LblTrangThai.Text}");
        foreach (var cs in ds) cs.Close();
    }
}
