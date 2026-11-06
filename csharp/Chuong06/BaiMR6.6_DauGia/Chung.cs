using System.Collections.Concurrent;
using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace BaiMR6_6;

// Khung dùng chung cho nhóm bài mở rộng "tranh chấp tài nguyên" MR6.6–MR6.14 (bài đầu: MR6.6 Đấu giá).
//
// Tệp này GIỐNG NHAU ở các bài (chỉ khác dòng namespace); nghiệp vụ riêng của từng bài nằm ở MayChu.cs (lớp con
// của MayChuCoSo, lớp con của SoKiemToan) và phần "NGHIỆP VỤ" của MoPhong.cs. Bảng ô của KhoTranhChap là ConcurrentDictionary
// (thêm ô khi đang chạy, không chép cả bảng); SoKiemToan có CongSaiPham(). Gồm:
//   1. Giao thức: GuiJson, MaHoa, DocDong (mỗi dòng một JSON UTF-8 kết thúc "\n", tối đa 64 KB), TenHopLe,
//      DocLenhStdin ("@t lệnh"), DocThamSo ("khoa=gia_tri").
//   2. Đo đạc: PhanVi (theo hạng gần nhất), ThongKe (đếm yêu cầu, độ trễ xử lý µs, yc/s).
//   3. KhoTranhChap: kho các "ô" tài nguyên và BỐN cách xử lý tranh chấp khi n luồng cùng ghi:
//         khong_khoa  đọc – xử lý – ghi không đồng bộ        -> tranh chấp (race condition), mất cập nhật
//         khoa        khóa loại trừ (lock/Monitor) theo từng ô, giữ suốt đọc – xử lý – ghi
//         lac_quan    đọc (giá trị, phiên bản) -> xử lý ngoài khóa -> so sánh-và-đổi (CAS) dưới khóa ngắn
//                     -> phiên bản đã đổi thì thử lại (tối đa 5 lần, nghỉ ngẫu nhiên 0–1 ms)
//         hang_doi    mọi yêu cầu ghi xếp hàng cho MỘT luồng xử lý (mô hình actor / vòng lặp sự kiện)
//   4. SoKiemToan: sổ ghi mọi giao dịch được chấp nhận (khóa riêng) và đếm sai phạm theo luật của bài.
//   5. MayChuCoSo: máy chủ TCP đa luồng (một Thread mỗi kết nối): chào, quản trị, ma_yc lặp lại,
//      đẩy cap_nhat gộp tối đa 10 lần/giây; lớp con chỉ viết vài hàm nghiệp vụ.
//   6. Định dạng in: Tien(), DongThongKe(), BangSoSanh() – các bản ngôn ngữ in GIỐNG HỆT nhau.
//
// Giao thức (TCP, mỗi dòng một JSON) – giống hệt bản Python (chung.py):
//   client -> server  {"loai":"chao","ten":"An","vai_tro":"nguoi_dung"|"quan_tri"[,"so_nguoi":m]}  (đầu tiên)
//                     {"loai":"<thao_tac>","ma_yc":k,["nguoi":"u17"],...}   (yêu cầu nghiệp vụ)
//                     {"loai":"che_do","ma_yc":k,"gia_tri":"khoa"} | {"loai":"tre","ma_yc":k,"ms":2}
//                     {"loai":"dat_lai","ma_yc":k,...tham số} | {"loai":"thong_ke","ma_yc":k}  (chỉ quan_tri)
//   server -> client  {"loai":"chao","ma":id,"che_do":"khoa","may_chu":"..."} rồi ngay một cap_nhat
//                     {"loai":"kq","ma_yc":k,"ok":true|false,["ma_loi":"..","ly_do":".."],...}
//                     {"loai":"cap_nhat","che_do":"khoa",...trạng thái nghiệp vụ}  (gộp ≤ 10 lần/s)
//                     {"loai":"su_kien","noi_dung":"..."} ; {"loai":"thong_ke","ma_yc":k,...}
//                     {"loai":"loi","ly_do":"..."} (sai giao thức)
// "so_nguoi" = m: kết nối gộp của bộ mô phỏng chở m người dùng ảo; mỗi yêu cầu ghi tên người ở "nguoi".
// Một kết nối được gửi nhiều yêu cầu liên tiếp không chờ trả lời (pipelining) – trả lời ghép theo ma_yc.
// Gửi lại một ma_yc đã xử lý: máy chủ KHÔNG làm lại mà gửi lại kết quả cũ kèm "lap_lai": true (idempotency).
static class Chung
{
    public const int GioiHanDong = 64 * 1024;            // một dòng JSON tối đa 64 KB
    public static readonly string[] CheDo = ["khong_khoa", "khoa", "lac_quan", "hang_doi"];
    public static readonly Dictionary<string, string> TenCheDo = new()
    {
        ["khong_khoa"] = "Không khóa (lỗi tranh chấp)",
        ["khoa"] = "Khóa loại trừ (mutex)",
        ["lac_quan"] = "Lạc quan (phiên bản + CAS)",
        ["hang_doi"] = "Hàng đợi một luồng (actor)",
    };
    public static readonly string[] VaiTro = ["nguoi_dung", "quan_tri"];
    public const int SoLanThu = 5;                       // lac_quan: số lần thử tối đa
    public const double ChoKhoaToiDaS = 2.0;             // khoa: chờ một khóa quá 2 s thì bỏ (nghi khóa chết – deadlock)
    public const int NhoKq = 1024;                       // mỗi kết nối nhớ kết quả của 1024 ma_yc gần nhất
    public const int GiuDoTre = 200_000;                 // giữ tối đa 200 000 mẫu độ trễ cho phân vị
    public const double NhipDayS = 0.1;                  // đẩy cap_nhat tối đa 10 lần/giây

    static readonly Stopwatch dongHo = Stopwatch.StartNew();
    /// <summary>Đồng hồ đơn điệu (giây) – như time.monotonic() của Python.</summary>
    public static double Now => dongHo.Elapsed.TotalSeconds;
    static readonly JsonSerializerOptions Opt = new() { Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };
    public static readonly CultureInfo Ic = CultureInfo.InvariantCulture;

    public static string F(double v, string fmt) => v.ToString(fmt, Ic);

    /// <summary>Ngủ ms mili giây (có phần lẻ). Thread.Sleep chỉ nhận số nguyên ms nên phần lẻ được làm tròn ngẫu
    /// nhiên (1,3 ms -> 1 ms với xác suất 0,7, 2 ms với xác suất 0,3): trung bình đúng bằng ms.</summary>
    public static void NguMs(double ms)
    {
        if (ms <= 0) return;
        int nguyen = (int)Math.Floor(ms);
        if (Random.Shared.NextDouble() < ms - nguyen) nguyen++;
        Thread.Sleep(nguyen);
    }

    // ================================================================ 1. Giao thức
    /// <summary>Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n'.</summary>
    public static byte[] MaHoa(JsonObject obj) => Encoding.UTF8.GetBytes(obj.ToJsonString(Opt) + "\n");

    public static void GuiJson(Socket s, JsonObject obj) => GuiHet(s, MaHoa(obj));

    /// <summary>Gửi trọn mảng byte (như sendall).</summary>
    public static void GuiHet(Socket s, byte[] b)
    {
        int da = 0;
        while (da < b.Length) da += s.Send(b, da, b.Length - da, SocketFlags.None);
    }

    /// <summary>Tên 1–16 ký tự in được, không khoảng trắng đầu/cuối (như str.isprintable() của Python).</summary>
    public static bool TenHopLe(string? ten)
    {
        if (ten == null || ten.Trim() != ten) return false;
        int n = 0;
        foreach (var r in ten.EnumerateRunes())
        {
            n++;
            var loai = Rune.GetUnicodeCategory(r);
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
                double cho = double.Parse(moc[1..], Ic) - (Now - t0);
                if (cho > 0) Thread.Sleep(TimeSpan.FromSeconds(cho));
            }
            if (dong != "") xuLy(dong);
        }
        ketThuc?.Invoke();
    }

    /// <summary>["thoi_gian_s=5", "ten_mon=Bình"] -> {thoi_gian_s: 5 (long), ten_mon: "Bình"} (số nếu đọc được:
    /// thử số nguyên, rồi số thực, rồi để nguyên chuỗi – như int() rồi float() của Python).</summary>
    public static OrderedDictionary<string, object> DocThamSo(IEnumerable<string> ds)
    {
        var kq = new OrderedDictionary<string, object>();
        foreach (var muc in ds)
        {
            int i = muc.IndexOf('=');
            if (i < 0) throw new ArgumentException($"cần dạng khóa=giá_trị: {muc}");
            string k = muc[..i], v = muc[(i + 1)..];
            if (long.TryParse(v, NumberStyles.Integer, Ic, out var l)) kq[k] = l;
            else if (DocSoThuc(v, out var d)) kq[k] = d;
            else kq[k] = v;
        }
        return kq;
    }

    /// <summary>Như float() của Python: "2", "2.5", " 1e3 ", "inf"; chuỗi rỗng -> false.</summary>
    public static bool DocSoThuc(string s, out double d)
    {
        s = s.Trim();
        if (s.Equals("inf", StringComparison.OrdinalIgnoreCase) || s.Equals("+inf", StringComparison.OrdinalIgnoreCase)) { d = double.PositiveInfinity; return true; }
        if (s.Equals("-inf", StringComparison.OrdinalIgnoreCase)) { d = double.NegativeInfinity; return true; }
        if (s.Equals("nan", StringComparison.OrdinalIgnoreCase)) { d = double.NaN; return true; }
        return double.TryParse(s, NumberStyles.Float, Ic, out d) && s.Length > 0;
    }

    // ---------------------------------------------------------------- đọc giá trị trong JSON
    public static string? Chuoi(JsonObject? o, string k) =>
        o?[k] is JsonValue v && v.GetValueKind() == JsonValueKind.String ? v.GetValue<string>() : null;

    public static bool LaSo(JsonNode? n) => n is JsonValue v && v.GetValueKind() == JsonValueKind.Number;

    /// <summary>Số (nguyên hoặc thực) trong JSON – đọc được cả nút do JsonNode.Parse tạo lẫn nút tạo trong mã.</summary>
    public static double? SoD(JsonNode? n) => LaSo(n) ? double.Parse(n!.ToJsonString(), Ic) : null;

    public static double SoD(JsonObject? o, string k, double md = 0) => SoD(o?[k]) ?? md;

    /// <summary>Số nguyên JSON (như isinstance(x, int) và không phải bool của Python): "3" có, "3.0" không.</summary>
    public static bool LaSoNguyen(JsonNode? n, out long x)
    {
        x = 0;
        if (!LaSo(n)) return false;
        string t = n!.ToJsonString();
        return t.IndexOfAny(['.', 'e', 'E']) < 0 && long.TryParse(t, NumberStyles.AllowLeadingSign, Ic, out x);
    }

    /// <summary>Số nguyên (số thực thì cắt phần lẻ); không phải số -> md.</summary>
    public static long SoL(JsonObject? o, string k, long md = 0) => SoD(o?[k]) is { } d ? (long)d : md;

    public static long? SoLN(JsonNode? n) => SoD(n) is { } d ? (long)d : null;

    /// <summary>Giá trị đúng (như "if tin.get(k)" của Python cho trường kiểu bool).</summary>
    public static bool Dung(JsonObject? o, string k) =>
        o?[k] is JsonValue v && (v.GetValueKind() == JsonValueKind.True
                                 || (v.GetValueKind() == JsonValueKind.Number && SoD(v) != 0)
                                 || (v.GetValueKind() == JsonValueKind.String && v.GetValue<string>() != ""));

    public static bool LaNull(JsonNode? n) => n is null || n.GetValueKind() == JsonValueKind.Null;

    /// <summary>Chuỗi như str() của Python cho một giá trị JSON (None, True, 5, 2.5, "abc").</summary>
    public static string ChuoiPy(JsonNode? n) => n switch
    {
        null => "None",
        JsonValue v when v.GetValueKind() == JsonValueKind.String => v.GetValue<string>(),
        JsonValue v when v.GetValueKind() == JsonValueKind.True => "True",
        JsonValue v when v.GetValueKind() == JsonValueKind.False => "False",
        JsonValue v when v.GetValueKind() == JsonValueKind.Null => "None",
        _ => n.ToJsonString(Opt),
    };

    // ================================================================ 2. Đo đạc
    /// <summary>Phân vị p (0–100) theo HẠNG GẦN NHẤT: phần tử thứ ⌈p/100 · n⌉ (đếm từ 1) của dãy đã sắp xếp.
    /// Ví dụ n = 1000: p99 là phần tử thứ 990. Dãy rỗng -> null.</summary>
    public static double? PhanVi(IReadOnlyList<double> dsDaSapXep, double p)
    {
        if (dsDaSapXep.Count == 0) return null;
        return dsDaSapXep[Math.Max(1, (int)Math.Ceiling(p / 100 * dsDaSapXep.Count)) - 1];
    }

    // ================================================================ 6. Định dạng in (giống nhau ở 5 ngôn ngữ)
    /// <summary>5300000 -> "5.300.000 đ" (dấu chấm ngăn nghìn).</summary>
    public static string Tien(long x) => x.ToString("#,0", Ic).Replace(",", ".") + " đ";

    /// <summary>Như định dạng "{x:g}" của Python: 2.0 -> "2", 0.5 -> "0.5", 1e-05 -> "1e-05".</summary>
    public static string G(double x)
    {
        string s = x.ToString("G6", Ic);
        int e = s.IndexOf('E');
        if (e < 0) return s;
        string mu = s[(e + 1)..];                        // "+06" / "-05" -> giữ dấu và ít nhất 2 chữ số
        char dau = mu[0] == '-' ? '-' : '+';
        string so = mu.TrimStart('+', '-').TrimStart('0');
        return s[..e] + "e" + dau + so.PadLeft(2, '0');
    }

    /// <summary>Như str(x) của Python cho số thực: 5.0 -> "5.0", 0.25 -> "0.25".</summary>
    public static string ReprThuc(double x)
    {
        if (double.IsInfinity(x)) return x > 0 ? "inf" : "-inf";
        if (double.IsNaN(x)) return "nan";
        if (x == Math.Floor(x) && Math.Abs(x) < 1e16) return ((long)x).ToString(Ic) + ".0";
        return x.ToString("R", Ic).Replace("E", "e");
    }

    /// <summary>Như "{v}" của Python cho giá trị tham số (long, double, chuỗi).</summary>
    public static string Repr(object? v) => v switch
    {
        null => "None",
        double d => ReprThuc(d),
        bool b => b ? "True" : "False",
        IFormattable f => f.ToString(null, Ic),
        _ => v.ToString() ?? "",
    };

    public static string SoUs(JsonNode? n) => SoLN(n) is { } x ? x.ToString(Ic) : "–";

    static string So(JsonObject d, string k) => SoL(d, k).ToString(Ic);

    /// <summary>"Sai phạm 37: giá thấp hơn 36, giá cuối sai 1".</summary>
    public static string DongSaiPham(JsonObject tk) =>
        $"Sai phạm {So(tk, "sai_pham_tong")}: " + string.Join(", ", (tk["sai_pham"] as JsonArray ?? [])
            .OfType<JsonObject>().Select(s => $"{Chuoi(s, "ten")} {So(s, "so")}"));

    /// <summary>Các dòng (≤ 64 ký tự) in số liệu máy chủ – dùng ở server, khach, mo_phong.</summary>
    public static List<string> DongThongKe(JsonObject d) =>
    [
        $"Chế độ {Chuoi(d, "che_do")}, trễ xử lý 0–{G(SoD(d, "tre_ms"))} ms",
        $"Yêu cầu {So(d, "so_yc")}: thành công {So(d, "thanh_cong")}, từ chối {So(d, "tu_choi")}",
        $"Xung đột {So(d, "xung_dot")}, thử lại {So(d, "thu_lai")}, hết lượt thử {So(d, "het_lan_thu")}",
        $"Hàng đợi max {So(d, "hang_doi_max")}, chờ khóa quá hạn {So(d, "khoa_cho_lau")}",
        $"Xử lý µs: p50 {SoUs(d["p50_us"])} p95 {SoUs(d["p95_us"])} p99 {SoUs(d["p99_us"])}",
        $"Thông lượng {So(d, "thong_luong")} yc/s",
        DongSaiPham(d),
        Chuoi(d, "tom_tat") ?? "",
    ];

    public const string TieuDeSoSanh = "  Chế độ       Thành công Sai phạm Xung đột  yc/s  p99 ms";

    /// <summary>Các hàng của bảng so sánh 4 chế độ (≤ 64 ký tự/dòng).</summary>
    public static List<string> BangSoSanh(IEnumerable<HangSoSanh> hang)
    {
        var ds = new List<string> { TieuDeSoSanh };
        foreach (var h in hang)
        {
            string p99 = h.P99Ms is { } p ? F(p, "F1") : "–";
            ds.Add($"  {h.CheDo,-12} {h.ThanhCong,10} {h.SaiPham,8} {h.XungDot,8} {h.YcS,5} {p99,7}");
        }
        return ds;
    }

    /// <summary>Địa chỉ IP để lắng nghe/kết nối: chuỗi IP, hoặc tên máy (ưu tiên IPv4).</summary>
    public static IPAddress DiaChiIp(string host)
    {
        if (IPAddress.TryParse(host, out var ip)) return ip;
        var ds = Dns.GetHostAddresses(host);
        return ds.FirstOrDefault(a => a.AddressFamily == AddressFamily.InterNetwork) ?? ds.First();
    }
}

/// <summary>Đối số dòng lệnh kiểu argparse: các đối số vị trí và tùy chọn "--ten giá_trị"; tên trong coBat là cờ
/// không có giá trị (ví dụ "--so-sanh").</summary>
sealed class DoiSo
{
    public readonly List<string> ViTri = [];
    readonly Dictionary<string, string?> tuyChon = [];

    public DoiSo(string[] a, params string[] coBat)
    {
        for (int i = 0; i < a.Length; i++)
        {
            if (a[i].StartsWith("--"))
            {
                if (coBat.Contains(a[i])) tuyChon[a[i]] = null;
                else if (i + 1 < a.Length) tuyChon[a[i]] = a[++i];
                else throw new ArgumentException($"thiếu giá trị cho {a[i]}");
            }
            else ViTri.Add(a[i]);
        }
    }

    public string VT(int i, string md) => i < ViTri.Count ? ViTri[i] : md;
    public bool Co(string ten) => tuyChon.ContainsKey(ten);
    public string? Lay(string ten) => tuyChon.GetValueOrDefault(ten);
    public string Chuoi(string ten, string md) => Lay(ten) ?? md;
    public int Int(string ten, int md) => Lay(ten) is { } s ? int.Parse(s, Chung.Ic) : md;
    public long Long(string ten, long md) => Lay(ten) is { } s ? long.Parse(s, Chung.Ic) : md;
    public double Double(string ten, double md) => Lay(ten) is { } s ? double.Parse(s, Chung.Ic) : md;
    public double? DoubleN(string ten) => Lay(ten) is { } s ? double.Parse(s, Chung.Ic) : null;
}

/// <summary>In một dòng "[tiền_tố] text" – nhiều luồng cùng in: mỗi dòng in trọn vẹn.</summary>
static class InDong
{
    static readonly object khoa = new();

    public static void In(string tienTo, string text)
    {
        lock (khoa) Console.WriteLine($"[{tienTo}] {text}");
    }
}

/// <summary>Một hàng của bảng so sánh:"Thành công", yc/s, p99 đo ở bộ mô phỏng; "Sai phạm", "Xung đột" lấy từ
/// thong_ke của máy chủ.</summary>
sealed record HangSoSanh(string CheDo, long ThanhCong, long SaiPham, long XungDot, long YcS, double? P99Ms, long TuChoi,
                         string TomTat);

/// <summary>Windows: hẹn giờ mặc định 15,6 ms làm Thread.Sleep(1) ngủ ~15 ms; timeBeginPeriod(1) đưa về ~1 ms để độ
/// trễ giả lập 0–2 ms đúng như trên Linux (Linux: bỏ qua).</summary>
static partial class WinMM
{
    [LibraryImport("winmm.dll")]
    private static partial uint timeBeginPeriod(uint uPeriod);

    public static void TangDoChinhXacHenGio()
    {
        if (!OperatingSystem.IsWindows()) return;
        try { timeBeginPeriod(1); } catch (Exception e) when (e is DllNotFoundException or EntryPointNotFoundException) { }
    }
}

/// <summary>Đọc từng dòng JSON từ luồng TCP có giới hạn độ dài (như doc_dong của bản Python).
/// Trả về đối tượng, null khi kết nối đóng; dòng > 64 KB, dòng bị cắt hoặc sai dạng -> InvalidDataException.</summary>
sealed class DocDong(Stream s)
{
    readonly byte[] buf = new byte[16384];
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
                    throw new InvalidDataException("dòng quá dài hoặc bị cắt");
                }
            }
            int nl = Array.IndexOf(buf, (byte)'\n', dau, cuoi - dau);
            int het = nl < 0 ? cuoi : nl + 1;
            dong.Write(buf, dau, het - dau);
            dau = het;
            if (dong.Length > Chung.GioiHanDong) throw new InvalidDataException("dòng quá dài hoặc bị cắt");
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

// ================================================================ 2. Thống kê máy chủ
/// <summary>Số liệu của máy chủ từ lần đặt lại gần nhất. Mọi hàm an toàn đa luồng (một khóa nhỏ riêng).</summary>
sealed class ThongKe
{
    public static readonly string[] Dem = ["so_yc", "thanh_cong", "tu_choi", "xung_dot", "thu_lai", "het_lan_thu",
                                           "khoa_cho_lau", "hang_doi_max"];
    readonly object khoa = new();
    Dictionary<string, long> dem = [];
    Dictionary<string, long> maLoi = [];                 // lý do từ chối -> số lần
    List<double> treUs = [];                              // độ trễ xử lý từng yêu cầu (µs)
    double? tDau, tCuoi;
    long giay, demGiay, ycS;

    public ThongKe() => DatLai();

    public void DatLai()
    {
        lock (khoa)
        {
            dem = Dem.ToDictionary(x => x, _ => 0L);
            maLoi = [];
            treUs = [];
            tDau = tCuoi = null;
            (giay, demGiay, ycS) = ((long)Chung.Now, 0, 0);
        }
    }

    public void Cong(string ten, long n = 1)
    {
        lock (khoa) dem[ten] += n;
    }

    public void DatMax(string ten, long x)
    {
        lock (khoa) if (x > dem[ten]) dem[ten] = x;
    }

    public void BatDauYc()
    {
        lock (khoa)
        {
            dem["so_yc"]++;
            tDau ??= Chung.Now;
        }
    }

    public void XongYc(JsonObject kq, double treUsMoi)
    {
        double bayGio = Chung.Now;
        lock (khoa)
        {
            if (Chung.Dung(kq, "ok")) dem["thanh_cong"]++;
            else
            {
                dem["tu_choi"]++;
                string ma = Chung.Chuoi(kq, "ma_loi") ?? "khac";
                maLoi[ma] = maLoi.GetValueOrDefault(ma) + 1;
            }
            if (treUs.Count < Chung.GiuDoTre) treUs.Add(treUsMoi);
            tCuoi = bayGio;
            SangGiay(bayGio);
            demGiay++;
        }
    }

    /// <summary>yc_s = số yêu cầu xong trong giây (đồng hồ) liền trước.</summary>
    void SangGiay(double bayGio)
    {
        long g = (long)bayGio;
        if (g != giay)
        {
            ycS = g == giay + 1 ? demGiay : 0;
            (giay, demGiay) = (g, 0);
        }
    }

    public JsonObject AnhChup()
    {
        var d = new JsonObject();
        double[] tre;
        lock (khoa)
        {
            SangGiay(Chung.Now);
            foreach (var (k, v) in dem) d[k] = v;
            tre = [.. treUs];
            var ml = new JsonObject();
            foreach (var (k, v) in maLoi) ml[k] = v;
            d["ma_loi"] = ml;
            d["yc_s"] = ycS;
            long xong = dem["thanh_cong"] + dem["tu_choi"];
            double khoang = tDau is { } a && tCuoi is { } b ? b - a : 0;
            d["thong_luong"] = khoang > 0 ? (long)Math.Round(xong / khoang) : 0L;
        }
        Array.Sort(tre);
        foreach (int p in new[] { 50, 95, 99 })
            d[$"p{p}_us"] = Chung.PhanVi(tre, p) is { } v ? JsonValue.Create((long)Math.Round(v)) : null;
        return d;
    }
}

// ================================================================ 3. Kho tài nguyên + 4 chế độ
/// <summary>Kết quả của hàm nghiệp vụ thuần: Kq trả lời client; Ghi = null (không ghi) hoặc {khóa ô: GIÁ TRỊ MỚI}.</summary>
delegate (JsonObject Kq, Dictionary<string, object>? Ghi) XuLy(IReadOnlyDictionary<string, object> doc);

/// <summary>Kho các ô tài nguyên; mọi thao tác GHI đi qua ThucHien() và được xử lý theo chế độ hiện tại.
///
/// Nghiệp vụ viết hàm THUẦN  xuLy(doc) -> (kq, ghi):
///   - doc: {khóa ô: giá trị đọc được} cho các ô trong cacKhoa;
///   - kq:  JsonObject trả lời client ({"ok": true/false, "ma_loi", "ly_do", ...});
///   - ghi: null (không ghi) hoặc {khóa ô: GIÁ TRỊ MỚI} – chỉ các ô trong cacKhoa. Giá trị là record bất biến
///          (sửa bằng "with") – lac_quan có thể gọi xuLy nhiều lần.
/// khiGhi(kq) (tùy chọn) được gọi NGAY SAU khi ghi, vẫn trong vùng găng của chế độ (dưới khóa ô với khoa, dưới
/// khóa CAS với lac_quan, trong luồng xử lý với hang_doi): nơi ghi sổ kiểm toán đúng thứ tự.
/// traLoi(kq) được gọi đúng một lần khi xong (luồng gọi, hoặc luồng xử lý của hang_doi).
///
/// "Cổng" (TamDung): đổi chế độ / đặt lại chỉ diễn ra khi không còn thao tác nào đang chạy hay đang xếp hàng –
/// không bao giờ có hai chế độ chạy lẫn vào nhau.</summary>
sealed class KhoTranhChap
{
    /// <summary>Một ô tài nguyên: giá trị, phiên bản (cho lac_quan), khóa riêng (cho khoa).</summary>
    sealed class O(object giaTri)
    {
        public volatile object GiaTri = giaTri;
        public long PhienBan;
        public readonly object Khoa = new();
    }

    sealed record Viec(List<string> CacKhoa, XuLy XuLy, Action<JsonObject>? KhiGhi, Action<JsonObject> TraLoi);

    readonly ThongKe thongKe;
    volatile string cheDo;
    double treMs;
    public bool KhoaNgayTho { get; set; }            // khoa: true = khóa theo thứ tự cacKhoa (có thể khóa chết)
    // Bảng ô: ConcurrentDictionary vì bài có thể THÊM ô khi đang chạy (MR6.9: ô của từng người quay, MR6.13: ô lượt
    // mua) trong lúc các luồng khác đang đọc; giá trị trong mỗi ô vẫn đi qua 4 chế độ như trước. Đặt lại: dựng bảng
    // mới rồi thay nguyên bảng (XoaHet ... XongDung).
    volatile ConcurrentDictionary<string, O> o = new();
    volatile ConcurrentDictionary<string, O>? dangDung;  // bảng mới đang dựng (XoaHet ... XongDung)
    readonly object khoaCas = new();                 // lac_quan: khóa rất ngắn quanh đọc ảnh chụp và so sánh-và-đổi
    readonly object cong = new();
    bool dongCong;
    int dangChay;
    readonly BlockingCollection<Viec?> hang = [];     // hang_doi: hàng đợi FIFO cho MỘT luồng xử lý

    public KhoTranhChap(ThongKe thongKe, string cheDo = "khoa", double treMs = 2.0)
    {
        if (!Chung.CheDo.Contains(cheDo)) throw new ArgumentException($"chế độ không hợp lệ: {cheDo}");
        this.thongKe = thongKe;
        this.cheDo = cheDo;
        this.treMs = treMs;
        new Thread(VongHangDoi) { IsBackground = true, Name = "hang_doi" }.Start();
    }

    public string CheDo => cheDo;
    public double TreMs => Volatile.Read(ref treMs);

    // ------------------------------------------------------------ ô tài nguyên
    /// <summary>Bắt đầu dựng lại kho (chỉ gọi khi cổng đóng): các ô mới thêm bằng DatO, XongDung() mới thay bảng –
    /// trong lúc dựng, luồng hiển thị vẫn đọc được bảng cũ trọn vẹn.</summary>
    public void XoaHet() => dangDung = new();

    /// <summary>Tạo (hoặc thay) một ô – lúc dựng kho, hoặc khi đang chạy (ô mới: chưa luồng nào chạm tới).</summary>
    public void DatO(string khoa, object giaTri) => (dangDung ?? o)[khoa] = new O(giaTri);

    public void XongDung()
    {
        if (dangDung == null) return;
        o = dangDung;
        dangDung = null;
    }

    /// <summary>Đọc không khóa (để hiển thị): giá trị là record bất biến được thay nguyên đối tượng nên luôn trọn vẹn.
    /// Ô không có -> KeyNotFoundException.</summary>
    public object Doc(string khoa) => o[khoa].GiaTri;

    public bool Co(string khoa) => o.ContainsKey(khoa);

    public Dictionary<string, object> AnhChup() => o.ToDictionary(x => x.Key, x => x.Value.GiaTri);

    public int DoDaiHang() => hang.Count;

    // ------------------------------------------------------------ cấu hình
    /// <summary>Đóng cổng, chờ mọi thao tác đang chạy/đang xếp hàng xong, chạy viec, rồi mở lại cổng.</summary>
    public void TamDung(Action viec)
    {
        lock (cong)
        {
            while (dongCong) Monitor.Wait(cong);
            dongCong = true;
            while (dangChay > 0) Monitor.Wait(cong);
        }
        try { viec(); }
        finally
        {
            lock (cong)
            {
                dongCong = false;
                Monitor.PulseAll(cong);
            }
        }
    }

    public void DatCheDo(string? cd)
    {
        if (cd == null || !Chung.CheDo.Contains(cd)) throw new ArgumentException($"chế độ không hợp lệ: {cd ?? "None"}");
        TamDung(() => cheDo = cd);
    }

    public void DatTre(double ms)
    {
        if (!(ms >= 0 && ms <= 100)) throw new ArgumentException("độ trễ 0–100 ms");
        Volatile.Write(ref treMs, ms);
    }

    /// <summary>Giả lập thời gian xử lý (đọc cơ sở dữ liệu, kiểm tra nghiệp vụ): ngẫu nhiên 0 … treMs.</summary>
    void Tre()
    {
        double t = TreMs;
        if (t > 0) Chung.NguMs(Random.Shared.NextDouble() * t);
    }

    // ------------------------------------------------------------ thực hiện một thao tác
    public void ThucHien(IEnumerable<string> cacKhoaVao, XuLy xuLy, Action<JsonObject> traLoi, Action<JsonObject>? khiGhi = null)
    {
        var cacKhoa = cacKhoaVao.Distinct().ToList();                 // bỏ trùng, giữ thứ tự
        var bang = o;
        if (cacKhoa.Any(k => !bang.ContainsKey(k)))
        {
            traLoi(new JsonObject { ["ok"] = false, ["ma_loi"] = "khong_co", ["ly_do"] = "không có tài nguyên" });
            return;
        }
        string cd;
        lock (cong)
        {
            while (dongCong) Monitor.Wait(cong);
            dangChay++;
            cd = cheDo;
        }
        if (cd == "hang_doi")
        {
            hang.Add(new Viec(cacKhoa, xuLy, khiGhi, traLoi));
            thongKe.DatMax("hang_doi_max", hang.Count);
            return;                                                   // luồng xử lý sẽ gọi traLoi
        }
        JsonObject kq;
        try
        {
            kq = cd switch
            {
                "khong_khoa" => MotLan(cacKhoa, xuLy, khiGhi),
                "khoa" => CoKhoa(cacKhoa, xuLy, khiGhi),
                _ => LacQuan(cacKhoa, xuLy, khiGhi),
            };
        }
        catch (Exception e)                                           // lỗi nghiệp vụ: trả lời lỗi, không làm sập luồng
        {
            kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "loi", ["ly_do"] = $"lỗi xử lý: {e.Message}" };
        }
        finally { XongMot(); }
        traLoi(kq);
    }

    void XongMot()
    {
        lock (cong)
        {
            dangChay--;
            if (dangChay == 0) Monitor.PulseAll(cong);
        }
    }

    void Ghi(Dictionary<string, object> ghi, JsonObject kq, Action<JsonObject>? khiGhi)
    {
        var bang = o;
        foreach (var (k, v) in ghi)
        {
            var oo = bang[k];
            oo.GiaTri = v;
            oo.PhienBan++;
        }
        khiGhi?.Invoke(kq);
    }

    Dictionary<string, object> DocCac(List<string> cacKhoa)
    {
        var bang = o;
        var doc = new Dictionary<string, object>(cacKhoa.Count);
        foreach (var k in cacKhoa) doc[k] = bang[k].GiaTri;
        return doc;
    }

    /// <summary>khong_khoa (và luồng hang_doi): đọc -> trễ -> xử lý -> ghi, KHÔNG khóa.
    /// Hai luồng cùng đọc giá trị cũ rồi cùng ghi: bản ghi sau đè bản trước (mất cập nhật).</summary>
    JsonObject MotLan(List<string> cacKhoa, XuLy xuLy, Action<JsonObject>? khiGhi)
    {
        var doc = DocCac(cacKhoa);
        Tre();
        var (kq, ghi) = xuLy(doc);
        if (ghi is { Count: > 0 }) Ghi(ghi, kq, khiGhi);
        return kq;
    }

    /// <summary>khoa: giữ khóa của MỌI ô liên quan suốt đọc – xử lý – ghi (vùng găng – critical section).
    /// Khóa theo thứ tự khóa ô đã sắp xếp => mọi luồng xin khóa cùng một thứ tự => không khóa chết.
    /// KhoaNgayTho: khóa theo thứ tự yêu cầu (ví dụ nguồn rồi đích), nghỉ giữa hai lần khóa.</summary>
    JsonObject CoKhoa(List<string> cacKhoa, XuLy xuLy, Action<JsonObject>? khiGhi)
    {
        bool ngayTho = KhoaNgayTho;
        var thuTu = ngayTho ? cacKhoa : [.. cacKhoa.OrderBy(k => k, StringComparer.Ordinal)];
        var bang = o;
        var daGiu = new List<string>();
        try
        {
            for (int i = 0; i < thuTu.Count; i++)
            {
                string k = thuTu[i];
                if (i > 0 && ngayTho) Tre();
                if (!Monitor.TryEnter(bang[k].Khoa, TimeSpan.FromSeconds(Chung.ChoKhoaToiDaS)))
                {
                    thongKe.Cong("khoa_cho_lau");
                    return new JsonObject
                    {
                        ["ok"] = false, ["ma_loi"] = "khoa_cho_lau",
                        ["ly_do"] = $"chờ khóa {k} quá {Chung.G(Chung.ChoKhoaToiDaS)} s (khóa chết?)",
                    };
                }
                daGiu.Add(k);
            }
            return MotLan(cacKhoa, xuLy, khiGhi);
        }
        finally
        {
            for (int i = daGiu.Count - 1; i >= 0; i--) Monitor.Exit(bang[daGiu[i]].Khoa);
        }
    }

    /// <summary>lac_quan (Kung &amp; Robinson 1981): không giữ khóa khi xử lý; lúc ghi kiểm tra phiên bản.</summary>
    JsonObject LacQuan(List<string> cacKhoa, XuLy xuLy, Action<JsonObject>? khiGhi)
    {
        var bang = o;
        for (int lan = 0; lan < Chung.SoLanThu; lan++)
        {
            Dictionary<string, object> doc;
            var phienBan = new Dictionary<string, long>(cacKhoa.Count);
            lock (khoaCas)                                            // ảnh chụp nhất quán của các ô
            {
                doc = DocCac(cacKhoa);
                foreach (var k in cacKhoa) phienBan[k] = bang[k].PhienBan;
            }
            Tre();
            var (kq, ghi) = xuLy(doc);
            if (ghi is not { Count: > 0 }) return kq;                 // chỉ đọc / từ chối: không cần so sánh-và-đổi
            lock (khoaCas)                                            // so sánh-và-đổi (compare-and-swap)
            {
                if (cacKhoa.All(k => bang[k].PhienBan == phienBan[k]))
                {
                    Ghi(ghi, kq, khiGhi);
                    return kq;
                }
            }
            thongKe.Cong("xung_dot");                                 // có luồng khác ghi trước: làm lại từ đầu
            if (lan < Chung.SoLanThu - 1)
            {
                thongKe.Cong("thu_lai");
                Chung.NguMs(Random.Shared.NextDouble());
            }
        }
        thongKe.Cong("het_lan_thu");
        return new JsonObject { ["ok"] = false, ["ma_loi"] = "xung_dot", ["ly_do"] = "xung đột, thử lại sau" };
    }

    /// <summary>hang_doi: MỘT luồng lấy lần lượt từng việc (đến trước phục vụ trước) – không cần khóa vì chỉ có
    /// luồng này chạm vào trạng thái.</summary>
    void VongHangDoi()
    {
        foreach (var viec in hang.GetConsumingEnumerable())
        {
            if (viec == null) return;
            JsonObject kq;
            try { kq = MotLan(viec.CacKhoa, viec.XuLy, viec.KhiGhi); }
            catch (Exception e) { kq = new JsonObject { ["ok"] = false, ["ma_loi"] = "loi", ["ly_do"] = $"lỗi xử lý: {e.Message}" }; }
            XongMot();
            try { viec.TraLoi(kq); }
            catch (Exception) { }
        }
    }

    public void Dong() => hang.Add(null);
}

// ================================================================ 4. Sổ kiểm toán
/// <summary>Ghi mọi giao dịch ĐƯỢC CHẤP NHẬN (dưới khóa riêng của sổ – không ảnh hưởng tranh chấp của kho) và đếm
/// sai phạm. Lớp con đặt TenSaiPham và viết hai luật:
///   KiemTra(gd)        – gọi khi ghi từng giao dịch (theo thứ tự ghi sổ), gọi SaiPham(ma, moTa);
///   KiemTraCuoi(doc)   – đối chiếu sổ với trạng thái hiện tại của kho, trả về {ma: số}.</summary>
abstract class SoKiemToan
{
    protected abstract (string Ma, string Ten)[] TenSaiPham { get; }
    public readonly object Khoa = new();
    readonly List<object> giaoDich = [];
    Dictionary<string, long>? dem;
    readonly List<string> viDu = [];                     // mô tả vài sai phạm đầu tiên

    Dictionary<string, long> Dem => dem ??= TenSaiPham.ToDictionary(x => x.Ma, _ => 0L);

    public void Ghi(object gd)
    {
        lock (Khoa)
        {
            giaoDich.Add(gd);
            KiemTra(gd);
        }
    }

    protected void SaiPham(string ma, string moTa)
    {
        Dem[ma]++;
        if (viDu.Count < 5) viDu.Add(moTa);
    }

    /// <summary>Cộng thêm n vào bộ đếm sai phạm (không thêm ví dụ) – ví dụ MR6.13 đếm theo số vé bán vượt.</summary>
    protected void CongSaiPham(string ma, long n) => Dem[ma] += n;

    protected virtual void KiemTra(object gd) { }

    protected virtual Dictionary<string, long> KiemTraCuoi(IReadOnlyDictionary<string, object> doc) => [];

    /// <summary>-> (danh sách {ma, ten, so}, tổng số sai phạm, vài ví dụ).</summary>
    public (List<(string Ma, string Ten, long So)> Ds, long Tong, List<string> ViDu) TongKet(IReadOnlyDictionary<string, object> doc)
    {
        Dictionary<string, long> d;
        List<string> vd;
        lock (Khoa)
        {
            d = new Dictionary<string, long>(Dem);
            foreach (var (ma, so) in KiemTraCuoi(doc)) d[ma] += so;
            vd = [.. viDu];
        }
        var ds = TenSaiPham.Select(x => (x.Ma, x.Ten, d[x.Ma])).ToList();
        return (ds, d.Values.Sum(), vd);
    }
}

// ================================================================ 5. Máy chủ TCP đa luồng
/// <summary>Một kết nối tới máy chủ (người dùng thật, kết nối gộp của bộ mô phỏng, hoặc quản trị).</summary>
sealed class KhachKetNoi(long ma, string ten, string vaiTro, Socket conn, long? soNguoi)
{
    public readonly long Ma = ma;
    public readonly string Ten = ten, VaiTro = vaiTro;
    public readonly Socket Conn = conn;
    public readonly long? SoNguoi = soNguoi;             // null: người dùng thật; m: kết nối gộp m người ảo
    public readonly object KhoaGui = new();              // nhiều luồng (kết nối, hàng đợi, đẩy) cùng gửi
    readonly Dictionary<long, JsonObject?> daTra = [];   // ma_yc -> kết quả đã gửi (null: đang xử lý)
    readonly Queue<long> thuTu = new();                  // thứ tự ma_yc để bỏ cái cũ nhất khi quá NhoKq
    public long SoYc;

    public bool Gui(JsonObject obj)
    {
        try
        {
            lock (KhoaGui) Chung.GuiJson(Conn, obj);
            return true;
        }
        catch (Exception e) when (e is SocketException or ObjectDisposedException) { return false; }
    }

    /// <summary>true nếu ma_yc mới. ma_yc đã gặp: gửi lại kết quả cũ (nếu đã có) kèm "lap_lai": true, trả false.</summary>
    public bool NhanYc(long maYc)
    {
        lock (KhoaGui)
        {
            if (daTra.TryGetValue(maYc, out var cu))
            {
                if (cu != null)
                {
                    var lai = (JsonObject)cu.DeepClone();
                    lai["lap_lai"] = true;
                    try { Chung.GuiJson(Conn, lai); }
                    catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
                }
                return false;
            }
            daTra[maYc] = null;
            thuTu.Enqueue(maYc);
            while (daTra.Count > Chung.NhoKq && thuTu.Count > 0) daTra.Remove(thuTu.Dequeue());
            return true;
        }
    }

    public void Tra(long maYc, JsonObject tin)
    {
        lock (KhoaGui)
        {
            if (!daTra.ContainsKey(maYc)) thuTu.Enqueue(maYc);
            daTra[maYc] = tin;
            try { Chung.GuiJson(Conn, tin); }
            catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        }
    }
}

/// <summary>Khung máy chủ. Lớp con (MayChu.cs) đặt Ten, LoaiYeuCau, ThamSoMacDinh và viết:
///   KhoiTao()                                tạo các ô trong Kho (DatO), tạo So (SoKiemToan)
///   XuLyYeuCau(khach, nguoi, tin, traLoi)    kiểm tra tin, gọi Kho.ThucHien(...)
///   TrangThaiNghiepVu()                      JsonObject trạng thái nghiệp vụ cho cap_nhat (đọc không khóa)
///   TomTat()                                 một dòng ngắn (≤ 48 ký tự) mô tả kết quả
///   DinhKy(bayGio)  (tùy chọn)               gọi mỗi 100 ms trên luồng nền (đồng hồ, hết hạn…)
/// Báo đổi trạng thái: BaoDoi() (đẩy cap_nhat ở nhịp kế tiếp); thông báo: ThongBao(text).
/// Lưu ý: KhoiTao() được gọi ngay trong hàm dựng của lớp cơ sở – lớp con khởi tạo trường bằng biểu thức gán ở khai
/// báo (chạy trước), không dựa vào thân hàm dựng của mình.</summary>
abstract class MayChuCoSo
{
    public virtual string Ten => "Máy chủ";
    public abstract string[] LoaiYeuCau { get; }
    public abstract OrderedDictionary<string, object> ThamSoMacDinh { get; }

    public readonly Action<string> Bao;
    public readonly ThongKe ThongKe = new();
    public readonly KhoTranhChap Kho;
    public readonly OrderedDictionary<string, object> ThamSo;
    readonly object khoa = new();                        // bảo vệ ds (danh sách kết nối)
    readonly Dictionary<long, KhachKetNoi> ds = [];
    volatile bool chay;
    public string DiaChi { get; private set; } = "";
    volatile bool doi;
    readonly ConcurrentQueue<string> suKien = new();
    readonly List<Thread> luong = [];
    TcpListener? srv;
    long maKetNoi;
    public SoKiemToan So = null!;

    public bool Chay => chay;

    protected MayChuCoSo(string cheDo, double treMs, Action<string>? bao, IEnumerable<KeyValuePair<string, object>>? thamSo)
    {
        Bao = bao ?? (_ => { });
        Kho = new KhoTranhChap(ThongKe, cheDo, treMs);
        ThamSo = new OrderedDictionary<string, object>(ThamSoMacDinh);
        var ts = EpKieuThamSo(thamSo ?? []);
        KiemThamSo(ts);
        foreach (var (k, v) in ts) ThamSo[k] = v;
        KhoiTaoLai();
    }

    // ------------------------------------------------------------ phần nghiệp vụ (lớp con viết)
    protected abstract void KhoiTao();
    protected abstract void XuLyYeuCau(KhachKetNoi khach, string nguoi, JsonObject tin, Action<JsonObject> traLoi);
    protected virtual JsonObject TrangThaiNghiepVu() => [];
    protected virtual string TomTat() => "";
    protected virtual void DinhKy(double bayGio) { }

    /// <summary>Kiểm tra tham số đặt lại; lớp con mở rộng (ném ArgumentException).</summary>
    protected virtual void KiemThamSo(OrderedDictionary<string, object> ts)
    {
        foreach (var k in ts.Keys)
            if (!ThamSoMacDinh.ContainsKey(k)) throw new ArgumentException($"tham số lạ: {k}");
    }

    public long TsL(string k) => (long)ThamSo[k];
    public double TsD(string k) => (double)ThamSo[k];
    public string TsS(string k) => (string)ThamSo[k];

    /// <summary>Ép kiểu một giá trị theo kiểu của giá trị mặc định (như type(mặc_định)(v) của Python).</summary>
    public static object EpKieu(object macDinh, object? v)
    {
        if (v is JsonNode n)
        {
            v = n.GetValueKind() switch
            {
                JsonValueKind.String => n.GetValue<string>(),
                JsonValueKind.Number => Chung.LaSoNguyen(n, out var l) ? l : Chung.SoD(n)!.Value,
                JsonValueKind.True => true,
                JsonValueKind.False => false,
                _ => n.ToJsonString(),
            };
        }
        switch (macDinh)
        {
            case long:
                return v switch
                {
                    long l => l,
                    double d when double.IsFinite(d) => (long)Math.Truncate(d),
                    bool b => b ? 1L : 0L,
                    string s when long.TryParse(s.Trim(), NumberStyles.AllowLeadingSign, Chung.Ic, out var l2) => l2,
                    string s => throw new ArgumentException($"invalid literal for int() with base 10: '{s}'"),
                    null => throw new ArgumentException("int() argument must be a string or a number, not 'NoneType'"),
                    _ => throw new ArgumentException($"cannot convert to int: {v}"),
                };
            case double:
                return v switch
                {
                    double d => d,
                    long l => (double)l,
                    bool b => b ? 1.0 : 0.0,
                    string s when Chung.DocSoThuc(s, out var d2) => d2,
                    string s => throw new ArgumentException($"could not convert string to float: '{s}'"),
                    null => throw new ArgumentException("float() argument must be a string or a real number, not 'NoneType'"),
                    _ => throw new ArgumentException($"cannot convert to float: {v}"),
                };
            default:
                return Chung.Repr(v);
        }
    }

    OrderedDictionary<string, object> EpKieuThamSo(IEnumerable<KeyValuePair<string, object>> thamSo)
    {
        var ts = new OrderedDictionary<string, object>();
        foreach (var (k, v) in thamSo)                   // ép kiểu theo giá trị mặc định
        {
            if (!ThamSoMacDinh.TryGetValue(k, out var md)) throw new ArgumentException($"tham số lạ: {k}");
            ts[k] = EpKieu(md, v);
        }
        return ts;
    }

    // ------------------------------------------------------------ điều khiển
    void KhoiTaoLai()
    {
        Kho.TamDung(() =>                               // không còn thao tác nào đang chạy
        {
            Kho.XoaHet();
            ThongKe.DatLai();
            KhoiTao();
            Kho.XongDung();
        });
        BaoDoi();
    }

    public void DatLai(IEnumerable<KeyValuePair<string, object>>? thamSo = null)
    {
        var ts = EpKieuThamSo(thamSo ?? []);
        KiemThamSo(ts);
        var tk = ThongKe.AnhChup();
        if (Chung.SoL(tk, "so_yc") > 0)
        {
            var (_, sp, _) = So.TongKet(Kho.AnhChup());
            Bao($"Lượt trước: {Kho.CheDo}, {Chung.SoL(tk, "so_yc")} yc, tc {Chung.SoL(tk, "thanh_cong")}, sai phạm {sp}");
        }
        foreach (var (k, v) in ts) ThamSo[k] = v;
        KhoiTaoLai();
        ThongBao("Đặt lại" + (ts.Count == 0 ? "" : " (" + string.Join(", ", ts.Select(x => $"{x.Key}={Chung.Repr(x.Value)}")) + ")"));
    }

    public void DatCheDo(string? cheDo)
    {
        if (cheDo == Kho.CheDo) return;
        Kho.DatCheDo(cheDo);
        ThongBao($"Chế độ: {cheDo} – {Chung.TenCheDo[cheDo!]}");
        BaoDoi();
    }

    public void DatTre(double ms)
    {
        Kho.DatTre(ms);
        Bao($"Độ trễ xử lý: ngẫu nhiên 0–{Chung.G(Kho.TreMs)} ms");
    }

    public void BaoDoi() => doi = true;

    /// <summary>Ghi nhật ký máy chủ và gửi {"loai":"su_kien"} cho mọi client (ở nhịp đẩy kế tiếp).</summary>
    public void ThongBao(string text)
    {
        Bao(text);
        suKien.Enqueue(text);
    }

    // ------------------------------------------------------------ khởi động / dừng
    public void BatDau(string host, int cong)
    {
        var l = new TcpListener(Chung.DiaChiIp(host), cong);
        l.Start(1024);
        srv = l;
        DiaChi = $"{host}:{cong}";
        chay = true;
        luong.Clear();
        luong.Add(new Thread(NhanKetNoi) { IsBackground = true, Name = "nhan_ket_noi" });
        luong.Add(new Thread(VongNen) { IsBackground = true, Name = "vong_nen" });
        foreach (var t in luong) t.Start();
        Bao($"Lắng nghe {host}:{cong}; chế độ {Kho.CheDo}, trễ 0–{Chung.G(Kho.TreMs)} ms");
    }

    public void Dung()
    {
        if (!chay) return;
        chay = false;
        srv?.Stop();                                     // luồng nhận kết nối thoát khỏi AcceptSocket
        List<KhachKetNoi> dsk;
        lock (khoa) dsk = [.. ds.Values];
        foreach (var k in dsk)
        {
            try { k.Conn.Shutdown(SocketShutdown.Both); }
            catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
        }
        foreach (var t in luong) t.Join(2000);
        Bao("Máy chủ đã dừng");
    }

    // ------------------------------------------------------------ kết nối
    void NhanKetNoi()
    {
        while (chay)
        {
            Socket conn;
            try { conn = srv!.AcceptSocket(); }
            catch (Exception e) when (e is SocketException or ObjectDisposedException or InvalidOperationException) { break; }
            new Thread(() => PhucVu(conn)) { IsBackground = true }.Start();
        }
    }

    /// <summary>Luồng của một kết nối: chào, rồi đọc và xử lý lần lượt từng yêu cầu (có thể pipelining).</summary>
    void PhucVu(Socket conn)
    {
        KhachKetNoi? k = null;
        try
        {
            conn.NoDelay = true;
            var doc = new DocDong(new NetworkStream(conn, ownsSocket: false));
            var tin = doc.Doc();
            string vaiTro = tin?["vai_tro"] is null ? "nguoi_dung" : Chung.Chuoi(tin, "vai_tro") ?? "";
            long? soNguoi = null;
            bool soNguoiHopLe = true;
            if (tin != null && !Chung.LaNull(tin["so_nguoi"]))
            {
                soNguoiHopLe = Chung.LaSoNguyen(tin["so_nguoi"], out var m) && m > 0;
                soNguoi = m;
            }
            if (tin == null || Chung.Chuoi(tin, "loai") != "chao" || !Chung.TenHopLe(Chung.Chuoi(tin, "ten"))
                || !Chung.VaiTro.Contains(vaiTro) || !soNguoiHopLe)
            {
                Chung.GuiJson(conn, new JsonObject { ["loai"] = "loi", ["ly_do"] = "cần chào: tên 1–16 ký tự, vai_tro hợp lệ" });
                return;
            }
            k = new KhachKetNoi(Interlocked.Increment(ref maKetNoi), Chung.Chuoi(tin, "ten")!, vaiTro, conn, soNguoi);
            lock (k.KhoaGui)                             // luồng đẩy chưa gửi được gì cho k trước lời chào
            {
                lock (khoa) ds[k.Ma] = k;
                Chung.GuiJson(conn, new JsonObject { ["loai"] = "chao", ["ma"] = k.Ma, ["che_do"] = Kho.CheDo, ["may_chu"] = Ten });
                Chung.GuiJson(conn, GoiCapNhat());
            }
            if (soNguoi == null && k.VaiTro == "nguoi_dung") Bao($"{k.Ten} vào");   // kết nối gộp, quản trị: không in
            while (chay)
            {
                tin = doc.Doc();
                if (tin == null) break;
                NhanTin(k, tin);
            }
        }
        catch (Exception e) when (e is IOException or SocketException or InvalidDataException or ObjectDisposedException)
        {
            if (k != null && chay && k.SoNguoi == null) Bao($"{k.Ten}: lỗi kết nối ({e.Message})");
        }
        finally
        {
            if (k != null)
            {
                lock (khoa) ds.Remove(k.Ma);
                if (k.SoNguoi == null && k.VaiTro == "nguoi_dung" && chay) Bao($"{k.Ten} rời");
            }
            try { conn.Close(); }
            catch (SocketException) { }
        }
    }

    static JsonObject KqLoi(long maYc, string maLoi, string lyDo) =>
        new() { ["loai"] = "kq", ["ma_yc"] = maYc, ["ok"] = false, ["ma_loi"] = maLoi, ["ly_do"] = lyDo };

    void NhanTin(KhachKetNoi k, JsonObject tin)
    {
        string? loai = Chung.Chuoi(tin, "loai");
        if (!Chung.LaSoNguyen(tin["ma_yc"], out long maYc))
        {
            k.Gui(new JsonObject { ["loai"] = "loi", ["ly_do"] = "thiếu ma_yc (số nguyên)" });
            return;
        }
        if (!k.NhanYc(maYc)) return;                     // ma_yc lặp lại: đã gửi lại kết quả cũ
        k.SoYc++;
        if (loai != null && LoaiYeuCau.Contains(loai))
        {
            string? nguoi = k.SoNguoi != null ? Chung.Chuoi(tin, "nguoi") : k.Ten;
            if (!Chung.TenHopLe(nguoi))
            {
                k.Tra(maYc, KqLoi(maYc, "sai_du_lieu", "thiếu tên người (nguoi)"));
                return;
            }
            long t0 = Stopwatch.GetTimestamp();
            ThongKe.BatDauYc();

            void TraLoi(JsonObject kq)                   // gọi từ luồng kết nối hoặc luồng hàng đợi
            {
                ThongKe.XongYc(kq, Stopwatch.GetElapsedTime(t0).TotalMicroseconds);
                var tl = new JsonObject { ["loai"] = "kq", ["ma_yc"] = maYc };
                foreach (var (x, v) in kq) tl[x] = v?.DeepClone();
                k.Tra(maYc, tl);
            }
            XuLyYeuCau(k, nguoi!, tin, TraLoi);
        }
        else if (loai is "che_do" or "tre" or "dat_lai" or "thong_ke")
        {
            if (k.VaiTro != "quan_tri")
            {
                k.Tra(maYc, KqLoi(maYc, "khong_quyen", "chỉ quản trị"));
                return;
            }
            try
            {
                if (loai == "thong_ke")
                {
                    var tl = new JsonObject { ["loai"] = "thong_ke", ["ma_yc"] = maYc };
                    foreach (var (x, v) in ThongKeDayDu()) tl[x] = v?.DeepClone();
                    k.Tra(maYc, tl);
                    return;
                }
                if (loai == "che_do") DatCheDo(Chung.Chuoi(tin, "gia_tri") ?? (Chung.LaNull(tin["gia_tri"]) ? null : Chung.ChuoiPy(tin["gia_tri"])));
                else if (loai == "tre") DatTre((double)EpKieu(0.0, tin["ms"]));
                else DatLai(tin.Where(x => x.Key is not ("loai" or "ma_yc")).Select(x => new KeyValuePair<string, object>(x.Key, (object?)x.Value ?? JsonValue.Create((string?)null)!)));
                k.Tra(maYc, new JsonObject { ["loai"] = "kq", ["ma_yc"] = maYc, ["ok"] = true });
            }
            catch (ArgumentException e)
            {
                k.Tra(maYc, KqLoi(maYc, "sai_du_lieu", e.Message));
            }
        }
        else
        {
            k.Tra(maYc, KqLoi(maYc, "khong_ho_tro", $"loại không hỗ trợ: {Chung.ChuoiPy(tin["loai"])}"));
        }
    }

    // ------------------------------------------------------------ đẩy trạng thái (gộp ≤ 10 lần/giây)
    JsonObject GoiCapNhat()
    {
        var o = new JsonObject { ["loai"] = "cap_nhat", ["che_do"] = Kho.CheDo };
        foreach (var (x, v) in TrangThaiNghiepVu()) o[x] = v?.DeepClone();
        return o;
    }

    void VongNen()
    {
        while (chay)
        {
            Thread.Sleep(TimeSpan.FromSeconds(Chung.NhipDayS));
            try { DinhKy(Chung.Now); }
            catch (Exception e) { Bao($"lỗi định kỳ: {e.Message}"); }
            var goi = new MemoryStream();
            while (suKien.TryDequeue(out var t)) goi.Write(Chung.MaHoa(new JsonObject { ["loai"] = "su_kien", ["noi_dung"] = t }));
            if (doi)
            {
                doi = false;                             // xóa cờ TRƯỚC khi chụp: đổi sau đó vẫn được đẩy
                try { goi.Write(Chung.MaHoa(GoiCapNhat())); }
                catch (Exception e) { Bao($"lỗi định kỳ: {e.Message}"); }
            }
            if (goi.Length == 0) continue;
            var duLieu = goi.ToArray();
            List<KhachKetNoi> dsk;
            lock (khoa) dsk = [.. ds.Values];
            foreach (var k in dsk)
            {
                try
                {
                    lock (k.KhoaGui) Chung.GuiHet(k.Conn, duLieu);
                }
                catch (Exception e) when (e is SocketException or ObjectDisposedException) { }
            }
        }
    }

    // ------------------------------------------------------------ số liệu
    /// <summary>Trạng thái hiện tại (như cap_nhat gửi client) – giao diện máy chủ đọc định kỳ.</summary>
    public JsonObject TrangThai() => GoiCapNhat();

    public (int KetNoi, long Nguoi) SoKetNoi()
    {
        List<KhachKetNoi> dsk;
        lock (khoa) dsk = [.. ds.Values];
        var nd = dsk.Where(k => k.VaiTro != "quan_tri");
        return (dsk.Count, nd.Sum(k => k.SoNguoi ?? 1));
    }

    public JsonObject ThongKeDayDu()
    {
        var tk = ThongKe.AnhChup();
        var (dsSp, tongSp, viDu) = So.TongKet(Kho.AnhChup());
        var (soKn, soNguoi) = SoKetNoi();
        var d = new JsonObject { ["che_do"] = Kho.CheDo, ["tre_ms"] = Kho.TreMs };
        foreach (var (x, v) in tk) d[x] = v?.DeepClone();
        d["hang_doi"] = Kho.DoDaiHang();
        d["sai_pham"] = new JsonArray([.. dsSp.Select(s => (JsonNode)new JsonObject { ["ma"] = s.Ma, ["ten"] = s.Ten, ["so"] = s.So })]);
        d["sai_pham_tong"] = tongSp;
        d["vi_du_sai_pham"] = new JsonArray([.. viDu.Take(3).Select(v => (JsonNode)JsonValue.Create(v))]);
        d["tom_tat"] = TomTat();
        d["so_ket_noi"] = soKn;
        d["so_nguoi"] = soNguoi;
        return d;
    }
}
