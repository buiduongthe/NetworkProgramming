using System.Collections.Concurrent;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Thành phần giao diện WinForms dùng chung cho FormServer, FormPhat, FormXem.
// Quy tắc (như MR6.2, MR6.3): CHỈ luồng giao diện chạm vào điều khiển. Luồng mạng/âm thanh/hiển thị của lõi đưa
// dòng nhật ký vào HangDoiGiaoDien (ConcurrentQueue) hoặc ghi số liệu vào đối tượng lõi dưới khóa; luồng giao diện
// đọc định kỳ bằng System.Windows.Forms.Timer rồi vẽ lại.
static class GD
{
    public static readonly Color Nen = Color.FromArgb(0xB0, 0xC4, 0xDE);
    public static readonly Color XanhLa = Color.FromArgb(0x90, 0xEE, 0x90), Do = Color.FromArgb(0xF0, 0x80, 0x80);
    public static readonly Color XanhDuong = Color.FromArgb(0xAD, 0xD8, 0xE6), Cam = Color.FromArgb(0xFF, 0xB3, 0x47);
    public static readonly Color Xam = Color.FromArgb(0xDD, 0xDD, 0xDD);

    /// <summary>Ảnh RGB thô (byte[], 3 byte/điểm) -> Bitmap 24 bit của GDI+ (thứ tự B, G, R; mỗi hàng đệm tới bội 4 byte).</summary>
    public static Bitmap SangBitmap(byte[] rgb, int w, int h)
    {
        var bmp = new Bitmap(w, h, PixelFormat.Format24bppRgb);
        var d = bmp.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.WriteOnly, PixelFormat.Format24bppRgb);
        var hang = new byte[w * 3];
        for (int y = 0; y < h; y++)
        {
            for (int x = 0; x < w; x++)
            {
                int i = (y * w + x) * 3;
                hang[x * 3] = rgb[i + 2];
                hang[x * 3 + 1] = rgb[i + 1];
                hang[x * 3 + 2] = rgb[i];
            }
            Marshal.Copy(hang, 0, d.Scan0 + y * d.Stride, hang.Length);
        }
        bmp.UnlockBits(d);
        return bmp;
    }

    public static Button Nut(string text, Color mau, int x, int y, int w = 110, int h = 28)
    {
        var b = new Button { Text = text, Location = new Point(x, y), Size = new Size(w, h), BackColor = mau, UseVisualStyleBackColor = false };
        b.FlatStyle = FlatStyle.Flat;
        b.FlatAppearance.BorderColor = Color.Gray;
        return b;
    }

    public static Label Nhan(string text, int x, int y, int w = 0, Font? font = null)
    {
        var l = w > 0 ? new Label { Text = text, Location = new Point(x, y), Size = new Size(w, 18), AutoEllipsis = true }
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

    /// <summary>Thêm điều khiển vào nhóm và trả font thường cho chúng (tiêu đề nhóm in đậm, nội dung không).</summary>
    public static void Them(GroupBox g, params Control[] ds)
    {
        foreach (var c in ds)
        {
            if (ReferenceEquals(c.Font, Control.DefaultFont)) c.Font = Thuong;   // chưa đặt font riêng: không thừa hưởng chữ đậm
            g.Controls.Add(c);
        }
    }

    public static readonly Font Thuong = new(Control.DefaultFont, FontStyle.Regular);
    public static readonly Font Dam = new(Control.DefaultFont.FontFamily, 10, FontStyle.Bold);
    public static readonly Font CoDinh = new(FontFamily.GenericMonospace, 9);

    public static NumericUpDown So(int x, int y, int min, int max, int giaTri) => new()
    {
        Minimum = min, Maximum = max, Value = giaTri, Location = new Point(x, y), Width = 50,
    };

    public static ComboBox Chon(int x, int y, int w) => new() { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(x, y), Width = w };

    public static string ChuMs(double? x, bool dau = false) => x is { } v ? Chung.ChuMs(v, dau) + " ms" : "–";

    /// <summary>Chụp một cửa sổ thành Bitmap (Control.DrawToBitmap).</summary>
    public static Bitmap Chup(Form f)
    {
        f.Refresh();
        var bmp = new Bitmap(f.Width, f.Height);
        f.DrawToBitmap(bmp, new Rectangle(0, 0, f.Width, f.Height));
        return bmp;
    }
}

/// <summary>Cầu nối luồng nền -> luồng giao diện: DuaVao() gọi từ bất kỳ luồng nào; bộ hẹn giờ 50 ms trên luồng
/// giao diện lấy ra và thực hiện.</summary>
sealed class HangDoiGiaoDien : IDisposable
{
    readonly ConcurrentQueue<Action> hang = new();
    readonly System.Windows.Forms.Timer nhip = new() { Interval = 50 };

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

    public void Dispose() => nhip.Dispose();
}

/// <summary>Ô hình: vẽ Bitmap phóng vừa ô bằng điểm gần nhất (khung 160×120 hiện thành ô vuông rõ), chữ chờ ở giữa.</summary>
sealed class OAnh : Control
{
    Bitmap? hinh;
    string chuCho = "";

    public OAnh(int w, int h)
    {
        DoubleBuffered = true;
        BackColor = Color.Black;
        Size = new Size(w, h);
    }

    /// <summary>Đặt ảnh (ô giữ và tự giải phóng bitmap) và chữ chờ.</summary>
    public void Dat(Bitmap? b, string cho = "")
    {
        hinh?.Dispose();
        (hinh, chuCho) = (b, cho);
        Invalidate();
    }

    public bool CoHinh => hinh != null;

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        if (hinh != null)
        {
            e.Graphics.InterpolationMode = InterpolationMode.NearestNeighbor;
            e.Graphics.PixelOffsetMode = PixelOffsetMode.Half;
            e.Graphics.DrawImage(hinh, 0, 0, Width, Height);
        }
        else if (chuCho != "")
            TextRenderer.DrawText(e.Graphics, chuCho, Font, ClientRectangle, Color.Silver,
                                  TextFormatFlags.HorizontalCenter | TextFormatFlags.VerticalCenter);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) hinh?.Dispose();
        base.Dispose(disposing);
    }
}

/// <summary>Thanh đo mức âm -60 … 0 dBFS (như MR6.2): xanh &lt; -12, vàng &lt; -3, đỏ gần cắt ngọn; mức hiện giảm dần
/// 2 dB mỗi lần cập nhật (giữ đỉnh cho mắt kịp thấy).</summary>
sealed class ThanhMuc : Control
{
    const int Chu = 66;
    double hien = double.NegativeInfinity, db = double.NegativeInfinity;

    public ThanhMuc(int x, int y, int w = 200, int h = 18)
    {
        DoubleBuffered = true;
        (Location, Size) = (new Point(x, y), new Size(w, h));
        BackColor = Color.FromArgb(0x20, 0x20, 0x20);
    }

    public void Dat(double moi)
    {
        db = moi;
        hien = moi > hien ? moi : hien - 2.0;
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        int rong = Width - Chu;
        double tiLe = hien < -60 ? 0 : Math.Min(1.0, (hien + 60) / 60);
        var mau = hien < -12 ? Color.FromArgb(0x30, 0xC0, 0x30) : hien < -3 ? Color.FromArgb(0xE0, 0xC0, 0x20) : Color.FromArgb(0xE0, 0x30, 0x30);
        using (var b = new SolidBrush(mau)) g.FillRectangle(b, 0, 0, (int)(tiLe * rong), Height);
        using (var b = new SolidBrush(Color.FromArgb(0xF4, 0xF4, 0xF4))) g.FillRectangle(b, rong, 0, Chu, Height);
        string chu = db < -90 ? "--- dBFS" : $"{db,5:F1} dBFS";
        TextRenderer.DrawText(g, chu, Font, new Rectangle(rong, 0, Chu - 4, Height), Color.Black,
                              TextFormatFlags.Right | TextFormatFlags.VerticalCenter);
        g.DrawRectangle(Pens.Gray, 0, 0, Width - 1, Height - 1);
    }
}

/// <summary>Ô nhật ký cuộn (chỉ đọc), mỗi dòng có giờ. InRa = true (đặt khi chạy riêng từng cửa sổ): mỗi dòng cũng
/// in ra dòng lệnh kèm tiền tố tên, để chuyển hướng được vào tệp nhật ký khi thử với thiết bị thật.</summary>
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

/// <summary>Khi chạy riêng (NhatKy.InRa): cứ `chuKy` giây in một dòng số liệu ra dòng lệnh (không ghi vào ô nhật ký)
/// – để tệp nhật ký khi thử thiết bị thật cho thấy tiếng/hình có thật sự đi qua.</summary>
sealed class ThongKeDinhKy(string ten, double chuKy = 5.0)
{
    double han = Chung.Now + chuKy;

    public void InNeuDenHan(Func<string> taoDong)
    {
        double bayGio = Chung.Now;
        if (!NhatKy.InRa || bayGio < han) return;
        han = bayGio + chuKy;                                     // không "đuổi bù" các chu kỳ đã lỡ (lúc chưa phát)
        Console.WriteLine($"{DateTime.Now:HH:mm:ss} [{ten}] {taoDong()}");
    }
}
