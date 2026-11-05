using System.Drawing;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_10;

// Bài mở rộng MR6.10 – Cổng đăng ký học phần (ứng dụng người dùng, WinForms).
//
// Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở GiaoDienChung.cs (FormNguoiDungCoSo); tệp này vẽ
// trang đăng ký như cổng đào tạo của trường: thanh tiêu đề, bảng lớp học phần (mã lớp, tên, tín chỉ, giảng viên,
// lịch học, thanh sĩ số "đã ĐK / sĩ số", nút Đăng ký / Hủy / Đã đủ trên từng hàng), danh sách lớp đã đăng ký.
// Bảng cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây). Đối chiếu kết quả máy chủ trả lời với danh sách lớp thật:
// máy chủ báo "thành công" mà danh sách lớp không có tên mình (mất cập nhật ở chế độ khong_khoa) -> cảnh báo
// "Đăng ký … đã bị MẤT".
sealed class FormDangKy(string ten, string host, int cong) : FormNguoiDungCoSo(ten, host, cong)
{
    static readonly Color XanhDh = GD.Hex("#0b3d91");    // màu thanh tiêu đề cổng đào tạo
    static readonly (string Chu, int Rong)[] Cot =
        [("Mã lớp HP", 76), ("Tên học phần", 108), ("TC", 26), ("Giảng viên", 118), ("Lịch học", 118), ("Đã ĐK / Sĩ số", 126), ("", 72)];
    protected override string TieuDe => "Cổng đăng ký học phần";
    protected override Size KichThuocKhung => new(668, 404);

    /// <summary>Các điều khiển của một hàng lớp.</summary>
    public sealed class HangLop
    {
        public Label[] O = [];
        public Color Nen;
        public OVe Thanh = null!;
        public Label So = null!;
        public Button Nut = null!;
        public double TiLe;
        public Color MauThanh;
    }

    public readonly Dictionary<string, string> CuaToi = [];   // mã lớp -> giờ đăng ký (theo kết quả máy chủ)
    readonly Dictionary<string, double> thieuTu = [];          // mã lớp -> lúc thấy danh sách lớp thiếu tên mình
    readonly HashSet<string> choCapNhat = [];                  // mã lớp vừa có kq, chưa có cap_nhat nào tới sau
    public readonly HashSet<string> DangCho = [];               // mã lớp đang chờ trả lời
    public readonly Dictionary<string, HangLop> Hang = [];
    Label lblSv = null!, lblTc = null!;
    public Label LblThongBao = null!;
    Panel bang = null!;
    ListBox ls = null!;

    protected override void Ve(Panel khung)
    {
        var dau = new Panel { Location = new Point(0, 4), Size = new Size(668, 34), BackColor = XanhDh };
        var t1 = new Label { Text = "CỔNG ĐĂNG KÝ HỌC PHẦN", Location = new Point(8, 5), AutoSize = true, Font = GD.Chu(14, true), ForeColor = Color.White };
        var t2 = new Label
        {
            Text = "Học kỳ 1 · Năm học 2026–2027", Location = new Point(420, 9), Size = new Size(240, 18), Font = GD.Chu(10, true),
            ForeColor = GD.Hex("#ffd54f"), TextAlign = ContentAlignment.MiddleRight,
        };
        dau.Controls.AddRange([t1, t2]);
        var tt = new Panel { Location = new Point(0, 38), Size = new Size(668, 26), BackColor = GD.Hex("#e8eef8") };
        lblSv = new Label { Text = "Sinh viên: –", Location = new Point(8, 4), Size = new Size(300, 18), Font = GD.Chu(10, true) };
        lblTc = new Label { Text = "", Location = new Point(320, 4), Size = new Size(340, 18), Font = GD.Chu(10), TextAlign = ContentAlignment.MiddleRight };
        tt.Controls.AddRange([lblSv, lblTc]);
        LblThongBao = new Label
        {
            Text = "Chưa kết nối", Location = new Point(0, 68), Size = new Size(668, 28), Font = GD.Chu(11, true), BackColor = GD.Hex("#e0e0e0"),
            TextAlign = ContentAlignment.MiddleCenter,
        };
        var g1 = GD.Nhom("Danh sách lớp học phần mở đăng ký", 0, 100, 668, 196);
        bang = new Panel { Location = new Point(6, 18), Size = new Size(656, 172), BackColor = GD.Hex("#9aa9c4") };
        int x = 0;
        foreach (var (chu, rong) in Cot)
        {
            bang.Controls.Add(new Label
            {
                Text = chu, Location = new Point(x, 0), Size = new Size(rong, 22), BackColor = GD.Hex("#34558b"), ForeColor = Color.White,
                Font = GD.Chu(9, true), TextAlign = ContentAlignment.MiddleLeft,
            });
            x += rong + 1;
        }
        g1.Controls.Add(bang);
        var g2 = GD.Nhom("Kết quả đăng ký của bạn", 0, 300, 668, 100);
        ls = new ListBox { Location = new Point(8, 18), Size = new Size(652, 76), Font = GD.ChuCoDinh(9), IntegralHeight = false, SelectionMode = SelectionMode.None };
        GD.Them(g2, ls);
        GD.Them(khung, dau, tt, LblThongBao, g1, g2);
    }

    // ------------------------------------------------------------ bảng lớp
    void TaoHang(int i, string ma)
    {
        var nen = i % 2 == 1 ? Color.White : GD.Hex("#f3f6fb");
        int y = 23 + i * 25, x = 0;
        var h = new HangLop { Nen = nen };
        var o = new List<Label>();
        for (int c = 0; c < 5; c++)
        {
            var nh = new Label { Location = new Point(x, y), Size = new Size(Cot[c].Rong, 24), BackColor = nen, Font = GD.Chu(9), TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true };
            o.Add(nh);
            bang.Controls.Add(nh);
            x += Cot[c].Rong + 1;
        }
        h.O = [.. o];
        var nenDk = new Label { Location = new Point(x, y), Size = new Size(Cot[5].Rong, 24), BackColor = nen };
        h.Thanh = new OVe(x + 3, y + 6, 70, 12) { BackColor = GD.Hex("#dddddd") };
        h.Thanh.Ve = gr =>
        {
            using var b = new SolidBrush(h.MauThanh);
            gr.FillRectangle(b, 0, 0, (float)(70 * h.TiLe), 12);
        };
        h.So = new Label { Location = new Point(x + 76, y + 3), Size = new Size(Cot[5].Rong - 78, 18), BackColor = nen, Font = GD.Chu(9, true) };
        x += Cot[5].Rong + 1;
        h.Nut = new Button
        {
            Text = "Đăng ký", Location = new Point(x, y), Size = new Size(Cot[6].Rong, 24), BackColor = GD.Hex("#7fb3ff"), FlatStyle = FlatStyle.Flat,
            UseVisualStyleBackColor = false, Font = GD.Chu(9),
        };
        h.Nut.Click += (_, _) => Bam(ma);
        bang.Controls.AddRange([h.Thanh, h.So, h.Nut]);
        bang.Controls.Add(nenDk);
        nenDk.SendToBack();
        Hang[ma] = h;
        h.O = [.. h.O, nenDk];                           // ô nền cột "Đã ĐK" cũng đổi màu theo hàng
    }

    /// <summary>Bấm nút của hàng: Đăng ký (chưa có) hoặc Hủy (đã có).</summary>
    void Bam(string ma)
    {
        if (Kn == null || DangCho.Contains(ma)) return;
        string loai = CuaToi.ContainsKey(ma) ? "huy" : "dang_ky";
        DangCho.Add(ma);
        (Hang[ma].Nut.Text, Hang[ma].Nut.Enabled) = ("…", false);
        NhatKy.Ghi((loai == "huy" ? "Hủy " : "Đăng ký ") + ma);
        GuiYc(loai, new JsonObject { ["ma_lop"] = ma });
    }

    /// <summary>Kịch bản demo: bấm nút của hàng lớp ma.</summary>
    public void DangKy(string ma) => Hang[ma].Nut.PerformClick();

    static string Gio() => DateTime.Now.ToString("HH:mm:ss");

    protected override void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt)
    {
        string ma = Chung.Chuoi(truong, "ma_lop")!;
        DangCho.Remove(ma);
        choCapNhat.Add(ma);                                    // Tt đang giữ có thể chụp trước lần ghi này
        if (Chung.Dung(tin, "ok"))
        {
            if (loai == "dang_ky")
            {
                CuaToi[ma] = Gio();
                Bao($"Đăng ký thành công lớp {ma}", GD.Hex("#70d070"), Color.Black);
            }
            else
            {
                CuaToi.Remove(ma);
                Bao($"Đã hủy đăng ký lớp {ma}", GD.Hex("#d0d0ff"), Color.Black);
            }
            NhatKy.Ghi($"  thành công ({Chung.SoL(tin, "da_dk")}/{Chung.SoL(Tt, "si_so")}, {Chung.F(rtt, "F1")} ms)");
        }
        else
        {
            string? m = Chung.Chuoi(tin, "ma_loi");
            string text = m switch
            {
                "lop_day" => $"Lớp {ma} đã đủ sĩ số",
                "da_dang_ky" => $"Bạn đã đăng ký lớp {ma} rồi",
                "xung_dot" => "Hệ thống bận, vui lòng thử lại",
                _ => Chung.Chuoi(tin, "ly_do") ?? "",
            };
            if (m == "da_dang_ky") CuaToi.TryAdd(ma, Gio());
            Bao(text, GD.Hex(m != "lop_day" ? "#ffb347" : "#ff6060"), m != "lop_day" ? Color.Black : Color.White);
            NhatKy.Ghi($"  từ chối: {Chung.ChuoiPy(tin["ly_do"])}");
        }
        if (Tt != null) CapNhatBang(Tt, kiemMat: false);
    }

    void Bao(string text, Color nen, Color chu) => (LblThongBao.Text, LblThongBao.BackColor, LblThongBao.ForeColor) = (text, nen, chu);

    // ------------------------------------------------------------ nhận trạng thái
    protected override void CapNhat(JsonObject tt)
    {
        choCapNhat.Clear();                                    // cap_nhat mới tới SAU các kq đã nhận
        CapNhatBang(tt, kiemMat: true);
    }

    static List<JsonObject> DsLop(JsonObject tt) => [.. (tt["lop"] as JsonArray ?? []).OfType<JsonObject>()];

    static bool CoTen(JsonObject lop, string ten) => (lop["sv"] as JsonArray ?? []).Any(s => Chung.ChuoiPy(s) == ten);

    void CapNhatBang(JsonObject tt, bool kiemMat)
    {
        var lop = DsLop(tt);
        var maMoi = lop.Select(x => Chung.Chuoi(x, "ma")!).ToList();
        if (!Hang.Keys.SequenceEqual(maMoi))             // số lớp đổi (đặt lại): vẽ lại bảng
        {
            foreach (var h in Hang.Values)
            {
                Control[] cu = [.. h.O, h.Thanh, h.So, h.Nut];
                foreach (var c in cu)
                {
                    bang.Controls.Remove(c);
                    c.Dispose();
                }
            }
            Hang.Clear();
            for (int i = 0; i < maMoi.Count; i++) TaoHang(i, maMoi[i]);
        }
        if (kiemMat) KiemMat(tt);
        foreach (var x in lop)
        {
            string ma = Chung.Chuoi(x, "ma")!;
            var h = Hang[ma];
            bool cuaToi = CuaToi.ContainsKey(ma);
            var nen = cuaToi ? GD.Hex("#d8f5d8") : h.Nen;
            string[] gt = [ma, Chung.Chuoi(x, "ten") ?? "", $"{Chung.SoL(x, "tc")}", Chung.Chuoi(x, "gv") ?? "", Chung.Chuoi(x, "lich") ?? ""];
            for (int c = 0; c < h.O.Length; c++)
            {
                h.O[c].BackColor = nen;
                if (c < gt.Length)
                {
                    h.O[c].Text = gt[c];
                    h.O[c].Font = GD.Chu(9, cuaToi);
                }
            }
            h.So.BackColor = nen;
            long daDk = Chung.SoL(x, "da_dk"), siSo = Math.Max(1, Chung.SoL(x, "si_so"));
            bool day = daDk >= siSo;
            h.TiLe = Math.Min(1.0, (double)daDk / siSo);
            h.MauThanh = day ? GD.DoDam : h.TiLe >= 0.8 ? GD.Hex("#f0a020") : GD.Hex("#3cb44b");
            h.Thanh.Invalidate();
            (h.So.Text, h.So.ForeColor) = ($"{daDk}/{Chung.SoL(x, "si_so")}", day ? GD.DoDam : Color.Black);
            if (DangCho.Contains(ma)) continue;
            var (chu, mau) = cuaToi ? ("Hủy", "#f08080") : day ? ("Đã đủ", "#c8c8c8") : ("Đăng ký", "#7fb3ff");
            (h.Nut.Text, h.Nut.BackColor, h.Nut.Enabled) = (chu, GD.Hex(mau), true);
        }
        var tc = lop.ToDictionary(x => Chung.Chuoi(x, "ma")!, x => Chung.SoL(x, "tc"));
        var tenLop = lop.ToDictionary(x => Chung.Chuoi(x, "ma")!, x => Chung.Chuoi(x, "ten") ?? "?");
        long soTc = CuaToi.Keys.Sum(m => tc.GetValueOrDefault(m));
        lblSv.Text = $"Sinh viên: {Ten}";
        lblTc.Text = $"Đã đăng ký {CuaToi.Count} lớp · {soTc} tín chỉ";
        var dong = CuaToi.OrderBy(x => x.Key, StringComparer.Ordinal)
                         .Select(x => $"{x.Key}  {tenLop.GetValueOrDefault(x.Key, "?"),-16} {tc.GetValueOrDefault(x.Key)} TC  lúc {x.Value}").ToList();
        if (!ls.Items.Cast<string>().SequenceEqual(dong))
        {
            ls.Items.Clear();
            foreach (var d in dong) ls.Items.Add(d);
        }
        if (LblThongBao.Text == "Chưa kết nối") Bao("Đợt đăng ký đang mở – chọn lớp và bấm Đăng ký", GD.Hex("#e0e0e0"), Color.Black);
    }

    /// <summary>Đối chiếu kết quả mình nhận với danh sách lớp của máy chủ. cap_nhat có thể đến trễ hơn kq, nên chỉ báo
    /// khi danh sách thiếu tên mình liên tục 0,5 s, và bỏ qua lớp vừa có kq mà chưa có cap_nhat nào tới sau (bản Tt
    /// đang giữ chụp trước lần ghi của mình – ví dụ vừa hủy mà Tt cũ vẫn còn tên).</summary>
    void KiemMat(JsonObject tt)
    {
        foreach (var x in DsLop(tt))
        {
            string ma = Chung.Chuoi(x, "ma")!;
            bool coTen = CoTen(x, Ten);
            bool cho = DangCho.Contains(ma) || choCapNhat.Contains(ma);
            if (cho || coTen || !CuaToi.ContainsKey(ma)) thieuTu.Remove(ma);
            if (cho) continue;
            if (CuaToi.ContainsKey(ma) && !coTen)
            {
                double bayGio = Chung.Now;
                if (!thieuTu.TryGetValue(ma, out double tu)) thieuTu[ma] = tu = bayGio;
                if (bayGio - tu < 0.5) continue;
                thieuTu.Remove(ma);
                CuaToi.Remove(ma);                       // máy chủ báo thành công nhưng danh sách không có tên
                Bao($"Đăng ký {ma} đã bị MẤT (lỗi máy chủ)!", GD.Hex("#c00000"), Color.White);
                NhatKy.Ghi($"CẢNH BÁO: danh sách lớp {ma} không có tên bạn");
                CapNhatBang(tt, kiemMat: false);
            }
            else if (!CuaToi.ContainsKey(ma) && coTen) CuaToi[ma] = "–";   // (đăng ký từ thiết bị khác)
        }
    }

    protected override void MoiNhip()
    {
        if (Kn != null && Tt != null) KiemMat(Tt);
    }

    protected override void SuKien(string text)
    {
        base.SuKien(text);
        if (!text.StartsWith("Đặt lại")) return;         // đợt đăng ký mới: danh sách cũ không còn
        CuaToi.Clear();
        thieuTu.Clear();
        Bao("Đợt đăng ký mới – chọn lớp và bấm Đăng ký", GD.Hex("#e0e0e0"), Color.Black);
    }

    protected override string DongDinhKy()
    {
        string lop = string.Join(", ", DsLop(Tt!).Select(x => $"{Chung.Chuoi(x, "ma")} {Chung.SoL(x, "da_dk")}/{Chung.SoL(x, "si_so")}"));
        var cua = CuaToi.Keys.Order(StringComparer.Ordinal).ToList();
        return $"{lop}; bạn: {(cua.Count > 0 ? string.Join(", ", cua) : "chưa có lớp")}";
    }
}
