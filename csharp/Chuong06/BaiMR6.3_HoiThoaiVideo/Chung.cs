using System.Buffers.Binary;
using System.Diagnostics;
using System.Globalization;
using System.IO.Compression;
using System.Net.Sockets;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR6_3;

// Các thủ tục dùng chung cho server và client hội thoại video.
// Khung hình (frame) là ma trận điểm ảnh; mỗi điểm RGB 3 byte. Khung 160×120:
//   - kích thước thô: 160 × 120 × 3 = 57 600 byte; ở 10 khung/giây (fps) cần ≈ 4,6 Mbit/s – quá lớn,
//     nên phải nén trước khi gửi;
//   - ở đây nén bằng zlib (ZLibStream của .NET, không mất dữ liệu); khung tổng hợp có "hạt nhiễu" giống
//     cảm biến camera nên nén còn khoảng 7 KB -> 10 fps × 7 KB × 8 ≈ 560 kbit/s;
//   - giảm độ phân giải 1/2 mỗi chiều -> số điểm ảnh còn 1/4, dung lượng giảm tương ứng.
// Một khung nén (vài KB) lớn hơn một gói UDP an toàn, nên được chia mảnh (fragmentation): mỗi datagram
// ≤ 1200 byte (mức QUIC – RFC 9000 – coi là an toàn), gồm header 20 byte + tối đa 1180 byte dữ liệu.
// Bên nhận ghép đủ mảnh mới giải nén được; mất một mảnh là hỏng cả khung (khung bị bỏ).
static class Chung
{
    public const int Rong = 160, Cao = 120;          // độ phân giải gốc của camera
    public const int MaxDatagram = 1200, HeaderSize = 20;
    public const int DuLieuToiDa = MaxDatagram - HeaderSize;   // 1180 byte
    public const byte PhienBan = 1;
    public const byte LoaiManh = 1, LoaiGiu = 2;      // mảnh khung hình; gói đăng ký/giữ kết nối
    public const int ManhToiDa = 256;                 // tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
    public const ushort MaServer = 0;
    static readonly Stopwatch clock = Stopwatch.StartNew();
    public static double Now => clock.Elapsed.TotalSeconds;
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static string F(double v, string fmt) => v.ToString(fmt, CultureInfo.InvariantCulture);

    // ---------------------------------------------------------------- Chia mảnh và ghép khung
    /// <summary>Header (big-endian): phiên bản(1), loại(1), mã nguồn(2), số khung(4), chỉ số mảnh(2),
    /// tổng số mảnh(2), rộng(2), cao(2), nhãn thời gian ms(4). Trả về danh sách datagram.</summary>
    public static List<byte[]> ChiaManh(ushort ma, long soKhung, int rong, int cao, byte[] duLieu)
    {
        int tong = Math.Max(1, (duLieu.Length + DuLieuToiDa - 1) / DuLieuToiDa);
        uint ts = (uint)((long)(Now * 1000) & 0xFFFFFFFF);
        var ds = new List<byte[]>();
        for (int i = 0; i < tong; i++)
        {
            int off = i * DuLieuToiDa, n = Math.Min(DuLieuToiDa, duLieu.Length - off);
            var g = new byte[HeaderSize + Math.Max(0, n)];
            VietHeader(g, LoaiManh, ma, (uint)soKhung, (ushort)i, (ushort)tong, (ushort)rong, (ushort)cao, ts);
            if (n > 0) Array.Copy(duLieu, off, g, HeaderSize, n);
            ds.Add(g);
        }
        return ds;
    }

    static void VietHeader(byte[] g, byte loai, ushort ma, uint so, ushort i, ushort tong, ushort w, ushort h, uint ts)
    {
        var s = g.AsSpan();
        s[0] = PhienBan; s[1] = loai;
        BinaryPrimitives.WriteUInt16BigEndian(s[2..], ma);
        BinaryPrimitives.WriteUInt32BigEndian(s[4..], so);
        BinaryPrimitives.WriteUInt16BigEndian(s[8..], i);
        BinaryPrimitives.WriteUInt16BigEndian(s[10..], tong);
        BinaryPrimitives.WriteUInt16BigEndian(s[12..], w);
        BinaryPrimitives.WriteUInt16BigEndian(s[14..], h);
        BinaryPrimitives.WriteUInt32BigEndian(s[16..], ts);
    }

    public static byte[] GoiGiu(ushort ma)
    {
        var g = new byte[HeaderSize];
        VietHeader(g, LoaiGiu, ma, 0, 0, 0, 0, 0, 0);
        return g;
    }

    public record Header(byte Loai, ushort Ma, uint So, int I, int Tong, int W, int H, byte[] Payload);

    /// <summary>Tách header; trả null nếu sai dạng.</summary>
    public static Header? TachHeader(ReadOnlySpan<byte> g)
    {
        if (g.Length < HeaderSize) return null;
        byte pb = g[0], loai = g[1];
        int i = BinaryPrimitives.ReadUInt16BigEndian(g[8..]), tong = BinaryPrimitives.ReadUInt16BigEndian(g[10..]);
        if (pb != PhienBan || (loai == LoaiManh && (tong == 0 || tong > ManhToiDa || i >= tong))) return null;
        return new Header(loai, BinaryPrimitives.ReadUInt16BigEndian(g[2..]), BinaryPrimitives.ReadUInt32BigEndian(g[4..]), i, tong,
                          BinaryPrimitives.ReadUInt16BigEndian(g[12..]), BinaryPrimitives.ReadUInt16BigEndian(g[14..]), g[HeaderSize..].ToArray());
    }

    // ---------------------------------------------------------------- Ảnh RGB thô (byte[])
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
        ['?'] = "0E110102040004", [' '] = "00000000000000",
    };

    /// <summary>Tô hình chữ nhật (x, y, rw, rh) bằng màu (r, g, b).</summary>
    public static void ToMau(byte[] anh, int w, int x, int y, int rw, int rh, (byte R, byte G, byte B) mau)
    {
        int h = anh.Length / (w * 3);
        for (int yy = y; yy < Math.Min(y + rh, h); yy++)          // cắt phần vượt ra ngoài ảnh
            for (int xx = x; xx < Math.Min(x + rw, w); xx++)
            {
                int i = (yy * w + xx) * 3;
                anh[i] = mau.R; anh[i + 1] = mau.G; anh[i + 2] = mau.B;
            }
    }

    /// <summary>Vẽ chuỗi chữ (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng co lần.</summary>
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

    /// <summary>Giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d (lấy mẫu thưa).</summary>
    public static (byte[] Anh, int W, int H) ThuNho(byte[] anh, int w, int h, int d)
    {
        if (d == 1) return ((byte[])anh.Clone(), w, h);
        int w2 = w / d, h2 = h / d;
        var ra = new byte[w2 * h2 * 3];
        for (int y = 0; y < h2; y++)
            for (int x = 0; x < w2; x++)
                Array.Copy(anh, ((y * d) * w + x * d) * 3, ra, (y * w2 + x) * 3, 3);
        return (ra, w2, h2);
    }

    /// <summary>Phóng to k lần bằng lặp điểm gần nhất (nearest neighbour): ảnh độ phân giải thấp thành ô vuông.</summary>
    public static byte[] PhongTo(byte[] anh, int w, int h, int k)
    {
        if (k == 1) return (byte[])anh.Clone();
        var ra = new byte[w * k * h * k * 3];
        for (int y = 0; y < h * k; y++)
            for (int x = 0; x < w * k; x++)
                Array.Copy(anh, ((y / k) * w + x / k) * 3, ra, (y * w * k + x) * 3, 3);
        return ra;
    }

    // ---------------------------------------------------------------- Nén zlib
    public static byte[] Nen(byte[] du, CompressionLevel muc = CompressionLevel.Fastest)
    {
        var ms = new MemoryStream();
        using (var z = new ZLibStream(ms, muc)) z.Write(du);
        return ms.ToArray();
    }

    /// <summary>Giải nén zlib có giới hạn kích thước đầu ra (chống "bom nén" từ client xấu).</summary>
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

    // ---------------------------------------------------------------- PNG viết tay (zlib + CRC-32)
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

    /// <summary>PNG = chữ ký 8 byte + các khối (độ dài, kiểu, dữ liệu, CRC-32): IHDR, IDAT, IEND.
    /// IDAT là zlib của các hàng ảnh, mỗi hàng bắt đầu bằng 1 byte bộ lọc (0 = không lọc).</summary>
    public static void GhiPng(string path, byte[] anh, int w, int h)
    {
        void Khoi(Stream s, string kieu, byte[] du)
        {
            var kd = Encoding.ASCII.GetBytes(kieu).Concat(du).ToArray();
            var b4 = new byte[4];
            BinaryPrimitives.WriteUInt32BigEndian(b4, (uint)du.Length); s.Write(b4);
            s.Write(kd);
            BinaryPrimitives.WriteUInt32BigEndian(b4, Crc32(kd)); s.Write(b4);
        }
        var hang = new byte[(w * 3 + 1) * h];
        for (int y = 0; y < h; y++) Array.Copy(anh, y * w * 3, hang, y * (w * 3 + 1) + 1, w * 3);
        var ihdr = new byte[13];
        BinaryPrimitives.WriteUInt32BigEndian(ihdr, (uint)w);
        BinaryPrimitives.WriteUInt32BigEndian(ihdr.AsSpan(4), (uint)h);
        ihdr[8] = 8; ihdr[9] = 2;                                 // 8 bit/kênh, kiểu màu 2 = RGB
        using var f = File.Create(path);
        f.Write([0x89, (byte)'P', (byte)'N', (byte)'G', 13, 10, 26, 10]);
        Khoi(f, "IHDR", ihdr);
        Khoi(f, "IDAT", Nen(hang, CompressionLevel.SmallestSize));
        Khoi(f, "IEND", []);
    }

    // ---------------------------------------------------------------- Kênh điều khiển JSON
    public static void GuiJson(Socket s, JsonObject obj)
    {
        var b = Encoding.UTF8.GetBytes(obj.ToJsonString(Opt) + "\n");
        int sent = 0;
        while (sent < b.Length) sent += s.Send(b, sent, b.Length - sent, SocketFlags.None);
    }
}

/// <summary>Ghép các mảnh thành khung cho một luồng (một nguồn gửi).
/// Mảnh có thể đến lệch thứ tự hoặc mất. Giữ các khung đang ghép dở; khi một khung đủ mảnh: trả về khung đó
/// và bỏ mọi khung cũ hơn còn dở (video trực tiếp chỉ cần khung mới nhất). Khung dở quá `cho` giây cũng bị bỏ.
/// Đếm: mảnh nhận, khung đủ, khung hỏng (thiếu mảnh), mảnh mất, khung mất hẳn, mảnh đến trễ.</summary>
class GhepKhung(double cho = 0.5)
{
    class Dang { public int Tong; public Dictionary<int, byte[]> Manh = []; public double T; }
    readonly Dictionary<long, Dang> dang = [];
    long? cuoi;                                       // số khung đã xong/bỏ gần nhất
    public int ManhNhan, KhungDu, KhungHong, ManhMat, KhungMatHan, Tre;

    void Bo(long so)
    {
        var k = dang[so];
        dang.Remove(so);
        KhungHong++;
        ManhMat += k.Tong - k.Manh.Count;
    }

    public (int W, int H, byte[] DuLieu)? Them(long so, int i, int tong, int w, int h, byte[] payload, double bayGio)
    {
        ManhNhan++;
        if (cuoi != null && so <= cuoi)
        {
            Tre++;                                    // thuộc khung đã xong hoặc đã bỏ
            return null;
        }
        if (!dang.TryGetValue(so, out var k)) dang[so] = k = new Dang { Tong = tong, T = bayGio };
        k.Manh[i] = payload;
        foreach (var s in dang.Where(d => bayGio - d.Value.T > cho && d.Key != so).Select(d => d.Key).ToList())
            Bo(s);                                    // chờ quá lâu
        if (k.Manh.Count < k.Tong) return null;
        var duLieu = Enumerable.Range(0, k.Tong).SelectMany(j => k.Manh[j]).ToArray();
        dang.Remove(so);
        var cu = dang.Keys.Where(s => s < so).ToList();
        foreach (var s in cu) Bo(s);                  // khung cũ còn dở: bỏ
        if (cuoi != null) KhungMatHan += (int)Math.Max(0, so - cuoi.Value - 1 - cu.Count);
        cuoi = so;
        KhungDu++;
        return (w, h, duLieu);
    }
}

/// <summary>Khung hình giả lập: nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm biến thật,
/// trượt ngang theo thời gian (chuyển động); vẽ tên client và số khung.</summary>
class CameraTongHop
{
    static readonly (int R, int G, int B)[] BangMau = [(230, 120, 60), (70, 170, 90), (70, 120, 220), (200, 80, 180), (200, 190, 60)];
    readonly string ten;
    readonly byte[] nen = new byte[Chung.Rong * Chung.Cao * 3];

    public CameraTongHop(string ten, int ma)
    {
        this.ten = ten;
        var mau = BangMau[(ma - 1) % BangMau.Length];
        int[] m = [mau.R, mau.G, mau.B];
        var r = new Random(ma);
        for (int y = 0; y < Chung.Cao; y++)
            for (int x = 0; x < Chung.Rong; x++)
            {
                double k = 0.35 + 0.65 * (0.5 + 0.5 * Math.Sin(2 * Math.PI * (x / 40.0 + y / 60.0)));
                int hat = r.NextDouble() < 0.04 ? r.Next(24) : 0;           // hạt nhiễu
                int i = (y * Chung.Rong + x) * 3;
                for (int c = 0; c < 3; c++) nen[i + c] = (byte)Math.Min(255, (int)(m[c] * k) / 16 * 16 + hat);
            }
    }

    public byte[] LayKhung(long so)
    {
        int rowBytes = Chung.Rong * 3, dich = (int)(so * 2 % Chung.Rong) * 3;   // trượt 2 điểm ảnh mỗi khung
        var anh = new byte[nen.Length];
        for (int y = 0; y < Chung.Cao; y++)
        {
            Array.Copy(nen, y * rowBytes + dich, anh, y * rowBytes, rowBytes - dich);
            Array.Copy(nen, y * rowBytes, anh, y * rowBytes + rowBytes - dich, dich);
        }
        Chung.ToMau(anh, Chung.Rong, 6, 6, 6 * 4 * ten.Length + 6, 7 * 4 + 6, (20, 20, 20));
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, 9, 9, ten, 4);
        string nhan = $"#{so:D4}";
        Chung.ToMau(anh, Chung.Rong, 6, Chung.Cao - 34, 6 * 2 * nhan.Length + 4, 7 * 2 + 4, (20, 20, 20));
        Chung.VeChu(anh, Chung.Rong, Chung.Cao, 8, Chung.Cao - 32, nhan, 2);
        return anh;
    }
}
