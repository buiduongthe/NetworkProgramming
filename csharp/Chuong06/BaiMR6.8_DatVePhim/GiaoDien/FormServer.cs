using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_8;

// Bài mở rộng MR6.8 – Bảng điều khiển máy chủ đặt vé xem phim (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": sơ đồ phòng chiếu thu nhỏ (trống / đang
// giữ / đã bán, ghế nóng viền đỏ), số ghế mỗi loại và tham số đặt lại (thời hạn giữ chỗ, ghế nóng của bộ mô phỏng,
// giá vé).
sealed class FormServerDatVe(string host = "127.0.0.1", int cong = 42722)
    : FormServerCoSo(bao => new MayChuDatVe("khoa", 2.0, bao), host, cong, 500, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ đặt vé xem phim MR6.8";
    protected override string TenTaiNguyen => "Tài nguyên: phòng chiếu 8 × 12 ghế";
    protected override Size KichThuocTaiNguyen => new(404, 160);

    SoDoGhe soDo = null!;
    readonly Dictionary<string, Label> lblDem = [];
    Label lblNong = null!;
    readonly Dictionary<string, TextBox> oTs = [];
    string veCu = "";
    static readonly (string Ma, string Chu)[] Loai = [("trong", "Trống"), ("giu", "Đang giữ"), ("ban", "Đã bán")];

    protected override void VeTaiNguyen(GroupBox g)
    {
        soDo = new SoDoGhe(8, 20, 206, 124, PhongChieu.Hang, PhongChieu.Cot, o: 13, khe: 2, le: 6, tren: 16, chu: false);
        GD.Them(g, soDo);
        Color[] mau = [SoDoGhe.Trong, SoDoGhe.KhacGiu, SoDoGhe.DaBan];
        for (int i = 0; i < Loai.Length; i++)
        {
            var o = new Label { Location = new Point(224, 24 + i * 26), Size = new Size(16, 16), BackColor = mau[i], BorderStyle = BorderStyle.FixedSingle };
            var l = GD.Nhan($" {Loai[i].Chu}: –", 242, 22 + i * 26, 170, GD.Chu(10, true), 22);
            lblDem[Loai[i].Ma] = l;
            GD.Them(g, o, l);
        }
        lblNong = GD.Nhan("", 224, 102, 188, GD.Chu(8), 36);
        GD.Them(g, lblNong);
        var ts = Lop.ThamSo;
        int x = 8;
        foreach (var (ma, chu, rong) in new[] { ("ttl_s", "Giữ chỗ (s)", 44), ("ghe_nong", "Ghế nóng", 40), ("gia_ve", "Giá vé", 64) })
        {
            var nhan = GD.Nhan(chu + ":", x, 156);
            int w = TextRenderer.MeasureText(chu + ":", GD.Thuong).Width + 2;
            var v = ts[ma];
            var t = GD.O(v is double d ? Chung.G(d) : Chung.Repr(v), x + w, 153, rong);
            oTs[ma] = t;
            GD.Them(g, nhan, t);
            x += w + rong + 8;
        }
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        try
        {
            var ic = CultureInfo.InvariantCulture;
            return new OrderedDictionary<string, object>
            {
                ["ttl_s"] = double.Parse(oTs["ttl_s"].Text, ic), ["ghe_nong"] = oTs["ghe_nong"].Text.Trim().ToUpperInvariant(),
                ["gia_ve"] = long.Parse(oTs["gia_ve"].Text, ic),
            };
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("thời hạn và giá vé phải là số"); }
    }

    /// <summary>Điền ô tham số (kịch bản demo) – như người dùng gõ.</summary>
    public void DatThamSo(params (string Ma, object GiaTri)[] ts)
    {
        foreach (var (k, v) in ts) oTs[k].Text = Chung.Repr(v);
    }

    protected override void CapNhatTaiNguyen(JsonObject tt, JsonObject tk)
    {
        var dem = tt["dem"] as JsonObject;
        foreach (var (ma, chu) in Loai) lblDem[ma].Text = $" {chu}: {Chung.SoL(dem, ma)}";
        string nong = Chung.Chuoi(tt, "ghe_nong") ?? "";
        var chuGhe = tt["chu"] as JsonObject;
        lblNong.Text = nong != "" ? $"Ghế nóng (mô phỏng): {nong}\n{nong}: {Chung.Chuoi(chuGhe, nong) ?? "trống"}" : "Ghế nóng (mô phỏng): ngẫu nhiên";
        var dong = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        string khoaVe = string.Join("|", dong) + nong;
        if (khoaVe == veCu) return;
        veCu = khoaVe;
        string hang = Chung.Chuoi(tt, "hang") ?? "";
        var mau = new Dictionary<string, Color>();
        for (int i = 0; i < Math.Min(hang.Length, dong.Count); i++)
            for (int c = 0; c < dong[i].Length; c++)
                if (dong[i][c] != '.') mau[$"{hang[i]}{c + 1}"] = dong[i][c] == 'g' ? SoDoGhe.KhacGiu : SoDoGhe.DaBan;
        soDo.Ve(mau, nong != "" ? new() { [nong] = GD.Hex("#ff3030") } : null);
    }
}
