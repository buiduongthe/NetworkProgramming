using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_10;

// Bài mở rộng MR6.10 – Bảng điều khiển máy chủ đăng ký học phần (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": tóm tắt số chỗ đã đăng ký, mỗi lớp một
// thanh sĩ số (đỏ khi đủ; ghi "(sổ N)" đỏ khi sổ kiểm toán đếm vượt sĩ số), tham số đặt lại (số lớp, sĩ số).
sealed class FormServerDangKy(string host = "127.0.0.1", int cong = 42742)
    : FormServerCoSo(bao => new MayChuDangKy("khoa", 2.0, bao), host, cong, 2000, 200, MoPhong.SoLanMacDinh)
{
    const int Rong = 150;                                // bề rộng thanh sĩ số (điểm ảnh)
    protected override string TieuDe => "Máy chủ đăng ký học phần MR6.10";
    protected override string TenTaiNguyen => "Tài nguyên: lớp học phần";
    protected override Size KichThuocTaiNguyen => new(340, 186);

    Label lblTong = null!;
    readonly List<(Label Ma, OVe Thanh, Label So)> hang = [];
    readonly double[] tiLe = new double[DanhMuc.Lop.Length];
    readonly bool[] day = new bool[DanhMuc.Lop.Length];
    readonly Dictionary<string, TextBox> oTs = [];

    protected override void VeTaiNguyen(GroupBox g)
    {
        lblTong = GD.Nhan("–", 8, 20, 330, GD.Chu(12, true), 24);
        lblTong.ForeColor = GD.Hex("#8b0000");
        GD.Them(g, lblTong);
        for (int i = 0; i < DanhMuc.Lop.Length; i++)
        {
            int y = 48 + i * 21, so = i;
            var ma = GD.Nhan("", 8, y, 82, GD.CoDinh, 18);
            var thanh = new OVe(92, y + 2, Rong, 14) { BackColor = GD.Hex("#dddddd") };
            thanh.Ve = gr =>
            {
                using var b = new SolidBrush(day[so] ? GD.DoDam : GD.Hex("#3cb44b"));
                gr.FillRectangle(b, 0, 0, (float)(Rong * tiLe[so]), 14);
            };
            var sl = GD.Nhan("", 248, y, 96, GD.Chu(9, true), 18);
            hang.Add((ma, thanh, sl));
            GD.Them(g, ma, thanh, sl);
        }
        var ts = Lop.ThamSo;
        oTs["so_lop"] = GD.O(Chung.Repr(ts["so_lop"]), 56, 178, 30);
        oTs["si_so"] = GD.O(Chung.Repr(ts["si_so"]), 142, 178, 44);
        GD.Them(g, GD.Nhan("Số lớp:", 8, 181), oTs["so_lop"], GD.Nhan("Sĩ số:", 100, 181), oTs["si_so"]);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object> { ["so_lop"] = long.Parse(oTs["so_lop"].Text, ic), ["si_so"] = long.Parse(oTs["si_so"].Text, ic) };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("tham số phải là số nguyên"); }
    }

    /// <summary>Điền ô tham số (kịch bản demo) – như người dùng gõ.</summary>
    public void DatThamSo(params (string Ma, object GiaTri)[] ts)
    {
        foreach (var (k, v) in ts) oTs[k].Text = Chung.Repr(v);
    }

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        lblTong.Text = Chung.Chuoi(tk, "tom_tat");
        var lop = (tt["lop"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        for (int i = 0; i < hang.Count; i++)
        {
            var (ma, thanh, so) = hang[i];
            if (i >= lop.Count)
            {
                (ma.Text, so.Text, thanh.Visible) = ("", "", false);
                continue;
            }
            var x = lop[i];
            long daDk = Chung.SoL(x, "da_dk"), siSo = Chung.SoL(x, "si_so"), theoSo = Chung.SoL(x, "theo_so");
            ma.Text = Chung.Chuoi(x, "ma");
            (tiLe[i], day[i], thanh.Visible) = (Math.Min(1.0, (double)daDk / Math.Max(1, siSo)), daDk >= siSo, true);
            thanh.Invalidate();
            bool vuot = theoSo > siSo;
            so.Text = $"{daDk}/{siSo}" + (vuot ? $" (sổ {theoSo})" : "");
            so.ForeColor = vuot ? GD.DoDam : Color.Black;
        }
    }
}
