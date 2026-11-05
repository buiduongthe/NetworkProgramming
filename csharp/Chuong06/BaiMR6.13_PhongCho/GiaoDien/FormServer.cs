using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_13;

// Bài mở rộng MR6.13 – Bảng điều khiển máy chủ bán vé có phòng chờ ảo (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này vẽ khung "Tài nguyên": vé còn lại (thanh tiến trình), số người trong
// hàng đợi, sổ đã bán (đỏ khi vượt), số thứ tự đang phục vụ / đã phát, số yêu cầu mua đang xử lý / giới hạn và số lần
// từ chối quá tải, tốc độ cấp lượt (đổi ngay khi đang chạy), thùng token, tham số đặt lại, ô "Bật phòng chờ ảo", nút
// "So sánh bật/tắt phòng chờ" kèm bảng kết quả (dòng có quá tải tô đỏ).
sealed class FormServerPhongCho(string host = "127.0.0.1", int cong = 42772)
    : FormServerCoSo(bao => new MayChuPhongCho("khoa", 2.0, bao), host, cong, 1000, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ bán vé MR6.13";
    protected override string TenTaiNguyen => "Tài nguyên: vé và phòng chờ ảo";
    protected override Size KichThuocTaiNguyen => new(372, 320);

    Label lblVe = null!, lblHang = null!, lblBan = null!, lblPhucVu = null!, lblXuLy = null!, lblToken = null!;
    ProgressBar thanhVe = null!;
    public TextBox TxtTocDo = null!;
    public Button BtnTocDo = null!, BtnSoSanhPc = null!;
    readonly Dictionary<string, TextBox> oTs = [];
    CheckBox oPc = null!;
    ListView bangPc = null!;
    public List<MoPhong.HangPhongCho> HangPc = [];

    MayChuPhongCho LopPc => (MayChuPhongCho)Lop;

    protected override void VeTaiNguyen(GroupBox g)
    {
        Label Nho(string t, int x) { var l = GD.Nhan(t, x, 20, 170, GD.Chu(8, true), 14); l.ForeColor = GD.Hex("#404040"); return l; }
        lblVe = GD.Nhan("–", 8, 34, 176, GD.Chu(16, true), 30);
        lblVe.ForeColor = GD.Hex("#8b0000");
        lblHang = GD.Nhan("–", 190, 34, 180, GD.Chu(16, true), 30);
        lblHang.ForeColor = GD.Hex("#1f4fa0");
        thanhVe = new ProgressBar { Location = new Point(8, 66), Size = new Size(360, 14), Maximum = 500 };
        lblBan = GD.Nhan("", 8, 84, 360, GD.Chu(9), 18);
        lblPhucVu = GD.Nhan("", 8, 102, 360, GD.Chu(9), 18);
        lblXuLy = GD.Nhan("", 8, 120, 360, GD.Chu(9, true), 18);
        var ts = Lop.ThamSo;
        TxtTocDo = GD.O(Chung.G((double)ts["toc_do"]), 64, 142, 44);
        BtnTocDo = GD.Nut("Áp dụng ngay", GD.XanhDuong, 164, 140, 96, 24);
        BtnTocDo.Click += (_, _) => DoiTocDo();
        lblToken = GD.Nhan("", 264, 145, 106, GD.Chu(8), 16);
        GD.Them(g, Nho("VÉ CÒN LẠI", 8), Nho("HÀNG ĐỢI", 190), lblVe, lblHang, thanhVe, lblBan, lblPhucVu, lblXuLy,
                GD.Nhan("Cấp lượt:", 8, 145), TxtTocDo, GD.Nhan("người/s", 112, 145), BtnTocDo, lblToken);
        (string Ma, string Chu, int X, int Y, int XO, int W)[] oDs =
            [("tong_ve", "Tổng vé:", 8, 170, 92, 50), ("dung_luong", "Thùng token:", 160, 170, 252, 40),
             ("luot_s", "Hạn lượt (s):", 8, 196, 92, 50), ("gioi_han_xu_ly", "Giới hạn xử lý:", 160, 196, 252, 40)];
        foreach (var (ma, chu, x, y, xo, w) in oDs)
        {
            oTs[ma] = GD.O(ts[ma] is double d ? Chung.G(d) : Chung.Repr(ts[ma]), xo, y, w);
            GD.Them(g, GD.Nhan(chu, x, y + 3), oTs[ma]);
        }
        oPc = new CheckBox { Text = "Bật phòng chờ ảo (có hiệu lực khi Đặt lại)", Location = new Point(8, 222), Size = new Size(300, 20), Checked = (long)ts["phong_cho"] == 1 };
        BtnSoSanhPc = GD.Nut("So sánh bật/tắt phòng chờ", GD.Cam, 8, 246, 200, 26);
        BtnSoSanhPc.Click += (_, _) => SoSanhPc();
        bangPc = new ListView
        {
            View = View.Details, FullRowSelect = true, Location = new Point(8, 276), Size = new Size(360, 62), HeaderStyle = ColumnHeaderStyle.Nonclickable,
        };
        foreach (var (t, w) in new[] { ("Phòng chờ", 70), ("Mua được", 62), ("Hết vé", 52), ("Quá tải", 56), ("Bỏ cuộc", 58), ("p99 ms", 58) })
            bangPc.Columns.Add(t, w, t == "Phòng chờ" ? HorizontalAlignment.Left : HorizontalAlignment.Right);
        GD.Them(g, oPc, BtnSoSanhPc, bangPc);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["tong_ve"] = long.Parse(oTs["tong_ve"].Text, ic), ["dung_luong"] = long.Parse(oTs["dung_luong"].Text, ic),
                ["luot_s"] = double.Parse(oTs["luot_s"].Text, NumberStyles.Float, ic), ["gioi_han_xu_ly"] = long.Parse(oTs["gioi_han_xu_ly"].Text, ic),
                ["toc_do"] = double.Parse(TxtTocDo.Text, NumberStyles.Float, ic), ["phong_cho"] = oPc.Checked ? 1L : 0L,
            };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("tham số phải là số"); }
    }

    /// <summary>Điền ô tham số / ô chọn (kịch bản demo) – như người dùng gõ.</summary>
    public void DatThamSo(params (string Ma, object GiaTri)[] ts)
    {
        foreach (var (k, v) in ts)
        {
            string chu = v is double d ? Chung.G(d) : Chung.Repr(v);
            if (k == "phong_cho") oPc.Checked = chu == "1";
            else if (k == "toc_do") TxtTocDo.Text = chu;
            else oTs[k].Text = chu;
        }
    }

    void DoiTocDo()
    {
        try { LopPc.DatTocDo(double.Parse(TxtTocDo.Text, NumberStyles.Float, CultureInfo.InvariantCulture)); }
        catch (Exception e) when (e is ArgumentException or FormatException) { NhatKy.Ghi($"Tốc độ không hợp lệ: {e.Message}"); }
    }

    // ------------------------------------------------------------ so sánh bật/tắt phòng chờ
    public void SoSanhPc()
    {
        if (DangMoPhong || !Lop.Chay)
        {
            NhatKy.Ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
            return;
        }
        int n, k;
        try
        {
            (n, k) = (int.Parse(TxtN.Text, CultureInfo.InvariantCulture), int.Parse(TxtK.Text, CultureInfo.InvariantCulture));
            if (n < 1 || k < 1) throw new FormatException("n, k ≥ 1");
        }
        catch (Exception e) when (e is FormatException or OverflowException)
        {
            NhatKy.Ghi($"Tham số mô phỏng không hợp lệ: {e.Message}");
            return;
        }
        DangMoPhong = true;
        BtnBan.Enabled = BtnSoSanh.Enabled = BtnSoSanhPc.Enabled = false;
        (LblMoPhong.Text, LblMoPhong.ForeColor) = ("đang chạy…", GD.DoDam);
        int i = Lop.DiaChi.LastIndexOf(':');
        string host = Lop.DiaChi[..i];
        int cong = int.Parse(Lop.DiaChi[(i + 1)..], CultureInfo.InvariantCulture);
        void Bao(string text) => Ui.DuaVao(() => NhatKy.Ghi("[mô phỏng] " + text));
        Task.Run(() =>                                   // mô phỏng chạy nền qua TCP tới chính máy chủ này
        {
            List<MoPhong.HangPhongCho> hang;
            try { hang = MoPhong.SoSanhPhongCho(host, cong, n, Math.Min(k, n), Bao); }
            catch (Exception e)
            {
                Bao($"lỗi: {e.Message}");
                hang = [];
            }
            Ui.DuaVao(() => XongPc(hang));
        });
    }

    void XongPc(List<MoPhong.HangPhongCho> hang)
    {
        DangMoPhong = false;
        BtnBan.Enabled = BtnSoSanh.Enabled = BtnSoSanhPc.Enabled = true;
        (LblMoPhong.Text, LblMoPhong.ForeColor) = ("xong", GD.XanhDam);
        if (hang.Count == 0) return;
        HangPc = hang;
        bangPc.Items.Clear();
        foreach (var h in hang)
        {
            var item = new ListViewItem([h.PhongCho, $"{h.MuaDuoc}", $"{h.HetVe}", $"{h.QuaTai}", $"{h.BoCuoc}", h.P99Ms is { } p ? Chung.F(p, "F1") : "–"]);
            (item.BackColor, item.ForeColor) = h.QuaTai > 0 ? (GD.Hex("#ffc8c8"), GD.DoDam) : (GD.Hex("#d8f5d8"), Color.Black);
            bangPc.Items.Add(item);
        }
        foreach (var d in MoPhong.BangPhongCho(hang)) NhatKy.Ghi(d.Trim());
    }

    // ------------------------------------------------------------ vẽ lại định kỳ
    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        BtnSoSanhPc.Enabled = !DangMoPhong;              // đang có lượt mô phỏng (của bất kỳ nút nào): khóa nút
        var sk = tt["su_kien"] as JsonObject;
        long tongVe = Chung.SoL(sk, "tong_ve"), con = Chung.SoL(tt, "con_lai"), banSo = Chung.SoL(tt, "ban_so");
        bool pc = Chung.Dung(tt, "phong_cho");
        lblVe.Text = $"{con} / {tongVe}";
        thanhVe.Maximum = (int)Math.Max(1, tongVe);
        thanhVe.Value = (int)Math.Clamp(con, 0, thanhVe.Maximum);
        (lblHang.Text, lblHang.ForeColor) = pc ? ($"{Chung.SoL(tt, "dang_cho")} người", GD.Hex("#1f4fa0")) : ("TẮT", GD.Hex("#808080"));
        bool vuot = banSo > tongVe;
        lblBan.Text = $"Sổ đã bán: {banSo} vé, {Chung.SoL(tt, "so_don")} đơn" + (vuot ? $" – VƯỢT {banSo - tongVe} vé!" : "")
                      + (Chung.Dung(tt, "het_ve") ? "  [HẾT VÉ]" : "");
        lblBan.ForeColor = vuot ? GD.DoDam : Color.Black;
        lblPhucVu.Text = $"Đang phục vụ tới #{Chung.SoL(tt, "dang_phuc_vu")} / đã phát #{Chung.SoL(tt, "so_cap")} ·"
                         + $" lượt đã cấp {Chung.SoL(tt, "da_cap_luot")}, hết hạn {Chung.SoL(tt, "het_han")}";
        long quaTai = Chung.SoL(tt, "qua_tai");
        lblXuLy.Text = $"Mua đang xử lý: {Chung.SoL(tt, "dang_xu_ly")} / {Chung.SoL(tt, "gioi_han")} (max {Chung.SoL(tt, "xu_ly_max")})"
                       + $" · từ chối quá tải: {quaTai}";
        lblXuLy.ForeColor = quaTai > 0 ? GD.DoDam : GD.XanhDam;
        lblToken.Text = $"thùng {Chung.SoL(tt, "token")}/{Chung.SoL(tt, "dung_luong")} token";
    }
}
