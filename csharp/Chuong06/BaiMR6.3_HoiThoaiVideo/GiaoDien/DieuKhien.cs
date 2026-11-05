using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace BaiMR6_3;

/// <summary>Ô hiển thị một khung hình (tự vẽ): ảnh phóng vừa ô giữ tỉ lệ 4:3, tên + dòng phụ ở góc trên,
/// chấm đỏ khi camera tắt, viền vàng khi được chọn. Chỉ luồng giao diện gọi Dat.</summary>
class OHinh : Control
{
    Bitmap? hinh;
    string ten = "", phu = "";
    bool camTat, duocChon;

    public OHinh()
    {
        DoubleBuffered = true;
        BackColor = Color.FromArgb(225, 225, 225);
        Margin = new Padding(3);
    }

    /// <summary>Đặt nội dung ô; ô giữ (và tự giải phóng) bitmap được truyền vào.</summary>
    public void Dat(Bitmap? b, string ten, string phu, bool camTat, bool duocChon = false)
    {
        hinh?.Dispose();
        (hinh, this.ten, this.phu, this.camTat, this.duocChon) = (b, ten, phu, camTat, duocChon);
        Invalidate();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        if (hinh != null)
        {
            float tl = Math.Min((float)Width / hinh.Width, (float)Height / hinh.Height);
            int w = (int)(hinh.Width * tl), h = (int)(hinh.Height * tl);
            g.InterpolationMode = InterpolationMode.NearestNeighbor;   // độ phân giải thấp hiện thành ô vuông rõ ràng
            g.PixelOffsetMode = PixelOffsetMode.Half;
            g.DrawImage(hinh, (Width - w) / 2, (Height - h) / 2, w, h);
        }
        if (ten != "")
        {
            using var nen = new SolidBrush(Color.FromArgb(150, 0, 0, 0));
            g.FillRectangle(nen, 0, 0, Width, 36);
            using var dam = new Font(Font, FontStyle.Bold);
            TextRenderer.DrawText(g, ten, dam, new Point(6, 2), Color.White);
            TextRenderer.DrawText(g, phu, Font, new Point(6, 18), Color.Yellow);
        }
        if (camTat)
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.FillEllipse(Brushes.Red, Width - 22, 8, 14, 14);
        }
        using var vien = new Pen(duocChon ? Color.Gold : Color.Gray, duocChon ? 4 : 2);
        g.DrawRectangle(vien, 1, 1, Width - 3, Height - 3);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) hinh?.Dispose();
        base.Dispose(disposing);
    }
}

static class GD
{
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

    public static Button Nut(string text, int x, int y, int w = 110, int h = 30) => new()
    {
        Text = text, Location = new Point(x, y), Size = new Size(w, h), FlatStyle = FlatStyle.System,
    };

    public static Label Nhan(string text, int x, int y, int w = 0) => w > 0
        ? new Label { Text = text, Location = new Point(x, y), Size = new Size(w, 18), AutoEllipsis = true }
        : new Label { Text = text, Location = new Point(x, y), AutoSize = true };

    public static TextBox O(string text, int x, int y, int w) => new() { Text = text, Location = new Point(x, y), Width = w };

    public static TrackBar Truot(int x, int y, int w, int min, int max, int giaTri) => new()
    {
        Minimum = min, Maximum = max, Value = giaTri, TickStyle = TickStyle.None, AutoSize = false,
        Location = new Point(x, y), Size = new Size(w, 26),
    };

    public static ComboBox Chon(int x, int y, int w, params string[] muc)
    {
        var c = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Location = new Point(x, y), Width = w };
        c.Items.AddRange(muc);
        if (muc.Length > 0) c.SelectedIndex = 0;
        return c;
    }

    /// <summary>Hẹn giờ một lần: gom nhiều lần kéo thanh trượt thành một lệnh (sau khi người dùng dừng tay).</summary>
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
