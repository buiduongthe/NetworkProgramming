using System.Buffers.Binary;
using System.Globalization;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR6_2;

// Các thủ tục dùng chung cho server và client hội thoại âm thanh.
// Âm thanh số PCM (Pulse-Code Modulation) 16 bit, mono, tần số lấy mẫu 16000 Hz:
//   - lấy mẫu: mỗi giây đo biên độ sóng âm 16000 lần; mỗi mẫu là số nguyên 16 bit có dấu
//     (-32768 … 32767). Theo định lý Nyquist, 16 kHz biểu diễn được âm tới 8 kHz – đủ cho giọng nói;
//   - tốc độ bit: 16000 mẫu/s × 16 bit = 256000 bit/s = 256 kbit/s (32000 byte/s);
//   - khung 20 ms: 16000 × 0,020 = 320 mẫu = 640 byte -> 50 gói mỗi giây;
//   - mỗi gói thêm 12 byte RTP + 8 byte UDP + 20 byte IPv4 = 40 byte phụ phí
//     -> 50 × 40 × 8 = 16 kbit/s, tổng ≈ 272 kbit/s mỗi chiều cho mỗi client.
// Mức âm đo bằng dBFS (decibel so với toàn thang – full scale): 0 dBFS là mức RMS lớn nhất (32768).
static class Chung
{
    public const int TanSoMau = 16000;                          // mẫu/giây
    public const int KhungMs = 20;                              // độ dài một khung (gói) âm thanh
    public const int MauMoiKhung = TanSoMau * KhungMs / 1000;   // 320 mẫu
    public const int ByteMoiKhung = MauMoiKhung * 2;            // 640 byte
    public const int PtAmThanh = 96;        // payload type động (RFC 3551) dùng cho L16/16000/mono
    public const int PtGiuKetNoi = 127;     // gói rỗng: đăng ký địa chỉ UDP, giữ kết nối khi mic tắt
    public const uint SsrcServer = 0;       // mã nguồn của luồng trộn do server gửi
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static string F(double v, string fmt) => v.ToString(fmt, CultureInfo.InvariantCulture);

    // ---------------------------------------------------------------- Gói RTP (RFC 3550)
    /// <summary>Header RTP cố định 12 byte: V=2|P|X|CC (1 byte), M|PT (1 byte), số thứ tự (16 bit),
    /// nhãn thời gian (32 bit, đơn vị: mẫu), SSRC (32 bit, ở đây là mã client).</summary>
    public static byte[] DongGoiRtp(int pt, long seq, long ts, uint ssrc, byte[]? payload = null)
    {
        payload ??= [];
        var g = new byte[12 + payload.Length];
        g[0] = 0x80;
        g[1] = (byte)(pt & 0x7F);
        BinaryPrimitives.WriteUInt16BigEndian(g.AsSpan(2), (ushort)(seq & 0xFFFF));
        BinaryPrimitives.WriteUInt32BigEndian(g.AsSpan(4), (uint)(ts & 0xFFFFFFFF));
        BinaryPrimitives.WriteUInt32BigEndian(g.AsSpan(8), ssrc);
        payload.CopyTo(g, 12);
        return g;
    }

    /// <summary>Trả về (pt, seq, ts, ssrc, payload) hoặc null nếu gói sai dạng.</summary>
    public static (int Pt, int Seq, uint Ts, uint Ssrc, byte[] Payload)? TachRtp(ReadOnlySpan<byte> g)
    {
        if (g.Length < 12 || g[0] >> 6 != 2) return null;       // phiên bản RTP phải là 2
        return (g[1] & 0x7F, BinaryPrimitives.ReadUInt16BigEndian(g[2..]), BinaryPrimitives.ReadUInt32BigEndian(g[4..]),
                BinaryPrimitives.ReadUInt32BigEndian(g[8..]), g[12..].ToArray());
    }

    // ---------------------------------------------------------------- PCM <-> byte
    /// <summary>RTP truyền L16 theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11.</summary>
    public static byte[] PcmSangByte(short[] mau)
    {
        var b = new byte[mau.Length * 2];
        for (int i = 0; i < mau.Length; i++) BinaryPrimitives.WriteInt16BigEndian(b.AsSpan(2 * i), mau[i]);
        return b;
    }

    public static short[] ByteSangPcm(byte[] b)
    {
        var m = new short[b.Length / 2];
        for (int i = 0; i < m.Length; i++) m[i] = BinaryPrimitives.ReadInt16BigEndian(b.AsSpan(2 * i));
        return m;
    }

    /// <summary>Cắt ngọn (clipping): giá trị vượt phạm vi 16 bit bị giữ ở biên.</summary>
    public static short CatNgon(double x) => (short)Math.Clamp(x, short.MinValue, short.MaxValue);

    // ---------------------------------------------------------------- Mức âm
    /// <summary>Mức RMS (căn bậc hai của trung bình bình phương) tính theo dBFS.</summary>
    public static double MucDbfs(IReadOnlyCollection<short> mau)
    {
        if (mau.Count == 0) return double.NegativeInfinity;
        double s = 0;
        foreach (var x in mau) s += (double)x * x;
        double rms = Math.Sqrt(s / mau.Count);
        return rms > 0 ? 20 * Math.Log10(rms / 32768) : double.NegativeInfinity;
    }

    /// <summary>Thanh đo mức dạng chữ: -60 dBFS (trống) … 0 dBFS (đầy).</summary>
    public static string ThanhMuc(double db, int rong = 16)
    {
        int n = double.IsNegativeInfinity(db) ? 0 : (int)Math.Max(0, Math.Min(rong, Math.Round((db + 60) / 60 * rong)));
        return "|" + new string('#', n) + new string('-', rong - n) + "|";
    }

    public static string ChuDb(double db, int rong = 5) => db < -90 ? "---".PadLeft(rong) : F(db, "F1").PadLeft(rong);

    // ---------------------------------------------------------------- Phân tích tần số
    /// <summary>Biên độ của thành phần tần số tanSo trong một khối mẫu, bằng thuật toán Goertzel:
    ///     s[n] = x[n] + 2cos(ω)·s[n-1] - s[n-2],  ω = 2π·f/fs
    /// sau N mẫu: |X|² = s1² + s2² - 2cos(ω)·s1·s2. Tín hiệu được nhân trước với cửa sổ Hann để
    /// giảm rò phổ từ các âm khác; biên độ sin A ≈ 2|X| / Σw.</summary>
    public static double GoertzelBienDo(ReadOnlySpan<short> mau, double tanSo, int fs = TanSoMau)
    {
        int n = mau.Length;
        double w = 2 * Math.PI * tanSo / fs, heSo = 2 * Math.Cos(w);
        double s1 = 0, s2 = 0, tongW = 0, k = 2 * Math.PI / (n - 1);
        for (int i = 0; i < n; i++)
        {
            double cs = 0.5 - 0.5 * Math.Cos(k * i);              // cửa sổ Hann
            tongW += cs;
            (s1, s2) = (mau[i] * cs + heSo * s1 - s2, s1);
        }
        return 2 * Math.Sqrt(Math.Max(s1 * s1 + s2 * s2 - heSo * s1 * s2, 0.0)) / tongW;
    }

    /// <summary>Mức RMS (dBFS) của một âm trong đoạn dài: chia khối 0,1 s (độ phân giải 10 Hz), đo biên độ
    /// từng khối rồi lấy trung bình công suất (không bị sai khi pha của âm nhảy giữa đoạn).</summary>
    public static double MucAmDbfs(ReadOnlySpan<short> mau, double tanSo, int khoi = 1600)
    {
        var ds = new List<double>();
        for (int i = 0; i + khoi <= mau.Length; i += khoi) ds.Add(GoertzelBienDo(mau.Slice(i, khoi), tanSo));
        if (ds.Count == 0) return double.NegativeInfinity;
        double rms = Math.Sqrt(ds.Sum(a => a * a) / ds.Count) / Math.Sqrt(2);   // biên độ sin -> RMS
        return rms > 0.5 ? 20 * Math.Log10(rms / 32768) : double.NegativeInfinity;
    }

    // ---------------------------------------------------------------- Tệp WAV
    /// <summary>WAV (RIFF) lưu mẫu theo little-endian, khác thứ tự byte mạng của RTP.</summary>
    public static void GhiWav(string path, IReadOnlyList<short> mau)
    {
        using var w = new BinaryWriter(File.Create(path));        // BinaryWriter ghi little-endian
        int data = mau.Count * 2;
        w.Write("RIFF"u8); w.Write(36 + data); w.Write("WAVE"u8);
        w.Write("fmt "u8); w.Write(16); w.Write((short)1); w.Write((short)1);    // PCM, mono
        w.Write(TanSoMau); w.Write(TanSoMau * 2); w.Write((short)2); w.Write((short)16);
        w.Write("data"u8); w.Write(data);
        foreach (var x in mau) w.Write(x);
    }

    /// <summary>Đọc WAV PCM 16 bit mono 16000 Hz (tìm khối "fmt " và "data").</summary>
    public static short[] DocWav(string path)
    {
        var b = File.ReadAllBytes(path);
        if (b.Length < 12 || Encoding.ASCII.GetString(b, 0, 4) != "RIFF" || Encoding.ASCII.GetString(b, 8, 4) != "WAVE")
            throw new InvalidDataException("không phải tệp WAV");
        int pos = 12;
        bool fmtOk = false;
        while (pos + 8 <= b.Length)
        {
            string id = Encoding.ASCII.GetString(b, pos, 4);
            int len = BinaryPrimitives.ReadInt32LittleEndian(b.AsSpan(pos + 4));
            var body = b.AsSpan(pos + 8, Math.Min(len, b.Length - pos - 8));
            if (id == "fmt ")
                fmtOk = BinaryPrimitives.ReadInt16LittleEndian(body) == 1 && BinaryPrimitives.ReadInt16LittleEndian(body[2..]) == 1 &&
                        BinaryPrimitives.ReadInt32LittleEndian(body[4..]) == TanSoMau && BinaryPrimitives.ReadInt16LittleEndian(body[14..]) == 16;
            else if (id == "data")
            {
                if (!fmtOk) throw new InvalidDataException("cần WAV PCM 16 bit, mono, 16000 Hz");
                var m = new short[body.Length / 2];
                for (int i = 0; i < m.Length; i++) m[i] = BinaryPrimitives.ReadInt16LittleEndian(body[(2 * i)..]);
                return m;
            }
            pos += 8 + len + (len & 1);                              // khối có độ dài lẻ được đệm 1 byte
        }
        throw new InvalidDataException("không thấy khối data");
    }

    // ---------------------------------------------------------------- Kênh điều khiển JSON
    /// <summary>Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\n' (định khung theo dòng).</summary>
    public static void GuiJson(Socket s, JsonObject obj)
    {
        var b = Encoding.UTF8.GetBytes(obj.ToJsonString(Opt) + "\n");
        int sent = 0;
        while (sent < b.Length) sent += s.Send(b, sent, b.Length - sent, SocketFlags.None);
    }
}

/// <summary>Bộ đệm chống rung trễ (jitter buffer) cho một luồng RTP.
/// Gói UDP đến không đều (jitter), có thể mất hoặc đảo thứ tự. Bộ đệm giữ vài khung (doSau) trước khi bắt đầu
/// phát, rồi cứ mỗi 20 ms lấy đúng khung có số thứ tự kế tiếp:
///   - có khung      -> phát;
///   - thiếu khung nhưng đã có khung sau -> coi như mất, phát khoảng lặng (che lỗi đơn giản);
///   - bộ đệm rỗng   -> cạn (người gửi ngừng gửi hoặc mạng nghẽn), chờ nạp lại doSau khung;
///   - khung đến sau khi đã qua lượt phát -> trễ, bỏ.
/// Đồng thời thống kê theo RFC 3550: số gói mất (từ khoảng trống số thứ tự) và jitter
/// (trung bình trượt 1/16 của chênh lệch thời gian truyền giữa hai gói liên tiếp).</summary>
class BoDemJitter(int doSau = 3, int toiDa = 50)
{
    readonly SortedDictionary<long, byte[]> khung = [];   // số thứ tự mở rộng -> payload (có thứ tự: lấy nhỏ nhất dễ)
    long keTiep;                                          // số thứ tự sẽ phát tiếp
    bool dangPhat;
    long? seqLonNhat, seqDau;                             // số thứ tự mở rộng (không quay vòng) lớn nhất / đầu tiên
    public int Nhan, Tre, Thieu, Can, Tran;
    double jitter;                                        // đơn vị: mẫu (1 ms = 16 mẫu)
    double? truyenTruoc;

    /// <summary>Số thứ tự 16 bit quay vòng sau 65535; đổi thành số nguyên tăng mãi.</summary>
    long MoRong(int seq)
    {
        if (seqLonNhat is not long ln) return seq;
        long lech = (seq - ln) & 0xFFFF;
        return lech < 0x8000 ? ln + lech : ln - (0x10000 - lech);
    }

    public void NhanGoi(int seq, uint ts, byte[] payload, double lucDen)
    {
        long s = MoRong(seq);
        seqDau ??= s;
        if (seqLonNhat == null || s > seqLonNhat) seqLonNhat = s;
        Nhan++;
        double truyen = lucDen * Chung.TanSoMau - ts;     // thời gian truyền (lệch hằng số không quan trọng)
        if (truyenTruoc is double tt)
        {
            double d = Math.Abs(truyen - tt);
            if (d < Chung.TanSoMau) jitter += (d - jitter) / 16;   // bỏ qua bước nhảy lớn (nhãn thời gian quay vòng)
        }
        truyenTruoc = truyen;
        if (dangPhat && s < keTiep)
        {
            Tre++;                                        // đến quá muộn, đã qua lượt phát
            return;
        }
        khung[s] = payload;
        if (khung.Count > toiDa)                          // tràn: bỏ khung cũ nhất
        {
            khung.Remove(khung.Keys.First());
            Tran++;
        }
    }

    /// <summary>Gọi mỗi 20 ms; trả về payload của khung cần phát hoặc null (phát khoảng lặng).</summary>
    public byte[]? Lay()
    {
        if (!dangPhat)
        {
            if (khung.Count < doSau) return null;
            dangPhat = true;
            keTiep = khung.Keys.First();
        }
        if (khung.Remove(keTiep, out var p))
        {
            keTiep++;
            return p;
        }
        if (khung.Count == 0)                             // cạn bộ đệm: chờ nạp lại
        {
            dangPhat = false;
            Can++;
            return null;
        }
        Thieu++;                                          // mất khung này, đã có khung sau
        keTiep++;
        return null;
    }

    public void Xoa()
    {
        khung.Clear();
        dangPhat = false;
    }

    /// <summary>Số gói mất theo RFC 3550: số gói mong đợi (từ số thứ tự) trừ số gói đã nhận.</summary>
    public long Mat => seqDau is long d ? Math.Max(0, seqLonNhat!.Value - d + 1 - Nhan) : 0;

    public double JitterMs => jitter / (Chung.TanSoMau / 1000.0);
}
