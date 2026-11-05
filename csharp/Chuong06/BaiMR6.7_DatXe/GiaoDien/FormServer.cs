using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_7;

// Bài mở rộng MR6.7 – Bảng điều khiển máy chủ đặt xe (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": bản đồ thu nhỏ với các xe (xe đã có khách
// mờ đi, ghi tên khách), số xe trống, danh sách xe và tham số đặt lại (số xe).
sealed class FormServerDatXe(string host = "127.0.0.1", int cong = 42712)
    : FormServerCoSo(bao => new MayChuDatXe("khoa", 2.0, bao, new OrderedDictionary<string, object> { ["so_xe"] = 1L }),
                     host, cong, 500, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ đặt xe MR6.7";
    protected override string TenTaiNguyen => "Tài nguyên: các xe (mỗi xe 1 chỗ)";
    protected override Size KichThuocTaiNguyen => new(404, 180);

    BanDo banDo = null!;
    Label lblTrong = null!;
    ListBox ls = null!;
    TextBox txtSoXe = null!;
    string veCu = "";
    readonly List<(string Chu, bool CoKhach)> dongLs = [];

    protected override void VeTaiNguyen(GroupBox g)
    {
        banDo = new BanDo(8, 20, 200, 150, nho: true);
        lblTrong = GD.Nhan("–", 216, 20, 196, GD.Chu(16, true), 30);
        ls = new ListBox
        {
            Location = new Point(216, 54), Size = new Size(196, 116), Font = GD.ChuCoDinh(8), DrawMode = DrawMode.OwnerDrawFixed,
            IntegralHeight = false,
        };
        ls.DrawItem += (_, e) =>                         // xe đã có khách: chữ đỏ
        {
            if (e.Index < 0 || e.Index >= dongLs.Count) return;
            e.DrawBackground();
            var (chu, coKhach) = dongLs[e.Index];
            TextRenderer.DrawText(e.Graphics, chu, ls.Font, e.Bounds, coKhach ? GD.Hex("#a00000") : Color.Black, TextFormatFlags.Left);
        };
        txtSoXe = GD.O(Chung.Repr(Lop.ThamSo["so_xe"]), 104, 177, 50);
        GD.Them(g, banDo, lblTrong, ls, GD.Nhan("Số xe (1–50):", 8, 180), txtSoXe);
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        if (!long.TryParse(txtSoXe.Text, NumberStyles.Integer, CultureInfo.InvariantCulture, out long so))
            throw new ArgumentException("số xe phải là số nguyên");
        return new OrderedDictionary<string, object> { ["so_xe"] = so };
    }

    /// <summary>Điền ô tham số (kịch bản demo) – như người dùng gõ.</summary>
    public void DatSoXe(int so) => txtSoXe.Text = so.ToString(CultureInfo.InvariantCulture);

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        long trong = Chung.SoL(tt, "trong");
        (lblTrong.Text, lblTrong.ForeColor) = ($"Trống {trong}/{Chung.SoL(tt, "so_xe")} xe", trong > 0 ? GD.XanhDam : GD.DoDam);
        var xe = (tt["xe"] as JsonArray ?? []).OfType<JsonObject>().ToList();
        string khoaVe = string.Join(";", xe.Select(x => $"{Chung.SoL(x, "ma")}:{Chung.Chuoi(x, "khach")}"));
        if (khoaVe == veCu) return;
        veCu = khoaVe;
        banDo.Xoa();
        dongLs.Clear();
        foreach (var x in xe)
        {
            var p = BanDoPho.DocDiem(x["vi_tri"]);
            string? khach = Chung.Chuoi(x, "khach");
            banDo.Xe(p.X, p.Y, Chung.Chuoi(x, "mau") ?? "", khach ?? Chung.Chuoi(x, "tai_xe") ?? "", mo: khach != null);
            dongLs.Add(($"{Chung.SoL(x, "ma"),2} {Chung.Chuoi(x, "tai_xe"),-6}{Chung.Chuoi(x, "bien_so"),-11}{khach ?? "·"}", khach != null));
        }
        ls.BeginUpdate();
        ls.Items.Clear();
        foreach (var d in dongLs) ls.Items.Add(d.Chu);
        ls.EndUpdate();
    }
}
