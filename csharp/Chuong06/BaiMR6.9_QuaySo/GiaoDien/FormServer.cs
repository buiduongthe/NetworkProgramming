using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Bảng điều khiển máy chủ quay số (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": 10 món quà (ảnh, còn / đã trao cho ai),
// số quà còn lại, số lượt đã quay, số lượt trúng theo sổ kiểm toán, tham số đặt lại (số quà, xác suất p).
sealed class FormServerQuaySo(string host = "127.0.0.1", int cong = 42732)
    : FormServerCoSo(bao => new MayChuQuaySo("khoa", 2.0, bao), host, cong, 10000, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ quay số MR6.9";
    protected override string TenTaiNguyen => "Tài nguyên: kho quà";
    protected override Size KichThuocTaiNguyen => new(296, 186);

    Label lblCon = null!, lblLuot = null!;
    readonly List<(OVe O, Label Nhan)> oQua = [];
    readonly Dictionary<string, TextBox> oTs = [];
    JsonArray? qua;                                      // danh sách quà của cap_nhat mới nhất (để vẽ)
    string veCu = "";

    protected override void VeTaiNguyen(GroupBox g)
    {
        lblCon = GD.Nhan("–", 8, 20, 290, GD.Chu(14, true), 26);
        lblCon.ForeColor = GD.Hex("#8b0000");
        lblLuot = GD.Nhan("–", 8, 47, 290, GD.Chu(9), 18);
        GD.Them(g, lblCon, lblLuot);
        for (int i = 0; i < Qua.TenQua.Length; i++)
        {
            int x = 8 + i % 5 * 58, y = 66 + i / 5 * 56, so = i;
            var o = new OVe(x + 4, y, 46, 36) { BackColor = Color.White };
            o.Ve = gr => VeO(gr, so);
            var nh = GD.Nhan("", x - 2, y + 37, 58, GD.Chu(7), 15);
            nh.TextAlign = ContentAlignment.TopCenter;
            oQua.Add((o, nh));
            GD.Them(g, o, nh);
        }
        var ts = Lop.ThamSo;
        oTs["so_qua"] = GD.O(Chung.Repr(ts["so_qua"]), 58, 180, 36);
        oTs["p"] = GD.O(Chung.G((double)ts["p"]), 182, 180, 64);
        GD.Them(g, GD.Nhan("Số quà:", 8, 183), oTs["so_qua"], GD.Nhan("Xác suất p:", 108, 183), oTs["p"]);
    }

    /// <summary>Ô quà i: ảnh (xám nếu đã trao), viền xanh (còn) / xám (đã trao); ô thừa (chương trình ít quà hơn) để trống.</summary>
    void VeO(Graphics gr, int i)
    {
        if (qua == null) return;
        var o = oQua[i].O;
        if (i >= qua.Count)
        {
            using var b = new SolidBrush(GD.Hex("#d0d0d0"));
            gr.FillRectangle(b, 0, 0, o.Width, o.Height);
            return;
        }
        bool con = Chung.Dung(qua[i] as JsonObject, "con");
        if (!con)
            using (var b = new SolidBrush(GD.Hex("#e4e4e4"))) gr.FillRectangle(b, 0, 0, o.Width, o.Height);
        AnhQua.Ve(gr, i, o.Width / 2f, o.Height / 2f, 14, xam: !con);
        using var p = new Pen(con ? GD.XanhDam : GD.Hex("#b0b0b0"), 2);
        gr.DrawRectangle(p, 1, 1, o.Width - 2, o.Height - 2);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["so_qua"] = long.Parse(oTs["so_qua"].Text, ic), ["p"] = double.Parse(oTs["p"].Text, NumberStyles.Float, ic),
            };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("tham số phải là số"); }
    }

    /// <summary>Điền ô tham số (kịch bản demo) – như người dùng gõ.</summary>
    public void DatThamSo(params (string Ma, object GiaTri)[] ts)
    {
        foreach (var (k, v) in ts) oTs[k].Text = v is double d ? Chung.G(d) : Chung.Repr(v);
    }

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        long soQua = Chung.SoL(tt, "so_qua"), soTrung = Chung.SoL(tt, "so_trung");
        lblCon.Text = $"Còn {Chung.SoL(tt, "con_lai")}/{soQua} quà · trao {soTrung}";
        lblCon.ForeColor = soTrung > soQua ? GD.DoDam : GD.Hex("#8b0000");
        lblLuot.Text = $"Đã quay {Chung.SoL(tt, "da_quay").ToString("#,0", Chung.Ic).Replace(",", ".")} lượt · p = {Qua.ChuP(Chung.SoD(tt, "p"))}";
        var ds = tt["qua"] as JsonArray ?? [];
        string khoa = string.Join("|", ds.OfType<JsonObject>().Select(q => $"{Chung.SoL(q, "ma")}:{Chung.Dung(q, "con")}"));
        if (khoa != veCu)
        {
            veCu = khoa;
            qua = ds;
            foreach (var (o, _) in oQua) o.Invalidate();
        }
        for (int i = 0; i < oQua.Count; i++)
        {
            var nh = oQua[i].Nhan;
            if (i < ds.Count && ds[i] is JsonObject q)
            {
                bool con = Chung.Dung(q, "con");
                nh.Text = con ? Chung.Chuoi(q, "ten") : $"→{Chung.Chuoi(q, "nguoi") ?? "?"}";
                nh.ForeColor = con ? Color.Black : GD.Hex("#a00000");
            }
            else nh.Text = "";
        }
    }
}
