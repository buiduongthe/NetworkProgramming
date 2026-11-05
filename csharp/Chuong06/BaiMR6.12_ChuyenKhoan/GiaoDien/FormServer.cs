using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_12;

// Bài mở rộng MR6.12 – Bảng điều khiển máy chủ ngân hàng (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": TỔNG TIỀN HỆ THỐNG (xanh khi khớp, đỏ khi
// lệch), mỗi tài khoản một thanh số dư (vạch xám = số dư ban đầu), số giao dịch và số vụ khóa chết đã phát hiện, 3
// giao dịch gần nhất, tham số đặt lại (số tài khoản, số dư ban đầu, ô chọn "khóa ngây thơ").
sealed class FormServerNganHang(string host = "127.0.0.1", int cong = 42762)
    : FormServerCoSo(bao => new MayChuNganHang("khoa", 2.0, bao), host, cong, 500, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ ngân hàng MR6.12";
    protected override string TenTaiNguyen => "Tài nguyên: các tài khoản";
    protected override Size KichThuocTaiNguyen => new(370, 330);

    Label lblTong = null!, lblTongCt = null!, lblGd = null!;
    OVe bangTk = null!;
    JsonObject? ttVe;
    string veCu = "";
    ListBox ls = null!;
    TextBox oSoTk = null!, oSoDu = null!;
    CheckBox oNgayTho = null!;

    protected override void VeTaiNguyen(GroupBox g)
    {
        var tieuDe = GD.Nhan("TỔNG TIỀN HỆ THỐNG", 8, 20, 300, GD.Chu(8, true), 14);
        tieuDe.ForeColor = GD.Hex("#404040");
        lblTong = new Label { Text = "–", Location = new Point(8, 35), Size = new Size(262, 30), Font = GD.Chu(16, true), BackColor = GD.Hex("#e0e0e0"),
                              TextAlign = ContentAlignment.MiddleLeft };
        lblTongCt = GD.Nhan("", 8, 67, 360, GD.Chu(8), 16);
        bangTk = new OVe(8, 84, 360, 150) { BackColor = Color.White, Ve = VeTaiKhoan };
        lblGd = GD.Nhan("", 8, 237, 360, GD.Chu(9), 18);
        ls = new ListBox { Location = new Point(8, 256), Size = new Size(360, 42), Font = GD.ChuCoDinh(8), IntegralHeight = false, SelectionMode = SelectionMode.None };
        var ts = Lop.ThamSo;
        oSoTk = GD.O(Chung.Repr(ts["so_tk"]), 88, 301, 34);
        oSoDu = GD.O(Chung.Repr(ts["so_du_dau"]), 228, 301, 80);
        oNgayTho = new CheckBox { Text = "Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)", Location = new Point(8, 326), Size = new Size(300, 20),
                                  Checked = (long)ts["khoa_ngay_tho"] == 1 };
        GD.Them(g, tieuDe, lblTong, lblTongCt, bangTk, lblGd, ls, GD.Nhan("Số tài khoản:", 8, 304), oSoTk, GD.Nhan("Số dư ban đầu:", 132, 304), oSoDu, oNgayTho);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["so_tk"] = long.Parse(oSoTk.Text, ic), ["so_du_dau"] = long.Parse(oSoDu.Text, ic), ["khoa_ngay_tho"] = oNgayTho.Checked ? 1L : 0L,
            };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("tham số phải là số"); }
    }

    /// <summary>Bấm ô chọn "khóa ngây thơ" (kịch bản demo) – có hiệu lực khi bấm Đặt lại.</summary>
    public void DatKhoaNgayTho(bool bat)
    {
        if (oNgayTho.Checked != bat) oNgayTho.Checked = bat;
    }

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        long tong = Chung.SoL(tt, "tong"), tongDau = Chung.SoL(tt, "tong_dau"), lech = tong - tongDau;
        (lblTong.Text, lblTong.BackColor, lblTong.ForeColor) = (Chung.Tien(tong), GD.Hex(lech != 0 ? "#ff6060" : "#70d070"), lech != 0 ? Color.White : Color.Black);
        lblTongCt.Text = $"ban đầu {Chung.Tien(tongDau)} – " + (lech != 0 ? $"LỆCH {NganHang.TienDau(lech)}" : "khớp");
        lblTongCt.ForeColor = lech != 0 ? GD.DoDam : GD.XanhDam;
        string khoa = string.Join("|", (tt["tai_khoan"] as JsonArray ?? []).OfType<JsonObject>().Select(t => $"{Chung.Chuoi(t, "ma")}:{Chung.SoL(t, "so_du")}"));
        if (khoa != veCu)                                // chỉ vẽ lại khi số dư đổi
        {
            (veCu, ttVe) = (khoa, tt);
            bangTk.Invalidate();
        }
        long kc = Chung.SoL(tt, "khoa_chet");
        lblGd.Text = $"Giao dịch: {Chung.SoL(tt, "so_gd")}   Khóa chết phát hiện: {kc}"
                     + (Chung.Dung(tt, "khoa_ngay_tho") ? "   [khóa ngây thơ]" : "   [khóa theo thứ tự]");
        lblGd.ForeColor = kc > 0 ? GD.DoDam : Color.Black;
        var dong = (tt["lich_su"] as JsonArray ?? []).OfType<JsonObject>().Take(3)
            .Select(x => $"{Chung.Chuoi(x, "gio")} {Chung.Chuoi(x, "tu")}→{Chung.Chuoi(x, "den")} {Chung.Tien(Chung.SoL(x, "so_tien")),13} {Chung.Chuoi(x, "nguoi")}").ToList();
        if (!ls.Items.Cast<string>().SequenceEqual(dong))
        {
            ls.Items.Clear();
            foreach (var d in dong) ls.Items.Add(d);
        }
    }

    /// <summary>Mỗi tài khoản một thanh số dư (≤ 10 dòng mỗi cột, 2 cột khi > 10 tài khoản).</summary>
    void VeTaiKhoan(Graphics g)
    {
        if (ttVe == null) return;
        var ds = (ttVe["tai_khoan"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        long dau = Chung.SoL(ttVe, "tong_dau") / Math.Max(1, ds.Count);
        long lon = Math.Max(2 * dau, ds.Count > 0 ? ds.Max(t => Chung.SoL(t, "so_du")) : 0);
        if (lon <= 0) lon = 1;
        int cot = ds.Count <= 10 ? 1 : 2, rong = 360 / cot, cao = 15;
        using var f = GD.Chu(7);
        using var fc = GD.ChuCoDinh(7);
        using var giua = new StringFormat { LineAlignment = StringAlignment.Center };
        using var phai = new StringFormat { LineAlignment = StringAlignment.Center, Alignment = StringAlignment.Far };
        using var nenThanh = new SolidBrush(GD.Hex("#eeeeee"));
        using var vach = new Pen(GD.Hex("#808080"));
        for (int i = 0; i < ds.Count; i++)
        {
            var t = ds[i];
            int x = i / 10 * rong + 4, y = i % 10 * cao + 3;
            long soDu = Chung.SoL(t, "so_du");
            g.DrawString($"{Chung.Chuoi(t, "ma")} {Chung.Chuoi(t, "chu")}", f, Brushes.Black, x, y + 6, giua);
            float x0 = x + 62, x1 = x + rong - 76;
            g.FillRectangle(nenThanh, x0, y + 2, x1 - x0, 9);
            using (var b = new SolidBrush(GD.Hex(soDu >= dau ? "#4a90d9" : "#f0a030")))
                g.FillRectangle(b, x0, y + 2, Math.Max(0, (x1 - x0) * soDu / (float)lon), 9);
            float xd = x0 + (x1 - x0) * dau / (float)lon;
            g.DrawLine(vach, xd, y, xd, y + 13);
            g.DrawString(Chung.Tien(soDu), fc, Brushes.Black, x + rong - 6, y + 6, phai);
        }
    }
}
