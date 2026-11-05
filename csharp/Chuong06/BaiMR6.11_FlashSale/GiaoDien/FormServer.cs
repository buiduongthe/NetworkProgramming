using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_11;

// Bài mở rộng MR6.11 – Bảng điều khiển máy chủ flash sale (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": ảnh sản phẩm, tồn kho cỡ lớn, thanh đã bán,
// số đã bán theo kho và theo sổ kiểm toán (đỏ khi lệch / bán vượt), đồng hồ mở bán, 3 đơn gần nhất, tham số đặt lại
// (tồn đầu kỳ, mở bán sau bao nhiêu giây).
sealed class FormServerFlashSale(string host = "127.0.0.1", int cong = 42752)
    : FormServerCoSo(bao => new MayChuFlashSale("khoa", 2.0, bao), host, cong, 10000, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ flash sale MR6.11";
    protected override string TenTaiNguyen => "Tài nguyên: kho hàng";
    protected override Size KichThuocTaiNguyen => new(330, 186);

    Label lblSp = null!, lblTon = null!, lblBan = null!, lblMo = null!;
    OVe thanhKho = null!;
    double tiLe;
    ListBox ls = null!;
    readonly Dictionary<string, TextBox> oTs = [];

    protected override void VeTaiNguyen(GroupBox g)
    {
        var anh = new OVe(8, 20, 120, 112) { Ve = gr => FormFlashSale.VeTaiNghe(gr, 120, 112, 60, 48, 0.55f) };
        lblSp = GD.Nhan("–", 136, 20, 194, GD.Chu(9, true), 18);
        lblTon = GD.Nhan("–", 136, 40, 194, GD.Chu(16, true), 30);
        lblTon.ForeColor = GD.Hex("#8b0000");
        thanhKho = new OVe(136, 72, 180, 14) { BackColor = GD.Hex("#dddddd") };
        thanhKho.Ve = gr =>
        {
            using var b = new SolidBrush(GD.Hex(tiLe >= 0.8 ? "#e4002b" : "#ff6a00"));
            gr.FillRectangle(b, 0, 0, (float)(180 * tiLe), 14);
        };
        lblBan = GD.Nhan("–", 136, 90, 194, GD.Chu(8), 18);
        lblMo = GD.Nhan("–", 136, 110, 194, GD.ChuCoDinh(11, true), 22);
        ls = new ListBox { Location = new Point(8, 136), Size = new Size(322, 42), Font = GD.ChuCoDinh(8), IntegralHeight = false, SelectionMode = SelectionMode.None };
        GD.Them(g, anh, lblSp, lblTon, thanhKho, lblBan, lblMo, ls);
        var ts = Lop.ThamSo;
        oTs["ton_dau"] = GD.O(Chung.Repr(ts["ton_dau"]), 76, 180, 50);
        oTs["mo_sau_s"] = GD.O(Chung.G((double)ts["mo_sau_s"]), 232, 180, 40);
        GD.Them(g, GD.Nhan("Tồn đầu kỳ:", 8, 183), oTs["ton_dau"], GD.Nhan("Mở bán sau (s):", 134, 183), oTs["mo_sau_s"]);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["ton_dau"] = long.Parse(oTs["ton_dau"].Text, ic), ["mo_sau_s"] = double.Parse(oTs["mo_sau_s"].Text, NumberStyles.Float, ic),
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
        long ton = Chung.SoL(tt, "ton"), tonDau = Math.Max(1, Chung.SoL(tt, "ton_dau")), daBan = Chung.SoL(tt, "da_ban"), banSo = Chung.SoL(tt, "ban_theo_so");
        lblSp.Text = Chung.Chuoi(tt["sp"] as JsonObject, "ten");
        lblTon.Text = $"Tồn {ton}/{Chung.SoL(tt, "ton_dau")}";
        double moi = Math.Min(1.0, (double)(tonDau - ton) / tonDau);
        if (moi != tiLe)
        {
            tiLe = moi;
            thanhKho.Invalidate();
        }
        bool vuot = banSo > tonDau;
        lblBan.Text = $"Đã bán {daBan} (sổ: {banSo}) · {Chung.SoL(tt, "so_don")} đơn";
        lblBan.ForeColor = vuot || banSo != daBan ? GD.DoDam : Color.Black;
        if (!Chung.Dung(tt, "mo_ban"))
        {
            long g = Chung.SoL(tt, "mo_con_ms") / 1000;
            (lblMo.Text, lblMo.ForeColor) = ($"Mở bán sau 00:{g / 60:00}:{g % 60:00}", GD.Hex("#a06000"));
        }
        else if (ton <= 0) (lblMo.Text, lblMo.ForeColor) = ("HẾT HÀNG", GD.Hex("#606060"));
        else (lblMo.Text, lblMo.ForeColor) = ("ĐANG MỞ BÁN", GD.XanhDam);
        var dong = (tt["don_gan"] as JsonArray ?? []).OfType<JsonObject>().Take(3)
            .Select(x => $"{Chung.Chuoi(x, "gio")} {Chung.Chuoi(x, "nguoi"),-6} ×{Chung.SoL(x, "so_luong")}").ToList();
        if (!ls.Items.Cast<string>().SequenceEqual(dong))
        {
            ls.Items.Clear();
            foreach (var d in dong) ls.Items.Add(d);
        }
    }
}
