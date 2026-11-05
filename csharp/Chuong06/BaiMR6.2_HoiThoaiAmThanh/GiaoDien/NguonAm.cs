using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Nguồn micro cho bản giao diện: micro thật (winmm) hoặc giả lập (sóng sin như bản dòng lệnh).
// Cả server và client dùng chung; phần này không phụ thuộc WinForms.
interface INguonMic : IDisposable
{
    /// <summary>Một khung 20 ms (320 mẫu), hoặc null nếu thiết bị chưa có dữ liệu.</summary>
    short[]? LayKhung();
    string MoTa { get; }
    bool That { get; }                                   // true = thiết bị thật, false = giả lập
}

sealed class MicGiaLap(double tanSo) : INguonMic
{
    readonly NguonSin sin = new(tanSo, 0.25);            // lớp NguonSin của bản dòng lệnh (KhachAmThanh.cs)
    public short[]? LayKhung() => sin.LayKhung();
    public string MoTa => $"giả lập (sin {F(tanSo, "F0")} Hz)";
    public bool That => false;
    public void Dispose() { }
}

sealed class MicThat(int chiSo, string ten) : INguonMic
{
    readonly ThuAmWinMM thu = new(chiSo);                // ném lỗi nếu không mở được thiết bị
    public short[]? LayKhung() => thu.LayKhung();
    public string MoTa => $"micro thật «{ten}»";
    public bool That => true;
    public void Dispose() => thu.Dispose();
}

/// <summary>Đo mức trung bình (dBFS) trên một cửa sổ: luồng âm thanh gọi Them mỗi 20 ms,
/// luồng giao diện gọi Lay mỗi 100 ms (lấy RMS của các khung đã cộng rồi xóa).</summary>
sealed class DoMuc
{
    readonly object khoa = new();
    double tong;
    long dem;

    public void Them(short[]? mau)
    {
        lock (khoa)
        {
            if (mau == null)
            {
                dem += MauMoiKhung;                       // khung trống = im lặng
                return;
            }
            foreach (var x in mau) tong += (double)x * x;
            dem += mau.Length;
        }
    }

    public double Lay()
    {
        lock (khoa)
        {
            double rms = dem > 0 ? Math.Sqrt(tong / dem) : 0;
            tong = 0;
            dem = 0;
            return rms > 0 ? 20 * Math.Log10(rms / 32768) : double.NegativeInfinity;
        }
    }
}

static class AmThanhPhu
{
    /// <summary>Nhân hệ số và cắt ngọn – dùng cho Mic Volume và âm lượng nghe (0..2).</summary>
    public static short[] NhanHeSo(short[] mau, double heSo) =>
        heSo == 1.0 ? mau : mau.Select(x => CatNgon(Math.Round(x * heSo))).ToArray();
}
