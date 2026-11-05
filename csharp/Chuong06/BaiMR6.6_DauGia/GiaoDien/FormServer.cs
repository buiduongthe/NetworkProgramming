using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_6;

// Bài mở rộng MR6.6 – Bảng điều khiển máy chủ đấu giá (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": món hàng, giá hiện tại, người dẫn đầu, đồng
// hồ phiên, vài giá gần nhất và tham số đặt lại phiên.
sealed class FormServerDauGia(string host = "127.0.0.1", int cong = 42702)
    : FormServerCoSo(bao => new MayChuDauGia("khoa", 2.0, bao, new OrderedDictionary<string, object> { ["thoi_gian_s"] = 120.0 }),
                     host, cong, 200, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ đấu giá MR6.6";
    protected override string TenTaiNguyen => "Tài nguyên: phiên đấu giá";
    protected override Size KichThuocTaiNguyen => new(392, 188);

    Label lblMon = null!, lblGia = null!, lblDanDau = null!, lblDongHo = null!;
    ListBox ls = null!;
    readonly Dictionary<string, TextBox> oTs = [];

    protected override void VeTaiNguyen(GroupBox g)
    {
        var anh = new OVe(8, 20, 120, 130) { Ve = gr => BinhGom.Ve(gr, 120, 130, 60, 14, 0.62f) };
        const int x = 136;
        lblMon = GD.Nhan("–", x, 20, 262, GD.Chu(10, true), 20);
        lblGia = GD.Nhan("–", x, 40, 262, GD.Chu(18, true), 32);
        lblGia.ForeColor = GD.Hex("#8b0000");
        lblDanDau = GD.Nhan("Dẫn đầu: –", x, 72, 262);
        lblDongHo = GD.Nhan("Còn lại: –", x, 90, 262, GD.ChuCoDinh(12, true), 22);
        ls = new ListBox { Location = new Point(x, 114), Size = new Size(262, 42), Font = GD.ChuCoDinh(8), IntegralHeight = false };
        GD.Them(g, anh, lblMon, lblGia, lblDanDau, lblDongHo, ls);
        var ts = Lop.ThamSo;
        var o = new (string Ma, string Chu, int Rong)[]
            { ("gia_khoi_diem", "Giá khởi điểm", 80), ("buoc_gia", "Bước giá", 80), ("thoi_gian_s", "Phiên (s, 0 = ∞)", 50), ("gia_han_s", "Gia hạn (s)", 50) };
        for (int i = 0; i < o.Length; i++)
        {
            int cx = 8 + (i % 2) * 196, cy = 166 + (i / 2) * 26;
            var v = ts[o[i].Ma];
            var t = GD.O(v is double d ? Chung.G(d) : Chung.Repr(v), cx + 104, cy - 3, o[i].Rong);
            oTs[o[i].Ma] = t;
            GD.Them(g, GD.Nhan(o[i].Chu + ":", cx, cy), t);
        }
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["gia_khoi_diem"] = long.Parse(oTs["gia_khoi_diem"].Text, ic), ["buoc_gia"] = long.Parse(oTs["buoc_gia"].Text, ic),
                ["thoi_gian_s"] = double.Parse(oTs["thoi_gian_s"].Text, ic), ["gia_han_s"] = double.Parse(oTs["gia_han_s"].Text, ic),
            };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("tham số phải là số"); }
    }

    /// <summary>Điền ô tham số (kịch bản demo) – như người dùng gõ.</summary>
    public void DatThamSo(params (string Ma, object GiaTri)[] ts)
    {
        foreach (var (k, v) in ts) oTs[k].Text = Chung.Repr(v);
    }

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        var mon = tt["mon"] as JsonObject;
        lblMon.Text = $"{Chung.Chuoi(mon, "ten")} (bước {Chung.Tien(Chung.SoL(mon, "buoc_gia"))})";
        lblGia.Text = Chung.Tien(Chung.SoL(tt, "gia"));
        string ai = Chung.Chuoi(tt, "nguoi_dan_dau") ?? "–";
        long soLan = Chung.SoL(tt, "so_lan");
        if (Chung.Dung(tt, "ket_thuc"))
        {
            (lblDanDau.Text, lblDanDau.ForeColor) = ($"THẮNG: {ai}  ({soLan} lượt)", GD.DoDam);
            (lblDongHo.Text, lblDongHo.ForeColor) = ("Kết thúc phiên", GD.Hex("#606060"));
        }
        else
        {
            (lblDanDau.Text, lblDanDau.ForeColor) = ($"Dẫn đầu: {ai}  ({soLan} lượt)", Color.Black);
            double? con = Chung.SoD(tt["con_lai_ms"]);
            lblDongHo.Text = con is { } c ? $"Còn lại: {BinhGom.DongHo(c / 1000)}" : "Còn lại: ∞ (không giới hạn)";
            lblDongHo.ForeColor = con is null or >= 10_000 ? GD.XanhDam : GD.DoDam;
        }
        var dong = (tt["lich_su"] as JsonArray ?? []).OfType<JsonObject>().Take(3)
            .Select(x => $"{Chung.Chuoi(x, "nguoi"),-6}{Chung.Tien(Chung.SoL(x, "gia")),14}" + (Chung.Dung(x, "gia_han") ? " GH" : "")).ToList();
        if (!ls.Items.Cast<string>().SequenceEqual(dong))
        {
            ls.Items.Clear();
            foreach (var d in dong) ls.Items.Add(d);
        }
    }
}
