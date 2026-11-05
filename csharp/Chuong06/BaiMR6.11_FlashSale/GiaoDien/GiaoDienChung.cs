using System.Collections.Concurrent;
using System.Drawing;
using System.Drawing.Imaging;
using System.Globalization;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace BaiMR6_11;

// Thành phần giao diện WinForms dùng chung cho nhóm bài tranh chấp MR6.6–MR6.13 (bài đầu: MR6.6 Đấu giá).
//
// Tệp này GIỐNG NHAU ở các bài (chỉ khác dòng namespace) – như giao_dien_chung.py của bản Python. Gồm:
//   - Tiện ích: GD (màu, nút, nhãn, ô nhập, nhóm, chụp và ghép ảnh), HangDoiGiaoDien, NhatKy, ThongKeDinhKy.
//   - FormServerCoSo: bảng điều khiển máy chủ đủ mọi phần chung – START/STOP, 4 nút chọn chế độ, độ trễ xử lý,
//     chỉ số trực tiếp (yc/s, thành công, từ chối, xung đột, hàng đợi, độ trễ, SAI PHẠM tô đỏ), khung mô phỏng tải
//     (Bắn đồng loạt, So sánh 4 chế độ + bảng), nhật ký. Lớp con chỉ vẽ khung "Tài nguyên":
//     VeTaiNguyen(nhóm), CapNhatTaiNguyen(tt, tk), ThamSoDatLai().
//   - FormNguoiDungCoSo: khung cửa sổ người dùng – kết nối/rời, nhật ký, thanh trạng thái, gửi yêu cầu; lớp con vẽ
//     phần nghiệp vụ: Ve(khung), CapNhat(tt), KetQua(...).
//
// Quy tắc: CHỈ luồng giao diện chạm vào điều khiển. Luồng mạng/mô phỏng đưa việc vào HangDoiGiaoDien (ConcurrentQueue
// + System.Windows.Forms.Timer 50 ms); số liệu của máy chủ đọc định kỳ bằng Timer 250 ms.
static class GD
{
    public static Color Hex(string s) => ColorTranslator.FromHtml(s);
    public static readonly Color Nen = Hex("#b0c4de");
    public static readonly Color XanhLa = Hex("#90ee90"), Do = Hex("#f08080"), XanhDuong = Hex("#add8e6"), Cam = Hex("#ffb347"),
                                 Vang = Hex("#ffd700");
    public static readonly Color DoDam = Hex("#c00000"), XanhDam = Hex("#006400");

    public static readonly Font Thuong = new(Control.DefaultFont, FontStyle.Regular);
    public static readonly Font Dam = new(Control.DefaultFont.FontFamily, 9, FontStyle.Bold);
    public static readonly Font CoDinh = new(FontFamily.GenericMonospace, 9);

    public static Font Chu(float co, bool dam = false) => new(Control.DefaultFont.FontFamily, co, dam ? FontStyle.Bold : FontStyle.Regular);
    public static Font ChuCoDinh(float co, bool dam = false) => new(FontFamily.GenericMonospace, co, dam ? FontStyle.Bold : FontStyle.Regular);

    public static Button Nut(string text, Color mau, int x, int y, int w = 110, int h = 28)
    {
        var b = new Button { Text = text, Location = new Point(x, y), Size = new Size(w, h), BackColor = mau, UseVisualStyleBackColor = false };
        b.FlatStyle = FlatStyle.Flat;
        b.FlatAppearance.BorderColor = Color.Gray;
        return b;
    }

    public static Label Nhan(string text, int x, int y, int w = 0, Font? font = null, int h = 0)
    {
        var l = w > 0 ? new Label { Text = text, Location = new Point(x, y), Size = new Size(w, h > 0 ? h : 18), AutoEllipsis = true }
                      : new Label { Text = text, Location = new Point(x, y), AutoSize = true };
        if (font != null) l.Font = font;
        return l;
    }

    public static TextBox O(string text, int x, int y, int w) => new() { Text = text, Location = new Point(x, y), Width = w };

    public static GroupBox Nhom(string text, int x, int y, int w, int h) => new()
    {
        Text = text, Location = new Point(x, y), Size = new Size(w, h), BackColor = Nen,
        Font = new Font(Control.DefaultFont, FontStyle.Bold),
    };

    /// <summary>Thêm điều khiển vào khung và trả font thường cho chúng (tiêu đề nhóm in đậm, nội dung không).</summary>
    public static void Them(Control cha, params Control[] ds)
    {
        foreach (var c in ds)
        {
            if (ReferenceEquals(c.Font, Control.DefaultFont)) c.Font = Thuong;   // chưa đặt font riêng: không thừa hưởng chữ đậm
            cha.Controls.Add(c);
        }
    }

    /// <summary>Độ trễ µs (JSON) -> "12.3" ms; null -> "–".</summary>
    public static string Ms(JsonNode? us) => Chung.SoD(us) is { } v ? Chung.F(v / 1000, "F1") : "–";

    /// <summary>Chụp một cửa sổ thành Bitmap (Control.DrawToBitmap).</summary>
    public static Bitmap Chup(Form f)
    {
        f.Refresh();
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        return bmp;
    }

    /// <summary>Ghép các ảnh theo hàng (mỗi hàng một danh sách) trên nền xám, viền 4; giữ tệp &lt; 1 MB.</summary>
    public static void Ghep(string duong, List<List<Bitmap>> hang)
    {
        const int vien = 4;
        int W = hang.Max(h => h.Sum(b => b.Width)) + 2 * vien, H = hang.Sum(h => h.Max(b => b.Height)) + 2 * vien;
        using (var tong = new Bitmap(W, H))
        {
            using (var g = Graphics.FromImage(tong))
            {
                g.Clear(Hex("#d0d0d0"));
                int y = vien;
                foreach (var h in hang)
                {
                    int x = vien;
                    foreach (var b in h)
                    {
                        g.DrawImage(b, x, y);
                        x += b.Width;
                    }
                    y += h.Max(b => b.Height);
                }
            }
            tong.Save(duong, ImageFormat.Png);
            for (int phan = 9; new FileInfo(duong).Length > 950_000 && phan >= 4; phan--)
            {
                using var nho = new Bitmap(tong, new Size(tong.Width * phan / 10, tong.Height * phan / 10));
                nho.Save(duong, ImageFormat.Png);
            }
        }
        foreach (var b in hang.SelectMany(h => h)) b.Dispose();
    }
}

/// <summary>Cầu nối luồng nền -> luồng giao diện: DuaVao() gọi từ bất kỳ luồng nào; bộ hẹn giờ 50 ms trên luồng giao
/// diện lấy ra và thực hiện.</summary>
sealed class HangDoiGiaoDien : IDisposable
{
    readonly ConcurrentQueue<Action> hang = new();
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 50 };
    public bool Song = true;

    public HangDoiGiaoDien()
    {
        nhip.Tick += (_, _) =>
        {
            while (hang.TryDequeue(out var viec))
            {
                try { viec(); }
                catch (ObjectDisposedException) { }                     // cửa sổ đã đóng
            }
        };
        nhip.Start();
    }

    public void DuaVao(Action viec) => hang.Enqueue(viec);

    public void Dispose()
    {
        Song = false;
        nhip.Dispose();
    }
}

/// <summary>Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. InRa = true (chạy riêng từng cửa sổ): mỗi dòng cũng in ra dòng
/// lệnh kèm tiền tố tên, để chuyển hướng được vào tệp nhật ký.</summary>
sealed class NhatKy : TextBox
{
    public static bool InRa;
    readonly string ten;

    public NhatKy(string ten, int x, int y, int w, int h)
    {
        this.ten = ten;
        (Multiline, ReadOnly, ScrollBars, WordWrap) = (true, true, ScrollBars.Vertical, true);
        (Location, Size, BackColor, Font) = (new Point(x, y), new Size(w, h), Color.White, GD.CoDinh);
    }

    public void Ghi(string text)
    {
        if (InRa) Console.WriteLine($"{DateTime.Now:HH:mm:ss} [{ten}] {text}");
        if (Lines.Length > 500) Text = string.Join(Environment.NewLine, Lines.Skip(Lines.Length - 400));
        AppendText($"{DateTime.Now:HH:mm:ss} {text}{Environment.NewLine}");
    }
}

/// <summary>Khi chạy riêng (NhatKy.InRa): cứ `chuKy` giây in một dòng số liệu ra dòng lệnh (không ghi vào ô nhật ký).</summary>
sealed class ThongKeDinhKy(string ten, double chuKy = 5.0)
{
    double han = Chung.Now + chuKy;

    public void InNeuDenHan(Func<string> taoDong)
    {
        double bayGio = Chung.Now;
        if (!NhatKy.InRa || bayGio < han) return;
        han = bayGio + chuKy;
        Console.WriteLine($"{DateTime.Now:HH:mm:ss} [{ten}] {taoDong()}");
    }
}

/// <summary>Ô vẽ tự do (double-buffered): lớp dùng gán Ve = (Graphics g) => … rồi gọi Invalidate().</summary>
class OVe : Control
{
    public Action<Graphics>? Ve;

    public OVe(int x, int y, int w, int h)
    {
        DoubleBuffered = true;
        SetStyle(ControlStyles.ResizeRedraw, true);
        (Location, Size) = (new Point(x, y), new Size(w, h));
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        e.Graphics.SmoothingMode = System.Drawing.Drawing2D.SmoothingMode.AntiAlias;
        Ve?.Invoke(e.Graphics);
    }
}

// ===================================================================== bảng điều khiển máy chủ
abstract class FormServerCoSo : Form
{
    protected virtual string TieuDe => "Máy chủ";
    protected virtual string TenTaiNguyen => "Tài nguyên";
    /// <summary>Kích thước phần nội dung của khung "Tài nguyên" (lớp con vẽ trong đó, gốc tọa độ (8, 20)).</summary>
    protected abstract Size KichThuocTaiNguyen { get; }

    protected readonly HangDoiGiaoDien Ui = new();
    public readonly MayChuCoSo Lop;
    public bool DangMoPhong;
    public List<HangSoSanh> HangSoSanh = [];
    public readonly TextBox TxtIp, TxtCong, TxtTre, TxtN, TxtK, TxtLan;
    public readonly Button BtnStart, BtnStop, BtnTre, BtnDatLai, BtnBan, BtnSoSanh;
    public readonly Dictionary<string, RadioButton> NutCheDo = [];
    public readonly Label LblMoPhong, LblKetLuan, LblSaiPham, LblSaiPhamCt, LblTrangThai;
    public readonly ListView Bang;
    public readonly NhatKy NhatKy;
    readonly Dictionary<string, Label> chiSo = [];
    readonly ThongKeDinhKy dinhKy = new("server");
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 250 };
    bool dongBoCheDo;                                    // đang đặt nút chọn theo máy chủ (không phải người bấm)

    protected FormServerCoSo(Func<Action<string>, MayChuCoSo> taoLop, string host, int cong, int n = 200, int k = 200, int lan = 1)
    {
        const int W = 1100;
        NhatKy = new NhatKy("server", 8, 20, W - 32, 100);
        Lop = taoLop(t => Ui.DuaVao(() => NhatKy.Ghi(t)));
        Text = $"{TieuDe} – bảng điều khiển";
        BackColor = GD.Nen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.Manual;

        // ---- hàng trên: kết nối | chế độ | chỉ số
        var g1 = GD.Nhom("Kết nối", 8, 4, 180, 196);
        TxtIp = GD.O(host, 50, 22, 120);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 50, 50, 60);
        BtnStart = GD.Nut("START SERVER", GD.XanhLa, 8, 84, 164, 30);
        BtnStop = GD.Nut("STOP SERVER", GD.Do, 8, 120, 164, 30);
        BtnStop.Enabled = false;
        GD.Them(g1, GD.Nhan("IP:", 8, 25), TxtIp, GD.Nhan("Cổng:", 8, 53), TxtCong, BtnStart, BtnStop);

        var g2 = GD.Nhom("Cách xử lý tranh chấp (đổi được khi đang chạy)", 194, 4, 400, 196);
        int y = 22;
        foreach (var cd in Chung.CheDo)
        {
            var r = new RadioButton
            {
                Text = $"{cd,-11} {Chung.TenCheDo[cd]}", Location = new Point(10, y), Size = new Size(380, 22), Font = GD.CoDinh,
                Checked = cd == Lop.Kho.CheDo, Tag = cd,
            };
            r.CheckedChanged += (_, _) =>
            {
                if (r.Checked && !dongBoCheDo) DoiCheDo((string)r.Tag!);
            };
            NutCheDo[cd] = r;
            g2.Controls.Add(r);
            y += 26;
        }
        TxtTre = GD.O(Chung.G(Lop.Kho.TreMs), 196, 137, 40);
        BtnTre = GD.Nut("Áp dụng", GD.XanhDuong, 270, 133, 80, 28);
        GD.Them(g2, GD.Nhan("Độ trễ xử lý: ngẫu nhiên 0 –", 10, 140), TxtTre, GD.Nhan("ms", 240, 140), BtnTre);

        var g3 = GD.Nhom("Chỉ số trực tiếp (từ lần đặt lại)", 600, 4, W - 608, 196);
        var dong = new (string Ma, string Chu)[]
        {
            ("ket_noi", "Kết nối / người"), ("yc_s", "Yêu cầu/giây"), ("so_yc", "Tổng yêu cầu"), ("thanh_cong", "Thành công"),
            ("tu_choi", "Từ chối"), ("xung_dot", "Xung đột / thử lại"), ("hang_doi", "Hàng đợi (lớn nhất)"), ("tre", "Xử lý p50/p95/p99"),
        };
        for (int i = 0; i < dong.Length; i++)
        {
            var v = GD.Nhan("–", 128, 20 + i * 21, 200, GD.Dam);
            chiSo[dong[i].Ma] = v;
            GD.Them(g3, GD.Nhan(dong[i].Chu + ":", 8, 20 + i * 21, 118), v);
        }
        LblSaiPham = new Label
        {
            Text = "SAI PHẠM: –", Location = new Point(336, 20), Size = new Size(g3.Width - 344, 30), Font = GD.Chu(12, true),
            BackColor = GD.Hex("#e0e0e0"), TextAlign = ContentAlignment.MiddleLeft,
        };
        LblSaiPhamCt = new Label { Location = new Point(336, 56), Size = new Size(g3.Width - 344, 132), Font = GD.Chu(8) };
        GD.Them(g3, LblSaiPham, LblSaiPhamCt);

        // ---- hàng giữa: tài nguyên | mô phỏng tải
        var kt = KichThuocTaiNguyen;
        int gw = kt.Width + 16, gh = Math.Max(kt.Height + 62, 250);
        var g4 = GD.Nhom(TenTaiNguyen, 8, 206, gw, gh);
        VeTaiNguyen(g4);
        BtnDatLai = GD.Nut("Đặt lại", GD.Cam, gw - 98, gh - 36, 90, 28);
        GD.Them(g4, BtnDatLai);

        int x5 = 8 + gw + 6;
        var g5 = GD.Nhom("Mô phỏng tải (n người ảo trên k kết nối, rào xuất phát)", x5, 206, W - x5 - 8, gh);
        TxtN = GD.O(n.ToString(CultureInfo.InvariantCulture), 30, 22, 60);
        TxtK = GD.O(k.ToString(CultureInfo.InvariantCulture), 120, 22, 50);
        TxtLan = GD.O(lan.ToString(CultureInfo.InvariantCulture), 262, 22, 36);
        BtnBan = GD.Nut("Bắn đồng loạt", GD.Cam, 8, 52, 124, 28);
        BtnSoSanh = GD.Nut("So sánh 4 chế độ", GD.XanhDuong, 138, 52, 140, 28);
        LblMoPhong = GD.Nhan("", 286, 58, 120);
        Bang = new ListView
        {
            View = View.Details, FullRowSelect = true, Location = new Point(8, 86), Size = new Size(g5.Width - 16, 112), Font = GD.Thuong,
            HeaderStyle = ColumnHeaderStyle.Nonclickable,
        };
        foreach (var (t, w, phai) in new[] { ("Chế độ", 100, false), ("Thành công", 80, true), ("Sai phạm", 70, true), ("Xung đột", 70, true),
                                             ("yc/s", 64, true), ("p99 ms", 74, true) })
            Bang.Columns.Add(t, w, phai ? HorizontalAlignment.Right : HorizontalAlignment.Left);
        LblKetLuan = GD.Nhan("", 8, 202, g5.Width - 16, GD.Chu(8), gh - 208);
        LblKetLuan.AutoEllipsis = false;
        GD.Them(g5, GD.Nhan("n:", 8, 25), TxtN, GD.Nhan("k:", 100, 25), TxtK, GD.Nhan("lần/người:", 180, 25), TxtLan,
                BtnBan, BtnSoSanh, LblMoPhong, Bang, LblKetLuan);

        // ---- nhật ký + thanh trạng thái
        var g6 = GD.Nhom("Nhật ký", 8, 206 + gh + 6, W - 16, 128);
        GD.Them(g6, NhatKy);
        LblTrangThai = new Label
        {
            Dock = DockStyle.Bottom, Height = 24, Text = "Trạng thái: máy chủ chưa chạy", BackColor = Color.White,
            BorderStyle = BorderStyle.Fixed3D, TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true,
        };
        Controls.AddRange([g1, g2, g3, g4, g5, g6, LblTrangThai]);
        ClientSize = new Size(W, 206 + gh + 6 + 128 + 30);

        BtnStart.Click += (_, _) => BatDau();
        BtnStop.Click += (_, _) => Dung();
        BtnTre.Click += (_, _) => DoiTre();
        BtnDatLai.Click += (_, _) => DatLai();
        BtnBan.Click += (_, _) => Ban();
        BtnSoSanh.Click += (_, _) => SoSanh();
        nhip.Tick += (_, _) => CapNhat();
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            Ui.Dispose();
            Lop.Dung();
        };
    }

    // ------------------------------------------------------------ lớp con viết
    protected abstract void VeTaiNguyen(GroupBox g);
    protected abstract void CapNhatTaiNguyen(JsonObject tt, JsonObject tk);
    protected virtual IEnumerable<KeyValuePair<string, object>> ThamSoDatLai() => [];

    // ------------------------------------------------------------ nút
    public void BatDau()
    {
        try { Lop.BatDau(TxtIp.Text.Trim(), int.Parse(TxtCong.Text, CultureInfo.InvariantCulture)); }
        catch (Exception e) when (e is SocketException or FormatException or OverflowException or ArgumentException)
        {
            NhatKy.Ghi($"Không khởi động được: {e.Message}");
            return;
        }
        BtnStart.Enabled = false;
        BtnStop.Enabled = true;
        TxtIp.Enabled = TxtCong.Enabled = false;
    }

    public void Dung()
    {
        BtnStop.Enabled = false;
        Task.Run(() =>                                   // chờ luồng nền dừng: không chặn giao diện
        {
            Lop.Dung();
            Ui.DuaVao(() =>
            {
                BtnStart.Enabled = true;
                TxtIp.Enabled = TxtCong.Enabled = true;
            });
        });
    }

    void DoiCheDo(string cd) => Task.Run(() =>          // có thể chờ hàng đợi cạn
    {
        try { Lop.DatCheDo(cd); }
        catch (ArgumentException e) { Ui.DuaVao(() => NhatKy.Ghi($"Không đổi được chế độ: {e.Message}")); }
    });

    /// <summary>Chọn chế độ như người dùng bấm nút (kịch bản demo).</summary>
    public void ChonCheDo(string cd) => NutCheDo[cd].Checked = true;

    public void DoiTre()
    {
        try { Lop.DatTre(double.Parse(TxtTre.Text, CultureInfo.InvariantCulture)); }
        catch (Exception e) when (e is ArgumentException or FormatException) { NhatKy.Ghi($"Độ trễ không hợp lệ: {e.Message}"); }
    }

    public void DatLai()
    {
        try { Lop.DatLai(ThamSoDatLai()); }
        catch (Exception e) when (e is ArgumentException or FormatException) { NhatKy.Ghi($"Không đặt lại được: {e.Message}"); }
    }

    (int N, int K, int Lan)? DocMoPhong()
    {
        try
        {
            int n = int.Parse(TxtN.Text, CultureInfo.InvariantCulture), k = int.Parse(TxtK.Text, CultureInfo.InvariantCulture);
            int lan = int.Parse(TxtLan.Text, CultureInfo.InvariantCulture);
            if (n < 1 || k < 1 || lan < 1) throw new FormatException("n, k, lần ≥ 1");
            return (n, Math.Min(k, n), lan);
        }
        catch (Exception e) when (e is FormatException or OverflowException)
        {
            NhatKy.Ghi($"Tham số mô phỏng không hợp lệ: {e.Message}");
            return null;
        }
    }

    /// <summary>Chạy mô phỏng trên luồng nền (kết nối TCP tới chính máy chủ này như client bên ngoài).</summary>
    void ChayNen(Func<string, int, Action<string>, List<HangSoSanh>> ham)
    {
        if (DangMoPhong || !Lop.Chay)
        {
            NhatKy.Ghi("Cần START SERVER và chờ lượt mô phỏng trước xong");
            return;
        }
        DangMoPhong = true;
        BtnBan.Enabled = BtnSoSanh.Enabled = false;
        (LblMoPhong.Text, LblMoPhong.ForeColor) = ("đang chạy…", GD.DoDam);
        int i = Lop.DiaChi.LastIndexOf(':');
        string host = Lop.DiaChi[..i];
        int cong = int.Parse(Lop.DiaChi[(i + 1)..], CultureInfo.InvariantCulture);
        void Bao(string text) => Ui.DuaVao(() => NhatKy.Ghi("[mô phỏng] " + text));
        Task.Run(() =>
        {
            List<HangSoSanh>? kq;
            try { kq = ham(host, cong, Bao); }
            catch (Exception e)
            {
                Bao($"lỗi: {e.Message}");
                kq = null;
            }
            Ui.DuaVao(() => XongMoPhong(kq));
        });
    }

    public void Ban()
    {
        if (DocMoPhong() is not var (n, k, lan)) return;
        ChayNen((h, c, b) => MoPhong.ChayLuot(h, c, n, k, lan, b) is { } x ? [x] : []);
    }

    public void SoSanh()
    {
        if (DocMoPhong() is not var (n, k, lan)) return;
        ChayNen((h, c, b) => MoPhong.SoSanh(h, c, n, k, lan, b));
    }

    void XongMoPhong(List<HangSoSanh>? hang)
    {
        DangMoPhong = false;
        BtnBan.Enabled = BtnSoSanh.Enabled = true;
        (LblMoPhong.Text, LblMoPhong.ForeColor) = ("xong", GD.XanhDam);
        if (hang is not { Count: > 0 }) return;
        if (hang.Count == Chung.CheDo.Length)
        {
            HangSoSanh = hang;
            Bang.Items.Clear();
        }
        foreach (var h in hang)
        {
            string[] gt = [h.CheDo, $"{h.ThanhCong}", $"{h.SaiPham}", $"{h.XungDot}", $"{h.YcS}", h.P99Ms is { } p ? Chung.F(p, "F1") : "–"];
            var item = Bang.Items.Cast<ListViewItem>().FirstOrDefault(x => x.Text == h.CheDo);
            if (item == null) Bang.Items.Add(item = new ListViewItem(gt) { UseItemStyleForSubItems = true });
            else for (int c = 0; c < gt.Length; c++) item.SubItems[c].Text = gt[c];
            (item.BackColor, item.ForeColor) = h.SaiPham > 0 ? (GD.Hex("#ffc8c8"), GD.DoDam) : (GD.Hex("#d8f5d8"), Color.Black);
        }
        var sai = hang.Where(h => h.SaiPham > 0).Select(h => h.CheDo).ToList();
        var dung = hang.Where(h => h.SaiPham == 0).Select(h => h.CheDo).ToList();
        LblKetLuan.Text = (sai.Count > 0 ? $"Sai phạm: {string.Join(", ", sai)}. " : "")
                          + (dung.Count > 0 ? $"Không sai phạm: {string.Join(", ", dung)}." : "");
    }

    // ------------------------------------------------------------ vẽ lại định kỳ (250 ms)
    void CapNhat()
    {
        if (!Lop.Chay)
        {
            LblTrangThai.Text = "Trạng thái: máy chủ chưa chạy";
            return;
        }
        var tk = Lop.ThongKeDayDu();
        var tt = Lop.TrangThai();
        long L(string k) => Chung.SoL(tk, k);
        chiSo["ket_noi"].Text = $"{L("so_ket_noi")} / {L("so_nguoi")}";
        chiSo["yc_s"].Text = $"{L("yc_s")}  (TB {L("thong_luong")})";
        chiSo["so_yc"].Text = $"{L("so_yc")}";
        chiSo["thanh_cong"].Text = $"{L("thanh_cong")}";
        var maLoi = (tk["ma_loi"] as JsonObject ?? []).OrderBy(x => x.Key, StringComparer.Ordinal).Select(x => $"{x.Key} {Chung.SoLN(x.Value)}").ToList();
        chiSo["tu_choi"].Text = $"{L("tu_choi")}" + (maLoi.Count > 0 ? "  (" + string.Join(", ", maLoi) + ")" : "");
        chiSo["xung_dot"].Text = $"{L("xung_dot")} / {L("thu_lai")} (hết lượt {L("het_lan_thu")})";
        chiSo["hang_doi"].Text = $"{L("hang_doi")}  ({L("hang_doi_max")})";
        chiSo["tre"].Text = $"{GD.Ms(tk["p50_us"])} / {GD.Ms(tk["p95_us"])} / {GD.Ms(tk["p99_us"])} ms";
        long sp = L("sai_pham_tong");
        LblSaiPham.Text = $"SAI PHẠM: {sp}";
        (LblSaiPham.BackColor, LblSaiPham.ForeColor) = sp > 0 ? (GD.Hex("#ff6060"), Color.White) : (GD.Hex("#70d070"), Color.Black);
        var ct = new List<string>
        {
            string.Join(", ", (tk["sai_pham"] as JsonArray ?? []).OfType<JsonObject>().Select(s => $"{Chung.Chuoi(s, "ten")} {Chung.SoL(s, "so")}")),
        };
        ct.AddRange((tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2).Select(v => $"• {Chung.ChuoiPy(v)}"));
        LblSaiPhamCt.Text = string.Join("\n", ct);
        string cd = Chung.Chuoi(tk, "che_do") ?? "";
        if (!DangMoPhong && NutCheDo.TryGetValue(cd, out var nut) && !nut.Checked)
        {
            dongBoCheDo = true;                          // mô phỏng/quản trị đổi chế độ từ xa
            nut.Checked = true;
            dongBoCheDo = false;
        }
        CapNhatTaiNguyen(tt, tk);
        string tomTat = Chung.Chuoi(tk, "tom_tat") ?? "";
        LblTrangThai.Text = $"Trạng thái: máy chủ chạy tại {Lop.DiaChi} – chế độ {cd}, {L("so_ket_noi")} kết nối – {tomTat}";
        dinhKy.InNeuDenHan(() => $"{cd}: {L("so_yc")} yc, tc {L("thanh_cong")}, {L("yc_s")} yc/s, sai phạm {sp}; {tomTat}");
    }
}

// ===================================================================== cửa sổ người dùng
/// <summary>Kết nối/rời, nhật ký, thanh trạng thái; lớp con vẽ phần nghiệp vụ trong Khung.</summary>
abstract class FormNguoiDungCoSo : Form
{
    protected virtual string TieuDe => "Ứng dụng";
    /// <summary>Kích thước phần nghiệp vụ (Khung) của cửa sổ.</summary>
    protected abstract Size KichThuocKhung { get; }

    protected readonly HangDoiGiaoDien Ui = new();
    public string Ten;
    public KetNoiKhach? Kn;
    public JsonObject? Tt;                               // cap_nhat mới nhất
    public double? RttCuoi;
    public readonly TextBox TxtTen, TxtIp, TxtCong;
    public readonly Button BtnKetNoi, BtnRoi;
    public readonly Panel Khung;
    public readonly NhatKy NhatKy;
    public readonly Label LblTrangThai;
    readonly ThongKeDinhKy dinhKy;
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 200 };

    protected FormNguoiDungCoSo(string ten, string host, int cong)
    {
        Ten = ten;
        var kt = KichThuocKhung;
        int W = kt.Width + 16;
        Text = $"{TieuDe} – {ten}";
        BackColor = GD.Nen;
        FormBorderStyle = FormBorderStyle.FixedSingle;
        MaximizeBox = false;
        StartPosition = FormStartPosition.Manual;
        TxtTen = GD.O(ten, 40, 8, 70);
        TxtIp = GD.O(host, 136, 8, 100);
        TxtCong = GD.O(cong.ToString(CultureInfo.InvariantCulture), 284, 8, 54);
        BtnKetNoi = GD.Nut("Kết nối", GD.XanhLa, 346, 5, 80, 26);
        BtnRoi = GD.Nut("Rời", GD.Do, 430, 5, 54, 26);
        BtnRoi.Enabled = false;
        Khung = new Panel { Location = new Point(8, 38), Size = kt, BackColor = GD.Nen };
        NhatKy = new NhatKy(ten, 8, 20, W - 32, 76);
        var gNk = GD.Nhom("Nhật ký", 8, 38 + kt.Height + 4, W - 16, 104);
        GD.Them(gNk, NhatKy);
        LblTrangThai = new Label
        {
            Dock = DockStyle.Bottom, Height = 24, Text = "Chưa kết nối", BackColor = Color.White, BorderStyle = BorderStyle.Fixed3D,
            TextAlign = ContentAlignment.MiddleLeft, AutoEllipsis = true,
        };
        Controls.AddRange([GD.Nhan("Tên:", 8, 11), TxtTen, GD.Nhan("IP:", 116, 11), TxtIp, GD.Nhan("Cổng:", 244, 11), TxtCong,
                           BtnKetNoi, BtnRoi, Khung, gNk, LblTrangThai]);
        ClientSize = new Size(W, 38 + kt.Height + 4 + 104 + 30);
        dinhKy = new ThongKeDinhKy(ten);
        Ve(Khung);
        BtnKetNoi.Click += (_, _) => KetNoi();
        BtnRoi.Click += (_, _) => Roi();
        nhip.Tick += (_, _) =>
        {
            MoiNhip();
            if (Kn != null && Tt != null) dinhKy.InNeuDenHan(DongDinhKy);
        };
        nhip.Start();
        FormClosing += (_, _) =>
        {
            nhip.Stop();
            Ui.Dispose();
            if (Kn != null)
            {
                Kn.KhiDong = () => { };
                Kn.Dong();
            }
        };
    }

    // ------------------------------------------------------------ lớp con viết
    protected abstract void Ve(Panel khung);
    protected abstract void CapNhat(JsonObject tt);
    protected abstract void KetQua(string loai, JsonObject truong, JsonObject tin, double rtt);
    protected virtual void SuKien(string text) => NhatKy.Ghi($"* {text}");
    /// <summary>Gọi mỗi 200 ms trên luồng giao diện (đồng hồ đếm ngược…).</summary>
    protected virtual void MoiNhip() { }
    protected virtual string DongDinhKy() => "";

    // ------------------------------------------------------------ kết nối
    public void KetNoi()
    {
        string ten = TxtTen.Text.Trim(), host = TxtIp.Text.Trim();
        if (!int.TryParse(TxtCong.Text, NumberStyles.Integer, CultureInfo.InvariantCulture, out int cong))
        {
            NhatKy.Ghi("Cổng không hợp lệ");
            return;
        }
        Ten = ten;
        BtnKetNoi.Enabled = false;
        KetNoiKhach? kn = null;
        kn = new KetNoiKhach(tin => Ui.DuaVao(() => NhanCapNhat(tin)), tin => Ui.DuaVao(() => NhanTin(tin)),
                             () => Ui.DuaVao(() => MatKetNoi(kn)));
        Task.Run(() =>                                   // kết nối trên luồng nền: không đơ giao diện
        {
            try
            {
                kn.KetNoi(host, cong, ten);
                Ui.DuaVao(() => DaKetNoi(kn));
            }
            catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ArgumentException)
            {
                Ui.DuaVao(() =>
                {
                    NhatKy.Ghi($"Không kết nối được: {e.Message}");
                    BtnKetNoi.Enabled = true;
                });
            }
        });
    }

    protected virtual void DaKetNoi(KetNoiKhach kn)
    {
        Kn = kn;
        BtnRoi.Enabled = true;
        TxtTen.Enabled = TxtIp.Enabled = TxtCong.Enabled = false;
        NhatKy.Ghi($"Đã kết nối {Chung.Chuoi(kn.Chao, "may_chu")} (chế độ {Chung.Chuoi(kn.Chao, "che_do")})");
        NhanCapNhat(kn.TrangThai!);
        kn.BatDauDoc();
    }

    public void Roi()
    {
        if (Kn == null) return;
        var kn = Kn;
        Kn = null;
        kn.KhiDong = () => { };
        kn.Dong();
        MatKetNoi(null, "Đã rời");
    }

    protected virtual void MatKetNoi(KetNoiKhach? kn, string lyDo = "Mất kết nối máy chủ")
    {
        if (kn != null && kn != Kn) return;
        Kn = null;
        NhatKy.Ghi(lyDo);
        LblTrangThai.Text = lyDo;
        BtnKetNoi.Enabled = true;
        BtnRoi.Enabled = false;
        TxtTen.Enabled = TxtIp.Enabled = TxtCong.Enabled = true;
    }

    void NhanCapNhat(JsonObject tt)
    {
        Tt = tt;
        CapNhat(tt);
        CapNhatThanhTt();
    }

    void NhanTin(JsonObject tin)
    {
        string? loai = Chung.Chuoi(tin, "loai");
        if (loai == "su_kien") SuKien(Chung.Chuoi(tin, "noi_dung") ?? "");
        else if (loai == "loi") NhatKy.Ghi($"Lỗi: {Chung.Chuoi(tin, "ly_do")}");
    }

    void CapNhatThanhTt()
    {
        if (Kn == null || Tt == null) return;
        string rtt = RttCuoi is { } r ? $", khứ hồi {Chung.F(r, "F1")} ms" : "";
        LblTrangThai.Text = $"Kết nối {TxtIp.Text}:{TxtCong.Text} – máy chủ chế độ {Chung.Chuoi(Tt, "che_do")}{rtt}";
    }

    // ------------------------------------------------------------ gửi yêu cầu
    public void GuiYc(string loai, JsonObject? truong = null)
    {
        truong ??= [];
        if (Kn == null)
        {
            NhatKy.Ghi("Chưa kết nối");
            return;
        }
        try
        {
            Kn.GuiYc(loai, (tin, rtt) => Ui.DuaVao(() =>    // luồng đọc -> chuyển sang luồng giao diện
            {
                RttCuoi = rtt;
                KetQua(loai, truong, tin, rtt);
                CapNhatThanhTt();
            }), truong);
        }
        catch (Exception e) when (e is SocketException or ObjectDisposedException or IOException)
        {
            NhatKy.Ghi($"Gửi lỗi: {e.Message}");
        }
    }
}
