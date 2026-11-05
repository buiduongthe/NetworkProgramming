using System.Drawing;
using System.Windows.Forms;

namespace BaiMR6_2;

/// <summary>Thanh đo mức âm (dBFS) tự vẽ: -60 dBFS (trống) … 0 dBFS (đầy); xanh – vàng – đỏ.
/// Giữ "đỉnh rơi chậm" (mỗi lần cập nhật rơi tối đa 3 dB) để mắt kịp nhìn. Chỉ luồng giao diện gọi DatMuc.</summary>
class ThanhMucAm : Control
{
    double hienThi = double.NegativeInfinity;

    public ThanhMucAm()
    {
        DoubleBuffered = true;                             // vẽ vào bộ đệm trước: không nhấp nháy
        Size = new Size(200, 18);
        BackColor = Color.FromArgb(30, 30, 30);
        ForeColor = Color.White;
    }

    public double LayMuc() => hienThi;

    public void DatMuc(double db)
    {
        if (double.IsNegativeInfinity(db) || db < -90) db = double.NegativeInfinity;
        double roi = double.IsNegativeInfinity(hienThi) ? double.NegativeInfinity : hienThi - 3;
        hienThi = Math.Max(db, roi < -60 ? double.NegativeInfinity : roi);
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        double ti = double.IsNegativeInfinity(hienThi) ? 0 : Math.Clamp((hienThi + 60) / 60, 0, 1);
        int w = (int)((Width - 2) * ti);
        var mau = hienThi > -6 ? Color.FromArgb(220, 60, 50) : hienThi > -18 ? Color.FromArgb(230, 190, 40) : Color.FromArgb(60, 180, 75);
        using (var b = new SolidBrush(mau)) g.FillRectangle(b, 1, 1, w, Height - 2);
        string chu = double.IsNegativeInfinity(hienThi) ? "--- dBFS" : $"{hienThi:F1} dBFS";
        TextRenderer.DrawText(g, chu, Font, ClientRectangle, ForeColor, TextFormatFlags.Right | TextFormatFlags.VerticalCenter);
    }
}

static class GD
{
    public static readonly Color NenXanh = Color.FromArgb(176, 196, 222);     // nền nhóm điều khiển (giống ảnh mẫu)
    public static readonly Color NutXanhLa = Color.FromArgb(144, 238, 144), NutDo = Color.FromArgb(240, 128, 128),
                                 NutXanhDuong = Color.FromArgb(173, 216, 230), NutCam = Color.FromArgb(255, 165, 0);

    public static Button Nut(string text, Color mau, int x, int y, int w = 100, int h = 30) => new()
    {
        Text = text, BackColor = mau, FlatStyle = FlatStyle.Flat, Location = new Point(x, y), Size = new Size(w, h),
        Font = new Font(SystemFonts.DefaultFont, FontStyle.Bold),
    };

    public static Label Nhan(string text, int x, int y, int w = 0) => w > 0
        ? new Label { Text = text, Location = new Point(x, y), Size = new Size(w, 20), AutoEllipsis = true }
        : new Label { Text = text, Location = new Point(x, y), AutoSize = true };

    public static TextBox O(string text, int x, int y, int w) => new() { Text = text, Location = new Point(x, y), Width = w };

    public static TrackBar Truot(int x, int y, int w, int max = 200, int giaTri = 100) => new()
    {
        Minimum = 0, Maximum = max, Value = giaTri, TickStyle = TickStyle.None, AutoSize = false,
        Location = new Point(x, y), Size = new Size(w, 26),
    };

    public static GroupBox Nhom(string text, int w, int h) => new()
    {
        Text = text, Size = new Size(w, h), Margin = new Padding(6), BackColor = NenXanh,
        Font = new Font(SystemFonts.DefaultFont, FontStyle.Bold),
    };

    /// <summary>Các điều khiển con của GroupBox lấy lại phông thường (GroupBox để tiêu đề in đậm).</summary>
    public static void PhongThuong(Control nhom)
    {
        foreach (Control c in nhom.Controls)
            if (c is not Button) c.Font = SystemFonts.DefaultFont;
    }

    public static string PhanTram(double v) => $"{Math.Round(v * 100):F0}%";

    /// <summary>Hẹn giờ một lần: gom nhiều lần kéo thanh trượt thành một lệnh gửi (sau khi người dùng dừng tay).</summary>
    public static System.Windows.Forms.Timer TreLenh(int ms, Action viec)
    {
        var t = new System.Windows.Forms.Timer { Interval = ms };
        t.Tick += (_, _) =>
        {
            t.Stop();
            viec();
        };
        return t;
    }

    public static void ChayLai(System.Windows.Forms.Timer t)
    {
        t.Stop();
        t.Start();
    }
}
