using System.Buffers.Binary;
using System.Diagnostics;
using System.Globalization;
using System.IO.Compression;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR6_5;

// Bài mở rộng MR6.5 – Các thủ tục dùng chung cho máy chủ, máy phát và người xem (phát trực tiếp 1→n).
//
// Tự chứa (không dùng mã của bài khác): chép và mở rộng phần cần thiết của MR6.2 (âm thanh RTP, bộ đệm
// jitter) và MR6.3 (header video, chia mảnh, ghép khung, phông chữ 5×7, PNG viết tay).
//
// Hai loại datagram trên CÙNG một socket UDP, phân biệt bằng byte đầu tiên:
//   - 0x80: gói RTP âm thanh (RFC 3550): V=2, P=0, X=0, CC=0 -> byte đầu luôn là 0x80;
//   - 0x01: gói video của bài (phiên bản 1): loại 2 = giữ kết nối, 3 = mảnh khung I, 4 = mảnh khung P.
//
// Nén liên khung (inter-frame) đơn giản – ý tưởng của mọi codec video (H.264, VP8, AV1):
//   - khung khóa I (intra): nén một mình  ->  I = zlib(rgb thô);
//   - khung sai khác P (predicted): chỉ mã hóa phần THAY ĐỔI so với khung ngay trước
//         P = zlib(XOR từng byte(khung hiện tại, khung trước))
//     cảnh tĩnh -> XOR gần như toàn 0 -> nén còn vài trăm byte. Giải mã P cần đúng khung trước:
//     mất một khung là hỏng cả chuỗi tới khung I kế tiếp (nhóm khung – GOP, Group Of Pictures).
static class Chung
{
    // ---------------------------------------------------------------- Âm thanh (như MR6.2)
    public const int TanSoMau = 16000;                              // mẫu/giây
    public const int KhungMs = 20;                                  // một gói âm thanh = 20 ms
    public const int MauMoiKhung = TanSoMau * KhungMs / 1000;       // 320 mẫu
    public const int ByteMoiKhung = MauMoiKhung * 2;                // 640 byte (L16)
    public const int PtAmThanh = 96;        // payload type động (RFC 3551) cho L16/16000/mono
    public const int RtpSize = 12;          // header RTP: V|P|X|CC, M|PT, seq, ts, ssrc
    public const byte DauRtp = 0x80;

    // ---------------------------------------------------------------- Video (như MR6.3)
    public const int Rong = 160, Cao = 120;
    public const int FpsMacDinh = 10, GopMacDinh = 20;
    public const int MaxDatagram = 1200, HeaderSize = 20;
    public const int DuLieuToiDa = MaxDatagram - HeaderSize;        // 1180 byte
    public const byte PhienBan = 1;                                 // cũng là byte đầu của gói video (0x01)
    public const byte LoaiGiu = 2, LoaiI = 3, LoaiP = 4;
    public const int ManhToiDa = 256;

    // ---------------------------------------------------------------- Giới hạn, thời gian chờ
    public const int GioiHanDong = 64 * 1024;       // một dòng JSON điều khiển tối đa 64 KB
    public const int BoDemGopToiDa = 4 << 20;       // bộ đệm GOP ở server tối đa 4 MB
    public const double PliServerS = 0.5;           // server chuyển xin_khung_khoa tối đa 1 lần / 500 ms
    public const double PliXemS = 1.0;              // người xem gửi xin_khung_khoa tối đa 1 lần / 1 s
    public const double ChoThamChieuS = 0.2;        // chờ khung thiếu trước khi coi là mất tham chiếu
    public const double ChoManhS = 0.5;             // chờ đủ mảnh của một khung
    public const int HangHienThi = 30;              // hàng khung chờ hiển thị tối đa 30 khung
    public const int DoSauMacDinh = 5;              // bộ đệm jitter 5 khung = 100 ms

    static readonly Stopwatch clock = Stopwatch.StartNew();
    /// <summary>Đồng hồ đơn điệu (giây) – như time.monotonic() của Python.</summary>
    public static double Now => clock.Elapsed.TotalSeconds;
    /// <summary>Giờ thực (ms Unix, có phần lẻ) – như time.time() * 1000.</summary>
    public static double GioUnixMs => (DateTime.UtcNow - DateTime.UnixEpoch).TotalMilliseconds;
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static string F(double v, string fmt) => v.ToString(fmt, CultureInfo.InvariantCulture);

    /// <summary>Số ms để in: "–" nếu chưa có; dau = true in dấu +/− (như f"{x:+.0f}").</summary>
    public static string ChuMs(double? x, bool dau = false) =>
        x is not { } v ? "–" : dau ? (v < 0 || double.IsNegative(v) ? "-" : "+") + F(Math.Abs(v), "F0") : F(v, "F0");

    /// <summary>Ngủ tới khi hết `giay` giây (làm tròn lên mili giây, tối thiểu 1 ms để không quay vòng bận).</summary>
    public static void Ngu(double giay) => Thread.Sleep(Math.Max(1, (int)Math.Ceiling(giay * 1000)));

    // ================================================================ RTP âm thanh
    /// <summary>Header RTP 12 byte: 0x80 (V=2), PT=96 (M=0), số thứ tự, nhãn thời gian (mẫu), SSRC.</summary>
    public static byte[] DongGoiRtp(long seq, long ts, uint ssrc, byte[] payload)
    {
        var g = new byte[RtpSize + payload.Length];
        g[0] = DauRtp;
        g[1] = PtAmThanh;
        BinaryPrimitives.WriteUInt16BigEndian(g.AsSpan(2), (ushort)(seq & 0xFFFF));
        BinaryPrimitives.WriteUInt32BigEndian(g.AsSpan(4), (uint)(ts & 0xFFFFFFFF));
        BinaryPrimitives.WriteUInt32BigEndian(g.AsSpan(8), ssrc);
        payload.CopyTo(g, RtpSize);
        return g;
    }

    /// <summary>Trả về (pt, seq, ts, ssrc, payload) hoặc null nếu sai dạng.</summary>
    public static (int Pt, int Seq, uint Ts, uint Ssrc, byte[] Payload)? TachRtp(ReadOnlySpan<byte> g)
    {
        if (g.Length < RtpSize || g[0] >> 6 != 2) return null;
        return (g[1] & 0x7F, BinaryPrimitives.ReadUInt16BigEndian(g[2..]), BinaryPrimitives.ReadUInt32BigEndian(g[4..]),
                BinaryPrimitives.ReadUInt32BigEndian(g[8..]), g[RtpSize..].ToArray());
    }

    /// <summary>L16 truyền theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11.</summary>
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

    /// <summary>Mức RMS theo dBFS (0 dBFS = toàn thang 32768).</summary>
    public static double MucDbfs(short[]? mau)
    {
        if (mau == null || mau.Length == 0) return double.NegativeInfinity;
        double tong = 0;
        foreach (var x in mau) tong += (double)x * x;
        double rms = Math.Sqrt(tong / mau.Length);
        return rms > 0 ? 20 * Math.Log10(rms / 32768) : double.NegativeInfinity;
    }

    // ================================================================ Gói video
    /// <summary>Chia dữ liệu một khung (I hoặc P) thành datagram ≤ 1200 byte. Header (big-endian, 20 byte):
    /// phiên bản 1, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao, nhãn ms.</summary>
    public static List<byte[]> ChiaManh(byte loai, int ma, long soKhung, int rong, int cao, long tsMs, byte[] duLieu)
    {
        int tong = Math.Max(1, (duLieu.Length + DuLieuToiDa - 1) / DuLieuToiDa);
        if (tong > ManhToiDa) throw new InvalidOperationException("khung quá lớn");
        var ds = new List<byte[]>(tong);
        for (int i = 0; i < tong; i++)
        {
            int off = i * DuLieuToiDa, n = Math.Max(0, Math.Min(DuLieuToiDa, duLieu.Length - off));
            var g = new byte[HeaderSize + n];
            VietHeader(g, loai, (ushort)ma, (uint)(soKhung & 0xFFFFFFFF), (ushort)i, (ushort)tong, (ushort)rong, (ushort)cao,
                       (uint)(tsMs & 0xFFFFFFFF));
            if (n > 0) Array.Copy(duLieu, off, g, HeaderSize, n);
            ds.Add(g);
        }
        return ds;
    }

    static void VietHeader(byte[] g, byte loai, ushort ma, uint so, ushort i, ushort tong, ushort w, ushort h, uint ts)
    {
        var s = g.AsSpan();
        s[0] = PhienBan;
        s[1] = loai;
        BinaryPrimitives.WriteUInt16BigEndian(s[2..], ma);
        BinaryPrimitives.WriteUInt32BigEndian(s[4..], so);
        BinaryPrimitives.WriteUInt16BigEndian(s[8..], i);
        BinaryPrimitives.WriteUInt16BigEndian(s[10..], tong);
        BinaryPrimitives.WriteUInt16BigEndian(s[12..], w);
        BinaryPrimitives.WriteUInt16BigEndian(s[14..], h);
        BinaryPrimitives.WriteUInt32BigEndian(s[16..], ts);
    }

    /// <summary>Gói giữ kết nối / đăng ký địa chỉ UDP: loại 2, các trường khác 0.</summary>
    public static byte[] GoiGiu(int ma)
    {
        var g = new byte[HeaderSize];
        VietHeader(g, LoaiGiu, (ushort)ma, 0, 0, 0, 0, 0, 0);
        return g;
    }

    public record Header(byte Loai, int Ma, long So, int I, int Tong, int W, int H, long Ts, byte[] Payload);

    /// <summary>Tách header; trả null nếu sai dạng.</summary>
    public static Header? TachHeader(ReadOnlySpan<byte> g)
    {
        if (g.Length < HeaderSize || g.Length > MaxDatagram) return null;
        byte pb = g[0], loai = g[1];
        int i = BinaryPrimitives.ReadUInt16BigEndian(g[8..]), tong = BinaryPrimitives.ReadUInt16BigEndian(g[10..]);
        int w = BinaryPrimitives.ReadUInt16BigEndian(g[12..]), h = BinaryPrimitives.ReadUInt16BigEndian(g[14..]);
        if (pb != PhienBan || loai is not (LoaiGiu or LoaiI or LoaiP)) return null;
        if (loai != LoaiGiu && (tong == 0 || tong > ManhToiDa || i >= tong || w == 0 || h == 0)) return null;
        return new Header(loai, BinaryPrimitives.ReadUInt16BigEndian(g[2..]), BinaryPrimitives.ReadUInt32BigEndian(g[4..]), i, tong,
                          w, h, BinaryPrimitives.ReadUInt32BigEndian(g[16..]), g[HeaderSize..].ToArray());
    }

    // ================================================================ Mã hóa / giải mã I, P
    /// <summary>XOR từng byte hai khung cùng kích thước.</summary>
    public static byte[] XorKhung(byte[] a, byte[] b)
    {
        var ra = new byte[a.Length];
        var va = System.Runtime.InteropServices.MemoryMarshal.Cast<byte, ulong>(a.AsSpan());
        var vb = System.Runtime.InteropServices.MemoryMarshal.Cast<byte, ulong>(b.AsSpan());
        var vr = System.Runtime.InteropServices.MemoryMarshal.Cast<byte, ulong>(ra.AsSpan());
        for (int i = 0; i < vr.Length; i++) vr[i] = va[i] ^ vb[i];      // 8 byte một lần
        for (int i = vr.Length * 8; i < ra.Length; i++) ra[i] = (byte)(a[i] ^ b[i]);
        return ra;
    }

    /// <summary>Nén zlib (RFC 1950) với mức nén 0–9 như zlib.compress(dữ_liệu, mức) của Python.</summary>
    public static byte[] Nen(byte[] du, int muc = 6)
    {
        var ms = new MemoryStream();
        using (var z = new ZLibStream(ms, new ZLibCompressionOptions { CompressionLevel = muc })) z.Write(du);
        return ms.ToArray();
    }

    /// <summary>Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén").</summary>
    public static byte[] GiaiNen(byte[] du, int toiDa = 4 << 20)
    {
        using var z = new ZLibStream(new MemoryStream(du), CompressionMode.Decompress);
        var ra = new MemoryStream();
        var buf = new byte[16384];
        int n;
        while ((n = z.Read(buf)) > 0)
        {
            ra.Write(buf, 0, n);
            if (ra.Length > toiDa) throw new InvalidDataException("dữ liệu giải nén quá lớn");
        }
        return ra.ToArray();
    }

    public static byte[] MaHoaI(byte[] anh) => Nen(anh, 6);
    public static byte[] MaHoaP(byte[] anh, byte[] truoc) => Nen(XorKhung(anh, truoc), 6);

    // ================================================================ Ảnh RGB, phông 5×7, PNG
    // Phông chữ bitmap 5×7: mỗi ký tự 7 hàng, mỗi hàng 5 bit (bit cao = điểm bên trái), viết hex.
    static readonly Dictionary<char, string> Phong = new()
    {
        ['A'] = "0E11111F111111", ['B'] = "1E11111E11111E", ['C'] = "0E11101010110E", ['D'] = "1E11111111111E",
        ['E'] = "1F10101E10101F", ['F'] = "1F10101E101010", ['G'] = "0E11101711110F",
        ['H'] = "1111111F111111", ['I'] = "0E04040404040E", ['J'] = "0702020202120C",
        ['K'] = "11121418141211", ['L'] = "1010101010101F", ['M'] = "111B1515111111", ['N'] = "11111915131111",
        ['O'] = "0E11111111110E", ['P'] = "1E11111E101010", ['Q'] = "0E11111115120D", ['R'] = "1E11111E141211",
        ['S'] = "0F10100E01011E", ['T'] = "1F040404040404", ['U'] = "1111111111110E", ['V'] = "11111111110A04",
        ['W'] = "1111111515150A", ['X'] = "11110A040A1111", ['Y'] = "1111110A040404", ['Z'] = "1F01020408101F",
        ['0'] = "0E11131519110E", ['1'] = "040C040404040E", ['2'] = "0E11010204081F", ['3'] = "1F02040201110E",
        ['4'] = "02060A121F0202", ['5'] = "1F101E0101110E", ['6'] = "0608101E11110E", ['7'] = "1F010204080808",
        ['8'] = "0E11110E11110E", ['9'] = "0E11110F01020C", ['#'] = "0A0A1F0A1F0A0A", ['/'] = "01010204081010",
        [':'] = "000C0C000C0C00", [';'] = "000C0C000C0408", ['.'] = "00000000000C0C", ['-'] = "0000001F000000",
        ['('] = "02040808080402", [')'] = "08040202020408", ['='] = "00001F001F0000", [','] = "000000000C0408",
        ['+'] = "0004041F040400", ['?'] = "0E110102040004",
        [' '] = "00000000000000",
    };

    /// <summary>Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b); phần vượt ra ngoài ảnh bị cắt.</summary>
    public static void ToMau(byte[] anh, int w, int x, int y, int rw, int rh, (byte R, byte G, byte B) mau)
    {
        int h = anh.Length / (w * 3);
        for (int yy = Math.Max(0, y); yy < Math.Min(y + rh, h); yy++)
            for (int xx = Math.Max(0, x); xx < Math.Min(x + rw, w); xx++)
            {
                int i = (yy * w + xx) * 3;
                anh[i] = mau.R;
                anh[i + 1] = mau.G;
                anh[i + 2] = mau.B;
            }
    }

    /// <summary>Vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng `co` lần.</summary>
    public static void VeChu(byte[] anh, int w, int h, int x, int y, string chu, int co = 1, (byte, byte, byte)? mau = null)
    {
        var m = mau ?? (255, 255, 255);
        foreach (char c in chu.ToUpperInvariant())
        {
            string ma = Phong.GetValueOrDefault(c, Phong['?']);
            for (int hang = 0; hang < 7; hang++)
            {
                int bits = Convert.ToInt32(ma.Substring(2 * hang, 2), 16);
                for (int cot = 0; cot < 5; cot++)
                {
                    if ((bits & (0x10 >> cot)) == 0) continue;
                    int px = x + cot * co, py = y + hang * co;
                    if (px + co <= w && py + co <= h) ToMau(anh, w, px, py, co, co, m);
                }
            }
            x += 6 * co;
        }
    }

    /// <summary>Phóng to k lần bằng lặp điểm gần nhất.</summary>
    public static byte[] PhongTo(byte[] anh, int w, int h, int k)
    {
        if (k == 1) return (byte[])anh.Clone();
        var ra = new byte[w * k * h * k * 3];
        for (int y = 0; y < h * k; y++)
            for (int x = 0; x < w * k; x++)
                Array.Copy(anh, ((y / k) * w + x / k) * 3, ra, (y * w * k + x) * 3, 3);
        return ra;
    }

    static readonly uint[] CrcTable = Enumerable.Range(0, 256).Select(n =>
    {
        uint c = (uint)n;
        for (int k = 0; k < 8; k++) c = (c & 1) != 0 ? 0xEDB88320 ^ (c >> 1) : c >> 1;
        return c;
    }).ToArray();

    /// <summary>CRC-32 (đa thức 0xEDB88320, giống zlib.crc32) – BCL không có sẵn (System.IO.Hashing là gói NuGet).</summary>
    static uint Crc32(ReadOnlySpan<byte> d)
    {
        uint c = 0xFFFFFFFF;
        foreach (byte b in d) c = CrcTable[(c ^ b) & 0xFF] ^ (c >> 8);
        return c ^ 0xFFFFFFFF;
    }

    /// <summary>PNG viết tay: chữ ký 8 byte + khối IHDR, IDAT (zlib các hàng, byte lọc 0), IEND; mỗi khối CRC-32.</summary>
    public static void GhiPng(string path, byte[] anh, int w, int h)
    {
        void Khoi(Stream s, string kieu, byte[] du)
        {
            var kd = Encoding.ASCII.GetBytes(kieu).Concat(du).ToArray();
            var b4 = new byte[4];
            BinaryPrimitives.WriteUInt32BigEndian(b4, (uint)du.Length);
            s.Write(b4);
            s.Write(kd);
            BinaryPrimitives.WriteUInt32BigEndian(b4, Crc32(kd));
            s.Write(b4);
        }
        var hang = new byte[(w * 3 + 1) * h];
        for (int y = 0; y < h; y++) Array.Copy(anh, y * w * 3, hang, y * (w * 3 + 1) + 1, w * 3);
        var ihdr = new byte[13];
        BinaryPrimitives.WriteUInt32BigEndian(ihdr, (uint)w);
        BinaryPrimitives.WriteUInt32BigEndian(ihdr.AsSpan(4), (uint)h);
        ihdr[8] = 8;                                              // 8 bit/kênh
        ihdr[9] = 2;                                              // kiểu màu 2 = RGB
        using var f = File.Create(path);
        f.Write([0x89, (byte)'P', (byte)'N', (byte)'G', 13, 10, 26, 10]);
        Khoi(f, "IHDR", ihdr);
        Khoi(f, "IDAT", Nen(hang, 9));
        Khoi(f, "IEND", []);
    }

    /// <summary>Ghép các ảnh (rộng, cao, rgb, nhãn) thành một PNG: `cot` cột, nhãn dưới mỗi ảnh.</summary>
    public static (int W, int H) GhepAnh(string path, List<(int W, int H, byte[] Rgb, string Nhan)> ds, int cot = 2)
    {
        int oW = ds.Max(a => a.W) + 16;
        var caoHang = new List<int>();
        for (int i = 0; i < ds.Count; i += cot) caoHang.Add(ds.Skip(i).Take(cot).Max(a => a.H) + 30);
        int W = Math.Min(cot, ds.Count) * oW + 16, H = caoHang.Sum() + 8;
        var anh = new byte[W * H * 3];
        Array.Fill(anh, (byte)0xFF);
        for (int n = 0; n < ds.Count; n++)
        {
            var (w, h, rgb, nhan) = ds[n];
            int x0 = 16 + (n % cot) * oW, y0 = 8 + caoHang.Take(n / cot).Sum();
            for (int y = 0; y < h; y++) Array.Copy(rgb, y * w * 3, anh, ((y0 + y) * W + x0) * 3, w * 3);
            VeChu(anh, W, H, x0, y0 + h + 8, nhan.Length > oW / 12 ? nhan[..(oW / 12)] : nhan, 2, (0, 0, 0));
        }
        GhiPng(path, anh, W, H);
        return (W, H);
    }

    // ================================================================ Kênh điều khiển JSON theo dòng
    /// <summary>Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n'.</summary>
    public static void GuiJson(Socket s, JsonObject obj)
    {
        var b = Encoding.UTF8.GetBytes(obj.ToJsonString(Opt) + "\n");
        int sent = 0;
        while (sent < b.Length) sent += s.Send(b, sent, b.Length - sent, SocketFlags.None);
    }

    /// <summary>Đọc chuỗi trong đối tượng JSON (null nếu không có hoặc sai kiểu).</summary>
    public static string? Chuoi(JsonObject o, string k) =>
        o[k] is JsonValue v && v.TryGetValue<string>(out var s) ? s : null;

    /// <summary>Đọc số (nguyên hoặc thực) trong đối tượng JSON.</summary>
    public static double? So(JsonObject o, string k) =>
        o[k] is JsonValue v && v.TryGetValue<double>(out var d) ? d : null;

    public static bool? Bool(JsonObject o, string k) =>
        o[k] is JsonValue v && v.TryGetValue<bool>(out var b) ? b : null;

    public static bool TenHopLe(string? ten)
    {
        if (ten == null || ten.Trim() != ten) return false;
        int n = 0;
        foreach (var r in ten.EnumerateRunes())
        {
            n++;
            var loai = Rune.GetUnicodeCategory(r);        // như str.isprintable() của Python
            if (loai is UnicodeCategory.Control or UnicodeCategory.Format or UnicodeCategory.Surrogate or UnicodeCategory.PrivateUse
                or UnicodeCategory.OtherNotAssigned or UnicodeCategory.LineSeparator or UnicodeCategory.ParagraphSeparator
                || (loai == UnicodeCategory.SpaceSeparator && r.Value != ' ')) return false;
        }
        return n is >= 1 and <= 16;
    }

    /// <summary>Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu).</summary>
    public static void DocLenhStdin(double t0, Action<string> xuLy, Action? ketThuc = null)
    {
        string? dong;
        while ((dong = Console.ReadLine()) != null)
        {
            dong = dong.Trim();
            if (dong.StartsWith('@'))
            {
                int sp = dong.IndexOf(' ');
                string moc = sp < 0 ? dong : dong[..sp];
                dong = sp < 0 ? "" : dong[(sp + 1)..].Trim();
                double cho = double.Parse(moc[1..], CultureInfo.InvariantCulture) - (Now - t0);
                if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
            }
            if (dong != "") xuLy(dong);
        }
        ketThuc?.Invoke();
    }

    /// <summary>Kết nối TCP tới host:cổng (ưu tiên IPv4), chờ tối đa msCho. Trả về socket và địa chỉ IP đã nối
    /// (socket UDP của client mở cùng họ địa chỉ và gửi tới đúng IP này). Lỗi -> IOException hoặc SocketException.</summary>
    public static (Socket S, System.Net.IPAddress Ip) KetNoiTcp(string host, int cong, int msCho = 5000)
    {
        var ds = System.Net.Dns.GetHostAddresses(host);
        var ip = ds.FirstOrDefault(a => a.AddressFamily == AddressFamily.InterNetwork) ?? ds.First();
        var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp) { NoDelay = true };
        try
        {
            if (!s.ConnectAsync(ip, cong).Wait(msCho)) throw new IOException("hết thời gian kết nối");
        }
        catch (AggregateException e)
        {
            s.Close();
            throw new IOException(e.InnerException?.Message ?? e.Message);
        }
        catch
        {
            s.Close();
            throw;
        }
        return (s, ip);
    }

    /// <summary>Tắt báo lỗi ConnectionReset của UDP trên Windows (ICMP "port unreachable" của một lần gửi trước
    /// làm lần nhận sau ném lỗi WSAECONNRESET – Linux không có hiện tượng này).</summary>
    public static void TatLoiUdpReset(Socket s)
    {
        if (!OperatingSystem.IsWindows()) return;
        const int SIO_UDP_CONNRESET = -1744830452;
        try { s.IOControl(SIO_UDP_CONNRESET, [0, 0, 0, 0], null); } catch (Exception e) when (e is SocketException or PlatformNotSupportedException) { }
    }
}

/// <summary>Đọc từng dòng JSON từ luồng TCP có giới hạn độ dài (như doc_dong của bản Python).
/// Trả về đối tượng, null khi kết nối đóng; dòng > 64 KB, dòng bị cắt hoặc sai dạng -> InvalidDataException.</summary>
sealed class DocDong(Stream s)
{
    readonly byte[] buf = new byte[8192];
    int dau, cuoi;
    readonly MemoryStream dong = new();

    public JsonObject? Doc()
    {
        dong.SetLength(0);
        while (true)
        {
            if (dau == cuoi)
            {
                dau = 0;
                cuoi = s.Read(buf, 0, buf.Length);
                if (cuoi <= 0)
                {
                    cuoi = 0;
                    if (dong.Length == 0) return null;
                    throw new InvalidDataException("dòng điều khiển bị cắt");
                }
            }
            int nl = Array.IndexOf(buf, (byte)'\n', dau, cuoi - dau);
            int het = nl < 0 ? cuoi : nl + 1;
            dong.Write(buf, dau, het - dau);
            dau = het;
            if (dong.Length > Chung.GioiHanDong) throw new InvalidDataException("dòng điều khiển quá dài");
            if (nl >= 0) break;
        }
        try
        {
            return JsonNode.Parse(dong.GetBuffer().AsSpan(0, (int)dong.Length)) as JsonObject
                   ?? throw new InvalidDataException("thông điệp không phải đối tượng JSON");
        }
        catch (JsonException e) { throw new InvalidDataException(e.Message); }
    }
}

/// <summary>Bộ đệm chống rung trễ (jitter buffer) cho luồng RTP – như MR6.2, thêm nhãn thời gian.
/// Giữ `DoSau` khung trước khi bắt đầu phát, rồi cứ 20 ms lấy đúng khung có số thứ tự kế tiếp:
///   - có khung  -> trả về (ts, payload);
///   - thiếu khung nhưng đã có khung sau -> mất: trả về (ts ước lượng, null) – phát im lặng,
///     đồng hồ phát vẫn tiến 20 ms;
///   - rỗng -> cạn: trả về null (đồng hồ phát dừng), chờ nạp lại `DoSau` khung.</summary>
sealed class BoDemJitter(int doSau = Chung.DoSauMacDinh, int toiDa = 60)
{
    public int DoSau = doSau;
    readonly SortedDictionary<long, (uint Ts, byte[] P)> khung = [];   // số thứ tự mở rộng -> (ts, payload)
    long keTiep;
    bool dangPhat;
    long? seqLonNhat, seqDau;
    uint tsCuoi;
    public int Nhan, Tre, Thieu, Can, Tran;

    /// <summary>Số thứ tự 16 bit quay vòng -> số nguyên tăng mãi.</summary>
    long MoRong(int seq)
    {
        if (seqLonNhat is not { } ln) return seq;
        long lech = (seq - ln) & 0xFFFF;
        return lech < 0x8000 ? ln + lech : ln - (0x10000 - lech);
    }

    public void NhanGoi(int seq, uint ts, byte[] payload)
    {
        long s = MoRong(seq);
        seqDau ??= s;
        if (seqLonNhat == null || s > seqLonNhat) seqLonNhat = s;
        Nhan++;
        if (dangPhat && s < keTiep)
        {
            Tre++;                                             // đến sau lượt phát của nó
            return;
        }
        khung[s] = (ts, payload);
        if (khung.Count > toiDa)
        {
            khung.Remove(khung.Keys.First());
            Tran++;
        }
    }

    /// <summary>Gọi mỗi 20 ms. Trả về (ts, payload|null) hoặc null (đang chờ nạp).</summary>
    public (uint Ts, byte[]? P)? Lay()
    {
        if (!dangPhat)
        {
            if (khung.Count < DoSau) return null;
            dangPhat = true;
            keTiep = khung.Keys.First();
        }
        if (khung.Remove(keTiep, out var k))
        {
            keTiep++;
            tsCuoi = k.Ts;
            return (k.Ts, k.P);
        }
        if (khung.Count == 0)
        {
            dangPhat = false;
            Can++;
            return null;
        }
        Thieu++;                                               // mất khung này
        keTiep++;
        tsCuoi = unchecked(tsCuoi + Chung.MauMoiKhung);
        return (tsCuoi, null);
    }

    /// <summary>Bỏ mọi khung đang giữ (khi tắt nhận âm thanh); lần sau nạp lại từ đầu.</summary>
    public void Xoa()
    {
        khung.Clear();
        dangPhat = false;
    }

    /// <summary>Đổi độ sâu khi đang chạy: tăng -> dừng phát để nạp thêm; giảm -> bỏ khung cũ nhất.</summary>
    public void DatDoSau(int n)
    {
        DoSau = n;
        if (khung.Count < n) dangPhat = false;
        while (khung.Count > n)
        {
            long s = khung.Keys.First();
            khung.Remove(s);
            if (dangPhat) keTiep = s + 1;
        }
    }

    /// <summary>Số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận.</summary>
    public long Mat => seqDau is { } d && seqLonNhat is { } ln ? Math.Max(0, ln - d + 1 - Nhan) : 0;
}

/// <summary>Ghép mảnh thành khung (I hoặc P). Khác MR6.3: KHÔNG bỏ khung cũ còn dở khi khung mới đủ,
/// vì khung P cần mọi khung trước nó – khung dở chỉ bị bỏ khi chờ quá `cho` giây.</summary>
sealed class GhepKhung(double cho = Chung.ChoManhS)
{
    public record Khung(byte Loai, long So, long Ts, int W, int H, byte[] DuLieu);
    sealed class Dang
    {
        public (byte Loai, int Tong, int W, int H, long Ts) Dau;
        public readonly Dictionary<int, byte[]> Manh = [];
        public double T;
    }
    readonly Dictionary<long, Dang> dang = [];
    HashSet<long> xong = [];                          // khung đã đủ hoặc đã bỏ (mảnh đến sau = trễ)
    long lonNhat = -1;
    public int ManhNhan, KhungDu, KhungHong, ManhMat, Tre;

    public void DonDep(double bayGio, long? tru = null)
    {
        foreach (var s in dang.Where(d => bayGio - d.Value.T > cho && d.Key != tru).Select(d => d.Key).ToList())
        {
            var d = dang[s];
            dang.Remove(s);
            KhungHong++;
            ManhMat += d.Dau.Tong - d.Manh.Count;
            xong.Add(s);
        }
    }

    /// <summary>Bỏ các khung đang ghép dở mà không tính là hỏng (khi tắt nhận video).</summary>
    public void BoDangDo()
    {
        xong.UnionWith(dang.Keys);
        dang.Clear();
    }

    /// <summary>Trả về khung khi vừa đủ mảnh, ngược lại null.</summary>
    public Khung? Them(byte loai, long so, int i, int tong, int w, int h, long ts, byte[] payload, double bayGio)
    {
        ManhNhan++;
        if (xong.Contains(so) || so < lonNhat - 1000)
        {
            Tre++;
            return null;
        }
        var dau = (loai, tong, w, h, ts);
        if (!dang.TryGetValue(so, out var k)) dang[so] = k = new Dang { Dau = dau, T = bayGio };
        if (k.Dau != dau) return null;                    // mâu thuẫn với mảnh trước: bỏ mảnh
        k.Manh[i] = payload;
        DonDep(bayGio, so);
        if (k.Manh.Count < tong) return null;
        dang.Remove(so);
        xong.Add(so);
        lonNhat = Math.Max(lonNhat, so);
        if (xong.Count > 3000) xong = xong.Where(s => s > lonNhat - 1000).ToHashSet();
        KhungDu++;
        var du = new byte[Enumerable.Range(0, tong).Sum(j => k.Manh[j].Length)];
        int off = 0;
        for (int j = 0; j < tong; j++)
        {
            k.Manh[j].CopyTo(du, off);
            off += k.Manh[j].Length;
        }
        return new Khung(loai, so, ts, w, h, du);
    }
}

/// <summary>Giải mã chuỗi I/P theo thứ tự số khung.
///   - I: giải nén -> thành khung tham chiếu (bỏ mọi khung cũ hơn đang chờ).
///   - P số k: giải được khi tham chiếu là k−1. Nếu khung k đủ mảnh mà tham chiếu &lt; k−1: giữ chờ khung thiếu
///     tối đa ChoThamChieuS; quá hạn -> MẤT THAM CHIẾU: đếm đứng hình, bỏ mọi P tới I kế tiếp, KiemTra() trả true
///     để người xem gửi xin_khung_khoa.
///   - Chưa từng có tham chiếu (vừa vào xem): chỉ chờ I, không tính đứng hình.</summary>
sealed class GiaiMaVideo(double cho = Chung.ChoThamChieuS)
{
    public record KhungGiai(long So, byte Loai, long Ts, int W, int H, byte[] Anh);
    public (long So, byte[] Anh, int W, int H)? ThamChieu;
    public bool MatThamChieu;
    public double? LucMat;
    public readonly SortedDictionary<long, (long Ts, int W, int H, byte[] DuLieu, double Luc)> Doi = [];
    public int SoI, SoP, BoP, DungHinh, Loi;

    /// <summary>Trả về danh sách khung vừa giải được.</summary>
    public List<KhungGiai> Them(GhepKhung.Khung k, double bayGio)
    {
        var ra = new List<KhungGiai>();
        if (k.Loai == Chung.LoaiI)
        {
            if (ThamChieu is { } tc0 && k.So <= tc0.So) return ra;      // I cũ hơn tham chiếu hiện tại
            byte[] anh;
            try { anh = Chung.GiaiNen(k.DuLieu); }
            catch (InvalidDataException)
            {
                Loi++;
                return ra;
            }
            if (anh.Length != k.W * k.H * 3)
            {
                Loi++;
                return ra;
            }
            ThamChieu = (k.So, anh, k.W, k.H);
            MatThamChieu = false;
            foreach (var s in Doi.Keys.Where(s => s <= k.So).ToList()) Doi.Remove(s);
            SoI++;
            ra.Add(new KhungGiai(k.So, Chung.LoaiI, k.Ts, k.W, k.H, anh));
            GiaiTiep(ra);
            return ra;
        }
        if (ThamChieu == null || MatThamChieu)
        {
            BoP++;                                                     // chưa có / mất tham chiếu: chờ I
            return ra;
        }
        if (k.So <= ThamChieu.Value.So) return ra;
        Doi[k.So] = (k.Ts, k.W, k.H, k.DuLieu, bayGio);
        GiaiTiep(ra);
        return ra;
    }

    void GiaiTiep(List<KhungGiai> ra)
    {
        while (ThamChieu is { } tc && Doi.Remove(tc.So + 1, out var v))
        {
            byte[] x;
            try { x = Chung.GiaiNen(v.DuLieu); }
            catch (InvalidDataException) { x = []; }
            if (x.Length != tc.Anh.Length || (v.W, v.H) != (tc.W, tc.H))
            {
                Loi++;                                                 // P hỏng: coi như thiếu khung này
                return;
            }
            var anh = Chung.XorKhung(x, tc.Anh);
            ThamChieu = (tc.So + 1, anh, v.W, v.H);
            SoP++;
            ra.Add(new KhungGiai(tc.So + 1, Chung.LoaiP, v.Ts, v.W, v.H, anh));
        }
    }

    /// <summary>True nếu vừa mất tham chiếu (khung thiếu không tới sau ChoThamChieuS).</summary>
    public bool KiemTra(double bayGio)
    {
        if (Doi.Count == 0) return false;
        if (bayGio - Doi.Values.Min(v => v.Luc) < cho) return false;
        MatThamChieu = true;
        LucMat = bayGio;
        DungHinh++;
        BoP += Doi.Count;
        Doi.Clear();
        return true;
    }
}

// ================================================================ Nguồn giả lập "bảng clapper"
/// <summary>Camera giả lập kiểu bảng clapper của nhà quay phim: nền tĩnh (sóng chéo + hạt nhiễu thưa như MR6.3),
/// một ô vuông chạy ngang (chuyển động nhỏ), đồng hồ phương tiện T=12.345 và số khung. Ở mỗi giây tròn
/// (ts % 1000 &lt; 1000/fps) cả khung CHỚP TRẮNG (giữ mờ vân nền) – cùng lúc tiếng bíp của NguonAmClapper:
/// nhìn hình, nghe tiếng là thấy lệch tiếng-hình.</summary>
sealed class NguonClapper
{
    static readonly (int R, int G, int B)[] BangMau = [(230, 120, 60), (70, 170, 90), (70, 120, 220), (200, 80, 180), (200, 190, 60)];
    readonly string ten;
    public int Fps;
    readonly byte[] nen = new byte[Chung.Rong * Chung.Cao * 3];
    readonly byte[] nenChop;

    public NguonClapper(string ten, int ma = 1, int fps = Chung.FpsMacDinh)
    {
        (this.ten, Fps) = (ten, fps);
        var mau = BangMau[((ma - 1) % BangMau.Length + BangMau.Length) % BangMau.Length];
        int[] m = [mau.R, mau.G, mau.B];
        var r = new Random(ma);
        for (int y = 0; y < Chung.Cao; y++)
            for (int x = 0; x < Chung.Rong; x++)
            {
                double k = 0.35 + 0.65 * (0.5 + 0.5 * Math.Sin(2 * Math.PI * (x / 40.0 + y / 60.0)));
                int hat = r.NextDouble() < 0.04 ? r.Next(24) : 0;            // hạt nhiễu
                int i = (y * Chung.Rong + x) * 3;
                for (int c = 0; c < 3; c++) nen[i + c] = (byte)Math.Min(255, (int)(m[c] * k) / 16 * 16 + hat);
            }
        // Khung chớp gần như trắng nhưng còn vân nền: khung I rơi vào lúc chớp vẫn có kích thước như khung I thường.
        nenChop = nen.Select(v => (byte)(255 - (255 - v) / 4)).ToArray();
    }

    public byte[] LayKhung(long so, long tsMs)
    {
        bool chop = tsMs % 1000 < 1000.0 / Fps;
        int x = 6 + (int)(tsMs / 10 % (Chung.Rong - 24));               // ô vuông chạy 100 điểm ảnh/giây
        byte[] anh;
        (byte, byte, byte) mauChu, nenChu;
        if (chop)                                                        // chớp trắng: v -> 255 − (255 − v)/4
        {
            anh = (byte[])nenChop.Clone();
            (mauChu, nenChu) = ((0, 0, 0), (255, 255, 255));
        }
        else
        {
            anh = (byte[])nen.Clone();
            (mauChu, nenChu) = ((255, 255, 255), (20, 20, 20));
            Chung.ToMau(anh, Chung.Rong, x, 52, 12, 12, (255, 230, 0));
        }
        string dongHo = $"T={tsMs / 1000}.{tsMs % 1000:D3}";
        Chung.ToMau(anh, Chung.Rong, 4, 4, 6 * 2 * dongHo.Length + 4, 18, nenChu);
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, 6, 6, dongHo, 2, mauChu);
        string nhan = $"#{so:D4}";
        Chung.ToMau(anh, Chung.Rong, 4, Chung.Cao - 22, 6 * 2 * nhan.Length + 4, 18, nenChu);
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, 6, Chung.Cao - 20, nhan, 2, mauChu);
        Chung.ToMau(anh, Chung.Rong, Chung.Rong - 30, Chung.Cao - 15, 27, 11, (200, 0, 0));
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, Chung.Rong - 28, Chung.Cao - 13, "LIVE", 1);
        string t3 = ten.Length > 3 ? ten[..3] : ten;
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, Chung.Rong - 6 * 2 * t3.Length - 4, 6, t3, 2, mauChu);
        return anh;
    }
}

/// <summary>Micro giả lập: âm 440 Hz biên độ thấp liên tục + tiếng "bíp" 1000 Hz dài 100 ms ở mỗi giây tròn của
/// đồng hồ phương tiện (mẫu thứ n: n % 16000 &lt; 1600). Mẫu đầu tiên = lúc bấm Phát.</summary>
class NguonAmClapper(double tanSo = 440.0, double bienDo = 0.08, double tanSoBip = 1000.0, double bienDoBip = 0.4)
{
    long n;
    readonly double w = 2 * Math.PI * tanSo / Chung.TanSoMau, a = bienDo * 32767;
    readonly double wb = 2 * Math.PI * tanSoBip / Chung.TanSoMau, ab = bienDoBip * 32767;

    public short[] LayKhung()
    {
        var ra = new short[Chung.MauMoiKhung];
        for (int i = 0; i < ra.Length; i++, n++)
        {
            double x = a * Math.Sin(w * n);
            if (n % Chung.TanSoMau < Chung.TanSoMau / 10) x += ab * Math.Sin(wb * n);
            ra[i] = (short)Math.Round(x);                                // làm tròn tới số chẵn như round() của Python
        }
        return ra;
    }
}
