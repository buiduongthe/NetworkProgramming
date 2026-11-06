using System.Drawing;
using System.Globalization;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_14;

// Bài mở rộng MR6.14 – Bảng điều khiển máy chủ đặt vé xe khách theo chặng (WinForms).
//
// Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký)
// ở GiaoDienChung.cs (FormServerCoSo); tệp này chỉ vẽ khung "Tài nguyên": sơ đồ xe thu nhỏ (mỗi chỗ có dải vạch theo
// 3 chặng, ghế nóng viền đỏ), số chỗ-chặng mỗi loại, số chỗ đã bán theo chặng và tham số đặt lại (thời hạn giữ chỗ,
// loại xe giuong/ghe, ghế nóng của bộ mô phỏng).
sealed class FormServerDatVeXe(string host = "127.0.0.1", int cong = 42782)
    : FormServerCoSo(bao => new MayChuDatVeXe("khoa", 2.0, bao), host, cong, 500, 200, MoPhong.SoLanMacDinh)
{
    protected override string TieuDe => "Máy chủ đặt vé xe khách MR6.14";
    protected override string TenTaiNguyen => "Tài nguyên: chỗ × chặng (Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang)";
    protected override Size KichThuocTaiNguyen => new(420, 250);

    SoDoXe? soDo;
    JsonObject? ttVe;
    OVe cv = null!;
    readonly Dictionary<string, Label> lblDem = [];
    Label lblXe = null!, lblChang = null!, lblNong = null!;
    readonly Dictionary<string, TextBox> oTs = [];
    string veCu = "";
    static readonly (string Ma, string Chu)[] Loai = [("trong", "Trống"), ("giu", "Đang giữ"), ("ban", "Đã bán")];

    MayChuDatVeXe Sv => (MayChuDatVeXe)Lop;

    protected override void VeTaiNguyen(GroupBox g)
    {
        cv = new OVe(8, 20, 184, 212) { BackColor = XeGui.NenXe };
        cv.Ve = gr =>
        {
            if (soDo is not { } s || ttVe is not { } t) return;
            s.VeNen(gr);
            string nong = Chung.Chuoi(t, "ghe_nong") ?? "";
            s.Ve(gr, t, null, 0, XeKhach.SoChang, null, nong != "" ? new() { [nong] = GD.Hex("#ff3030") } : null);
        };
        lblXe = GD.Nhan(" ", 200, 20, 220, GD.Chu(9, true), 20);
        GD.Them(g, cv, lblXe);
        Color[] mau = [XeGui.Trong, XeGui.KhacGiu, XeGui.DaBan];
        for (int i = 0; i < Loai.Length; i++)
        {
            var o = new Label { Location = new Point(202, 46 + i * 24), Size = new Size(16, 16), BackColor = mau[i], BorderStyle = BorderStyle.FixedSingle };
            var l = GD.Nhan($" {Loai[i].Chu}: –", 220, 44 + i * 24, 190, GD.Chu(10, true), 22);
            lblDem[Loai[i].Ma] = l;
            GD.Them(g, o, l);
        }
        lblChang = GD.Nhan(" ", 202, 120, 214, GD.ChuCoDinh(8), 62);
        lblNong = GD.Nhan(" ", 202, 184, 214, GD.Chu(8), 48);
        GD.Them(g, lblChang, lblNong);
        var ts = Lop.ThamSo;
        int x = 8;
        foreach (var (ma, chu, rong) in new[] { ("ttl_s", "Giữ chỗ (s)", 40), ("loai_xe", "Loại xe", 56), ("ghe_nong", "Ghế nóng", 40) })
        {
            var nhan = GD.Nhan(chu + ":", x, 243);
            int w = TextRenderer.MeasureText(chu + ":", GD.Thuong).Width + 2;
            var v = ts[ma];
            var t = GD.O(v is double d ? Chung.G(d) : Chung.Repr(v), x + w, 240, rong);
            oTs[ma] = t;
            GD.Them(g, nhan, t);
            x += w + rong + 8;
        }
    }

    protected override IEnumerable<KeyValuePair<string, object>> ThamSoDatLai()
    {
        double ttl;
        try
        {
            ttl = double.Parse(oTs["ttl_s"].Text.Trim(), CultureInfo.InvariantCulture);
        }
        catch (Exception e) when (e is FormatException or OverflowException) { throw new ArgumentException("thời hạn giữ chỗ phải là số"); }
        string loai = oTs["loai_xe"].Text.Trim().ToLowerInvariant();
        string nong = oTs["ghe_nong"].Text.Trim().ToUpperInvariant();
        var ts = new OrderedDictionary<string, object> { ["ttl_s"] = ttl, ["loai_xe"] = loai };
        if (!(loai != Sv.TsS("loai_xe") && nong == Sv.GheNong()))   // đổi loại xe mà không sửa ghế nóng: máy chủ tự chọn chỗ đầu
            ts["ghe_nong"] = nong;
        return ts;
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
        lblXe.Text = $"{Chung.Chuoi(tt, "ten_xe")}, chuyến {Chung.Chuoi(tt, "chuyen")}";
        int soGhe = (tt["ghe"] as JsonArray)?.Count ?? 0;
        var sd = (tt["so_do"] as JsonArray ?? []).Select(Chung.ChuoiPy).ToList();
        var dong = new List<string>();
        for (int k = 0; k < XeKhach.SoChang; k++)
        {
            int ban = sd.Count(d => k < d.Length && d[k] == 'b');
            dong.Add($"{XeKhach.MaTram[k]}-{XeKhach.MaTram[k + 1]} {ban,2}/{soGhe}");
        }
        lblChang.Text = "Đã bán theo chặng:\n" + string.Join("\n", dong);
        string nong = Chung.Chuoi(tt, "ghe_nong") ?? "";
        if (nong == "") lblNong.Text = "Ghế nóng (mô phỏng): ngẫu nhiên";
        else
        {
            string d = XeGui.DongCua(tt, nong);
            var p = Enumerable.Range(0, XeKhach.SoChang).Where(k => d[k] != '.')
                              .Select(k => $"{XeKhach.MaTram[k]}-{XeKhach.MaTram[k + 1]}: {NguoiDatVeXe.ChuO(tt, $"{nong}#{k}")}").ToList();
            lblNong.Text = $"Ghế nóng (mô phỏng): {nong}\n{(p.Count == 0 ? "trống cả tuyến" : string.Join(", ", p))}";
        }
        string loai = Chung.Chuoi(tt, "loai_xe") ?? "giuong";
        if (soDo == null || soDo.LoaiXe != loai)
        {
            soDo = new SoDoXe(loai, 0.5f, false);
            veCu = "";
            DatThamSo(("loai_xe", loai), ("ghe_nong", nong));   // đổi loại xe: ô ghế nóng theo máy chủ
        }
        ttVe = tt;
        string khoa = string.Join("|", sd) + nong;
        if (khoa == veCu) return;
        veCu = khoa;
        cv.Invalidate();
    }
}
