// Bài mở rộng MR5.6 – Bộ lập lịch mô phỏng: đơn luồng/đa luồng × đồng bộ/bất đồng bộ.
// (Chuyển từ lap_lich.py của bản Python; dùng chung cho bản dòng lệnh và ứng dụng WinForms.)
//
// Thời gian rời rạc, 1 nhịp = 100 ms. Mỗi tác vụ T1, T2, T3 có thời lượng D giây, gồm ba bước:
//   CPU (D/4) → chờ I/O (D/2, ví dụ đọc mạng hay đĩa) → CPU (phần còn lại).
// Tùy chọn:
//   - phụ thuộc: T3 chỉ bắt đầu khi T1 và T2 đã xong (Thread.Join / await Task.WhenAll);
//   - chia: bước CPU cuối của T1 được chia đôi, hai nửa chạy cùng lúc trên hai luồng rồi hợp lại;
//   - việc khẩn U: đến khi các luồng đang bận (mặc định lúc 0,3 s), cần 0,5 s CPU, ưu tiên cao;
//   - số lõi CPU và lát thời gian (quantum) của bộ lập lịch hệ điều hành.
//
// Bốn chế độ:
//   don_dong_bo      một luồng, lời gọi chặn: làm lần lượt theo thứ tự đến (T1, T2, T3 rồi U); khi chờ I/O
//                    luồng đứng yên, không làm được việc khác;
//   da_dong_bo       mỗi tác vụ một luồng hệ điều hành (new Thread), lời gọi chặn: các luồng tranh nhau lõi CPU;
//                    hệ điều hành chia lát thời gian và cho luồng ưu tiên cao CHEN NGANG ngay (preemption);
//                    T3 chờ T1, T2 bằng Join; nửa sau phần CPU của T1 chạy trên luồng phụ T1b;
//   don_bat_dong_bo  một luồng chạy vòng lặp sự kiện: bước CPU chạy trên vòng lặp tới khi xong (đa nhiệm hợp tác,
//                    không ai chen ngang), chờ I/O không giữ luồng nên các I/O chồng lên nhau; việc sẵn sàng xếp
//                    hàng theo thứ tự đến (FIFO, như SynchronizationContext một luồng của WinForms);
//   da_bat_dong_bo   nhóm luồng thợ (số thợ = số lõi) + bất đồng bộ: mỗi bước CPU giao cho thợ rảnh đầu tiên, nên
//                    sau một lần chờ I/O, cùng một tác vụ có thể chạy tiếp trên MỘT LUỒNG KHÁC; hai nửa của T1 chạy
//                    song song trên hai thợ (như Task.Run + await trên ThreadPool, ConfigureAwait(false)).
// Mỗi nhịp gồm bốn bước theo thứ tự cố định: (1) tác vụ đến, (2) các luồng/tác vụ tiến sang thao tác kế tiếp,
// (3) lập lịch (cấp lõi, giao việc cho thợ), (4) chạy một nhịp và ghi lại. Kết quả tất định: cùng tham số →
// cùng biểu đồ, cùng số liệu ở mọi ngôn ngữ (đầu ra bản dòng lệnh trùng từng ký tự với bản Python).
using System.Text;

namespace BaiMR5_6;

/// <summary>Hằng số, hàm tiện ích và điểm vào của bộ lập lịch.</summary>
public static class LapLich
{
    // Thứ tự khai báo = thứ tự duyệt (như dict của Python)
    public static readonly string[] DsCheDo = ["don_dong_bo", "da_dong_bo", "don_bat_dong_bo", "da_bat_dong_bo"];
    public static readonly Dictionary<string, string> CheDo = new()
    {
        ["don_dong_bo"] = "Đơn luồng – đồng bộ (Single-threaded Sync)",
        ["da_dong_bo"] = "Đa luồng – đồng bộ (Multi-threaded Sync)",
        ["don_bat_dong_bo"] = "Đơn luồng – bất đồng bộ (Single-threaded Async)",
        ["da_bat_dong_bo"] = "Đa luồng – bất đồng bộ (Multi-threaded Async)",
    };
    public static readonly Dictionary<string, string> TenNgan = new()
    {
        ["don_dong_bo"] = "đơn, đồng bộ", ["da_dong_bo"] = "đa, đồng bộ",
        ["don_bat_dong_bo"] = "đơn, bất đ.bộ", ["da_bat_dong_bo"] = "đa, bất đ.bộ",
    };
    // trạng thái một ô (một làn, một nhịp)
    public const string CPU = "cpu", IO = "io", SAN_SANG = "san_sang", CHO = "cho", RANH = "ranh";
    public const int U_CPU = 5;                 // việc khẩn: 0,5 s CPU

    // Chia lấy phần nguyên và phần dư kiểu Python (làm tròn xuống, kể cả với số âm)
    public static int Div(int a, int b) { int q = a / b; return (a % b != 0 && (a < 0) != (b < 0)) ? q - 1 : q; }
    public static int Mod(int a, int b) { int r = a % b; return (r != 0 && (r < 0) != (b < 0)) ? r + b : r; }

    // round() của Python với số thực: làm tròn về số chẵn gần nhất khi đúng nửa (banker's rounding);
    // Math.Round mặc định MidpointRounding.ToEven nên cho cùng kết quả: round(2.5) = 2, round(3.5) = 4
    public static int Nhip(double giay) => (int)Math.Round(giay * 10, MidpointRounding.ToEven);

    /// <summary>Số nhịp (100 ms) → chuỗi giây, một chữ số thập phân.</summary>
    public static string Giay(int nhip) => $"{Div(nhip, 10)}.{Mod(nhip, 10)}";

    // Căn lề theo số ký tự như f"{x:<n}" / f"{x:>n}" của Python (chuỗi tiếng Việt ở đây đều dựng sẵn NFC,
    // mỗi ký tự là một đơn vị UTF-16); không ném lỗi khi độ rộng nhỏ hơn độ dài
    public static string Trai(string s, int n) => s.Length >= n ? s : s + new string(' ', n - s.Length);
    public static string Phai(string s, int n) => s.Length >= n ? s : new string(' ', n - s.Length) + s;

    public static KetQua Chay(string cheDo, ThamSo ts) => cheDo switch
    {
        "don_dong_bo" => new DongBo(ts, false).Chay(),
        "da_dong_bo" => new DongBo(ts, true).Chay(),
        "don_bat_dong_bo" => new BatDongBo(ts, false).Chay(),
        _ => new BatDongBo(ts, true).Chay(),
    };

    public static List<Viec> TaoViec(ThamSo ts)
    {
        var ds = new List<Viec>();
        for (int i = 0; i < 3; i++)
        {
            int n = ts.D[i];
            int c1 = Div(n, 4), io = Div(n, 2);
            int c2 = n - c1 - io;
            var buoc = new List<Buoc> { new("cpu", c1), new("io", io), new(ts.ChiaCpu && i == 0 && c2 >= 2 ? "chia" : "cpu", c2) };
            var dep = ts.PhuThuoc && i == 2 ? new List<string> { "T1", "T2" } : [];
            ds.Add(new Viec($"T{i + 1}", buoc.Where(b => b.N > 0).ToList(), 0, 0, dep));
        }
        if (ts.Khan)
            ds.Add(new Viec("U", [new Buoc("cpu", U_CPU)], 1, ts.TKhan, []));
        return ds;
    }

    // ==================================================================== chỉ số và biểu đồ văn bản
    /// <summary>Hoàn tất (nhịp), độ trễ phản hồi của U (nhịp, -1 nếu không có U), số luồng, % lõi bận.</summary>
    public static ChiSo TinhChiSo(KetQua kq)
    {
        int ban = kq.Lan.Sum(l => l.O.Count(c => c.Tt == CPU));
        var u = kq.Viec.FirstOrDefault(v => v.Ten == "U");
        return new ChiSo(kq.Tong, u != null ? u.BatDau - u.Den : -1, kq.Lan.Count,
                         Div(ban * 100, Math.Max(1, kq.Ts.SoLoi * kq.Tong)), kq.ChenNgang, kq.HetLat);
    }

    public static string KyTu(O c)
    {
        if (c.Tt == CPU) return c.Nhan == "U" ? "U" : c.Nhan.Substring(1, 1);     // T1 → "1"
        return c.Tt switch { IO => "~", SAN_SANG => "-", CHO => ".", _ => " " };
    }

    /// <summary>Biểu đồ Gantt văn bản, 1 ô = 1 nhịp (0,1 s); dài hơn `rong` nhịp thì xuống dòng thành nhiều khối.</summary>
    public static List<string> Gantt(KetQua kq, int rong = 48)
    {
        var dong = new List<string>();
        for (int dau = 0; dau < kq.Tong; dau += rong)
        {
            int cuoi = Math.Min(kq.Tong, dau + rong);
            foreach (var l in kq.Lan)
            {
                var sb = new StringBuilder();
                for (int j = dau; j < Math.Min(cuoi, l.O.Count); j++) sb.Append(KyTu(l.O[j]));
                dong.Add($"  {Trai(l.Ten, 11)}|{sb}|");
            }
            if (kq.Io.Count > 0)
            {
                var sb = new StringBuilder();
                for (int j = dau; j < Math.Min(cuoi, kq.Io.Count); j++)
                    sb.Append(kq.Io[j] != 0 ? Math.Min(kq.Io[j], 9).ToString() : " ");
                dong.Add($"  {Trai("I/O chờ", 11)}|{sb}|");
            }
            string nhan = $"{Giay(cuoi)} s";
            dong.Add($"  {Trai("", 11)} {Giay(dau)}{Phai(nhan, cuoi - dau - Giay(dau).Length + 2)}");
        }
        return dong;
    }
}

public record ChiSo(int Tong, int UCho, int Luong, int Ban, int Chen, int Lat);

/// <summary>Một ô của làn: trạng thái và nhãn tác vụ (so sánh theo giá trị để gộp ô khi vẽ).</summary>
public readonly record struct O(string Tt, string Nhan);

public record Buoc(string Loai, int N);

public class ThamSo
{
    public readonly int[] D;              // nhịp
    public readonly int SoLoi, Lat, TKhan;
    public readonly bool PhuThuoc, ChiaCpu, Khan;

    public ThamSo(double t1 = 4.0, double t2 = 3.0, double t3 = 2.0, int soLoi = 2, double lat = 0.5,
                  bool phuThuoc = true, bool chia = true, bool khan = true, double tKhan = 0.3)
    {
        D = [LapLich.Nhip(t1), LapLich.Nhip(t2), LapLich.Nhip(t3)];
        SoLoi = Math.Max(1, soLoi);
        Lat = Math.Max(1, LapLich.Nhip(lat));
        PhuThuoc = phuThuoc; ChiaCpu = chia; Khan = khan;
        TKhan = LapLich.Nhip(tKhan);
    }
}

/// <summary>Một tác vụ: các bước (loại, số nhịp), loại = cpu | io | chia (CPU chia đôi chạy song song).</summary>
public class Viec(string ten, List<Buoc> buoc, int uuTien, int den, List<string> phuThuoc)
{
    public readonly string Ten = ten;
    public readonly List<Buoc> Buoc = buoc;
    public readonly int UuTien = uuTien, Den = den;
    public readonly List<string> PhuThuoc = phuThuoc;
    public int BatDau = -1, Xong = -1;    // nhịp đầu tiên dùng CPU, nhịp hoàn tất
    public int K;                         // (bất đồng bộ) bước đang làm
    public int ConCho;                    // (bất đồng bộ) số nửa của bước chia còn chưa xong
    public bool DaChay;                   // (bất đồng bộ) đã bắt đầu
}

/// <summary>Kết quả một lần chạy: các làn (tên, ô theo nhịp), số I/O đang chờ mỗi nhịp, nhật ký, tác vụ.</summary>
public class KetQua(string cheDo, ThamSo ts)
{
    public readonly string CheDo = cheDo;
    public readonly ThamSo Ts = ts;
    public List<(string Ten, List<O> O)> Lan = [];
    public readonly List<int> Io = [];
    public readonly List<(int T, string Text)> NhatKy = [];
    public List<Viec> Viec = [];
    public int ChenNgang, HetLat, Tong;

    public void Log(int t, string text) => NhatKy.Add((t, text));
}

// ======================================================================== đồng bộ (luồng hệ điều hành)
/// <summary>Một thao tác trong chương trình của luồng (cpu, io, cho_viec, tao_phu, cho_phu, ghi, xong, het).</summary>
class ThaoTac(string loai, int n, string nhan, Viec v)
{
    public readonly string Loai = loai;
    public int N = n;                         // số nhịp còn lại (cpu, io) hoặc độ dài nửa sau (tao_phu)
    public List<string> Dk = [];              // cho_viec: tên các tác vụ phải xong
    public Luong? Phu;                        // cho_phu: luồng phụ phải kết thúc
    public string Text = "";                  // ghi: dòng nhật ký
    public readonly string Nhan = nhan;
    public readonly Viec V = v;
}

class Luong
{
    public readonly string Ten;
    public readonly int UuTien;
    public List<ThaoTac> Ct;                  // chương trình: các thao tác chưa làm
    public ThaoTac? Op;                       // thao tác đang làm
    public bool KetThuc;
    public string TrangThai = LapLich.RANH;
    public int LatDaChay;
    public readonly List<O> O = [];

    public Luong(string ten, int uuTien, List<ThaoTac> ct, int t)
    {
        Ten = ten; UuTien = uuTien; Ct = ct;
        for (int i = 0; i < t; i++) O.Add(new O(LapLich.RANH, ""));   // luồng tạo muộn: các nhịp trước để trống
    }
}

class SanSang(Luong l, int stt)               // một mục của hàng sẵn sàng: luồng và số thứ tự vào hàng
{
    public readonly Luong L = l;
    public readonly int Stt = stt;
}

class DongBo
{
    readonly ThamSo ts;
    readonly bool nhieu;
    readonly KetQua kq;
    readonly List<Viec> viec;
    readonly List<Luong> luong = [];
    readonly List<SanSang> sanSang = [];
    readonly List<Luong> dangChay = [];
    int stt;
    readonly List<Viec> hang = [];            // đơn luồng: tác vụ đã đến, chờ luồng chính (FIFO)
    Luong? chinh;
    int t;

    public DongBo(ThamSo ts, bool nhieuLuong)
    {
        this.ts = ts; nhieu = nhieuLuong;
        kq = new KetQua(nhieuLuong ? "da_dong_bo" : "don_dong_bo", ts);
        viec = LapLich.TaoViec(ts);
        kq.Viec = viec;
    }

    void Log(string text) => kq.Log(t, text);

    bool DaXong(List<string> ds) => ds.All(d => viec.Any(v => v.Ten == d && v.Xong >= 0));

    /// <summary>Chương trình của tác vụ v: [chờ phụ thuộc], các bước, xong.</summary>
    List<ThaoTac> CtViec(Viec v, bool motLuong)
    {
        var ct = new List<ThaoTac>();
        if (v.PhuThuoc.Count > 0 && !motLuong)
            ct.Add(new ThaoTac("cho_viec", 0, v.Ten, v) { Dk = [.. v.PhuThuoc] });
        foreach (var b in v.Buoc)
        {
            if (b.Loai == "chia")
            {
                int a = LapLich.Div(b.N + 1, 2), c = LapLich.Div(b.N, 2);
                if (motLuong)
                {
                    ct.Add(new ThaoTac("ghi", 0, v.Ten, v) { Text = $"{v.Ten}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt" });
                    ct.Add(new ThaoTac("cpu", a, v.Ten, v));
                    ct.Add(new ThaoTac("cpu", c, v.Ten, v));
                }
                else
                {
                    ct.Add(new ThaoTac("tao_phu", c, v.Ten, v));
                    ct.Add(new ThaoTac("cpu", a, v.Ten, v));
                    ct.Add(new ThaoTac("cho_phu", 0, v.Ten, v));
                }
            }
            else
                ct.Add(new ThaoTac(b.Loai, b.N, v.Ten, v));
        }
        ct.Add(new ThaoTac("xong", 0, v.Ten, v));
        return ct;
    }

    Luong ThemLuong(string ten, int uuTien, List<ThaoTac> ct)
    {
        var l = new Luong(ten, uuTien, ct, t);
        luong.Add(l);
        return l;
    }

    void VaoSanSang(Luong l)
    {
        stt++;
        sanSang.Add(new SanSang(l, stt));
    }

    void RoiLoi(Luong l) => dangChay.Remove(l);

    /// <summary>Bước 2: luồng L làm các thao tác tức thời và sang thao tác kế tiếp nếu thao tác hiện tại đã xong.</summary>
    void Tien(Luong L)
    {
        while (true)
        {
            var op = L.Op;
            if (op != null)
            {
                string loai = op.Loai;
                if ((loai == "cpu" || loai == "io") && op.N > 0)
                    return;
                if (loai == "cho_viec")
                {
                    if (!DaXong(op.Dk))
                        return;
                    Log($"{L.Ten}: {string.Join(" và ", op.Dk)} đã xong (join), {op.Nhan} chạy tiếp");
                }
                if (loai == "cho_phu")
                {
                    if (!op.Phu!.KetThuc)
                        return;
                    Log($"{L.Ten}: hai nửa của {op.Nhan} đã xong (join {op.Phu.Ten})");
                }
            }
            if (L.Ct.Count == 0)
            {
                if (!nhieu && L == chinh && hang.Count > 0)
                {
                    var v = hang[0];
                    hang.RemoveAt(0);
                    Log($"Luồng chính bắt đầu {v.Ten}");
                    L.Ct = CtViec(v, true);
                }
                else
                {
                    L.Op = null;
                    RoiLoi(L);
                    if (nhieu || viec.All(v => v.Den <= t))
                        L.KetThuc = true;
                    L.TrangThai = LapLich.RANH;
                    return;
                }
            }
            L.Op = L.Ct[0];
            L.Ct.RemoveAt(0);
            op = L.Op;
            switch (op.Loai)
            {
                case "cpu":
                    if (!dangChay.Contains(L) && sanSang.All(s => s.L != L))
                        VaoSanSang(L);
                    L.TrangThai = dangChay.Contains(L) ? LapLich.CPU : LapLich.SAN_SANG;
                    return;
            }
            RoiLoi(L);
            switch (op.Loai)
            {
                case "io":
                    L.TrangThai = LapLich.IO;
                    return;
                case "cho_viec":
                    L.TrangThai = LapLich.CHO;
                    if (!DaXong(op.Dk))
                        Log($"{L.Ten}: {op.Nhan} chờ {string.Join(" và ", op.Dk)} xong (join)");
                    continue;
                case "tao_phu":
                    var phu = ThemLuong($"Luồng {op.Nhan}b", L.UuTien,
                                        [new ThaoTac("cpu", op.N, op.Nhan, op.V), new ThaoTac("het", 0, op.Nhan, op.V)]);
                    Log($"{op.Nhan}: chia CPU cuối 2 nửa, {phu.Ten} chạy nửa sau");
                    L.Ct[1].Phu = phu;               // thao tác cho_phu chờ đúng luồng phụ này
                    continue;
                case "cho_phu":
                    L.TrangThai = LapLich.CHO;
                    continue;
                case "ghi":
                    Log(op.Text);
                    continue;
                case "xong":
                    op.V.Xong = t;
                    Log($"{op.V.Ten} xong lúc {LapLich.Giay(t)} s");
                    continue;
                default:                             // het
                    continue;
            }
        }
    }

    SanSang? TotNhat()                               // ưu tiên cao nhất, cùng ưu tiên thì vào hàng trước
    {
        SanSang? best = null;
        foreach (var s in sanSang)
            if (best == null || s.L.UuTien > best.L.UuTien || (s.L.UuTien == best.L.UuTien && s.Stt < best.Stt))
                best = s;
        return best;
    }

    /// <summary>Bước 3: bộ lập lịch hệ điều hành cấp lõi CPU cho các luồng sẵn sàng.</summary>
    void LapLichLoi()
    {
        // a. hết lát thời gian: nhường lõi nếu có luồng ưu tiên bằng hoặc cao hơn đang chờ
        foreach (var r in dangChay.ToList())
        {
            if (r.LatDaChay >= ts.Lat && sanSang.Any(s => s.L.UuTien >= r.UuTien))
            {
                dangChay.Remove(r);
                r.LatDaChay = 0;
                VaoSanSang(r);
                kq.HetLat++;
                Log($"{r.Ten} hết lát thời gian {LapLich.Giay(ts.Lat)} s, nhường lõi");
            }
        }
        // b. lõi còn trống: cấp cho luồng sẵn sàng tốt nhất (ưu tiên cao, rồi đến trước)
        while (dangChay.Count < ts.SoLoi && sanSang.Count > 0)
        {
            var s = TotNhat()!;
            sanSang.Remove(s);
            s.L.LatDaChay = 0;
            dangChay.Add(s.L);
        }
        // c. chen ngang: luồng sẵn sàng ưu tiên cao hơn luồng đang chạy thì lấy lõi ngay
        while (sanSang.Count > 0)
        {
            var s = TotNhat()!;
            // nạn nhân: ưu tiên thấp nhất, rồi đã chạy nhiều nhất trong lát, rồi đứng đầu danh sách
            Luong? nan = null;
            foreach (var r in dangChay)
                if (r.UuTien < s.L.UuTien &&
                    (nan == null || r.UuTien < nan.UuTien || (r.UuTien == nan.UuTien && r.LatDaChay > nan.LatDaChay)))
                    nan = r;
            if (nan == null)
                break;
            int i = dangChay.IndexOf(nan);
            sanSang.Remove(s);
            s.L.LatDaChay = 0;
            dangChay[i] = s.L;
            nan.LatDaChay = 0;
            VaoSanSang(nan);
            kq.ChenNgang++;
            Log($"{s.L.Ten} CHEN NGANG: {nan.Ten} tạm dừng, nhường lõi");
        }
        foreach (var L in luong)
            if (L.Op != null && L.Op.Loai == "cpu" && L.Op.N > 0)
                L.TrangThai = dangChay.Contains(L) ? LapLich.CPU : LapLich.SAN_SANG;
    }

    public KetQua Chay()
    {
        while (true)
        {
            // 1. tác vụ đến
            foreach (var v in viec)
            {
                if (v.Den != t) continue;
                if (v.UuTien != 0)
                    Log($"{v.Ten} (việc khẩn, ưu tiên cao) đến");
                if (nhieu)
                {
                    var L = ThemLuong($"Luồng {v.Ten}", v.UuTien, CtViec(v, false));
                    if (v.UuTien != 0)
                        Log($"tạo {L.Ten}");
                }
                else
                {
                    hang.Add(v);
                    if (chinh == null)
                        chinh = ThemLuong("Luồng chính", 0, []);
                    else if (v.UuTien != 0 && (chinh.Op != null || chinh.Ct.Count > 0))
                        Log($"{v.Ten} xếp hàng: luồng chính bận, không chen ngang được");
                }
            }
            // 2. tiến (theo thứ tự tạo luồng; luồng mới tạo trong bước này cũng được tiến). Làm hai lượt để
            //    luồng đang join thấy ngay luồng có số thứ tự lớn hơn vừa kết thúc trong cùng nhịp.
            for (int lan = 0; lan < 2; lan++)
                for (int i = 0; i < luong.Count; i++)
                    if (!luong[i].KetThuc)
                        Tien(luong[i]);
            if (luong.All(L => L.KetThuc) && viec.All(v => v.Den <= t))
                break;
            // 3. lập lịch
            LapLichLoi();
            // 4. chạy một nhịp
            foreach (var L in luong)
            {
                var op = L.Op;
                if (L.KetThuc || op == null)
                    L.O.Add(new O(LapLich.RANH, ""));
                else if (L.TrangThai == LapLich.CPU)
                {
                    op.N--;
                    L.LatDaChay++;
                    if (op.V.BatDau < 0)
                        op.V.BatDau = t;
                    L.O.Add(new O(LapLich.CPU, op.Nhan));
                }
                else if (L.TrangThai == LapLich.IO)
                {
                    op.N--;
                    L.O.Add(new O(LapLich.IO, op.Nhan));
                }
                else
                    L.O.Add(new O(L.TrangThai, op.Nhan));
            }
            t++;
        }
        kq.Tong = t;
        kq.Lan = luong.Select(L => (L.Ten, L.O)).ToList();
        return kq;
    }
}

// ======================================================================== bất đồng bộ (vòng lặp / nhóm thợ)
class Phan(Viec v, int n, bool nua)           // một bước CPU (hoặc nửa bước chia) trong hàng / trên thợ
{
    public readonly Viec V = v;
    public int N = n;
    public readonly bool Nua = nua;
}

class BatDongBo
{
    readonly ThamSo ts;
    readonly bool nhieu;
    readonly KetQua kq;
    readonly List<Viec> viec;
    readonly string[] tenTho;
    readonly Phan?[] tho;                     // việc đang chạy trên mỗi thợ
    readonly List<O>[] o;
    readonly List<Phan> hang = [];            // các bước CPU sẵn sàng, FIFO
    readonly List<Phan> choIo = [];           // các lần chờ I/O đang diễn ra (không giữ thợ nào)
    readonly Dictionary<string, int> cuoiTho = [];   // tác vụ -> thợ chạy bước CPU gần nhất
    int t;

    public BatDongBo(ThamSo ts, bool nhieuLuong)
    {
        this.ts = ts; nhieu = nhieuLuong;
        kq = new KetQua(nhieuLuong ? "da_bat_dong_bo" : "don_bat_dong_bo", ts);
        viec = LapLich.TaoViec(ts);
        kq.Viec = viec;
        int soTho = nhieuLuong ? ts.SoLoi : 1;
        tenTho = nhieuLuong ? Enumerable.Range(1, soTho).Select(i => $"Thợ {i}").ToArray() : ["Vòng lặp"];
        tho = new Phan?[soTho];
        o = Enumerable.Range(0, soTho).Select(_ => new List<O>()).ToArray();
    }

    void Log(string text) => kq.Log(t, text);

    /// <summary>Tác vụ v bắt đầu bước v.K (hoặc hoàn tất nếu hết bước).</summary>
    void BatDauBuoc(Viec v)
    {
        if (v.K >= v.Buoc.Count)
        {
            v.Xong = t;
            Log($"{v.Ten} xong lúc {LapLich.Giay(t)} s");
            foreach (var w in viec)          // tác vụ đang chờ (await) các tác vụ vừa xong
            {
                if (!w.DaChay && w.Den <= t && w.PhuThuoc.Count > 0 && DuPhuThuoc(w))
                {
                    Log($"{string.Join(" và ", w.PhuThuoc)} đã xong → {w.Ten} bắt đầu (await)");
                    KhoiDong(w);
                }
            }
            return;
        }
        var b = v.Buoc[v.K];
        if (b.Loai == "cpu")
            hang.Add(new Phan(v, b.N, false));
        else if (b.Loai == "io")
            choIo.Add(new Phan(v, b.N, false));
        else                                 // chia: hai nửa vào hàng cùng lúc
        {
            v.ConCho = 2;
            hang.Add(new Phan(v, LapLich.Div(b.N + 1, 2), true));
            hang.Add(new Phan(v, LapLich.Div(b.N, 2), true));
            if (tho.Length > 1)
                Log($"{v.Ten}: chia CPU cuối 2 nửa, chạy song song trên 2 thợ");
            else
                Log($"{v.Ten}: chia CPU cuối 2 nửa, một luồng nên làm lần lượt");
        }
    }

    bool DuPhuThuoc(Viec v) => v.PhuThuoc.All(d => viec.Any(w => w.Ten == d && w.Xong >= 0));

    void KhoiDong(Viec v)
    {
        v.DaChay = true;
        v.K = 0;
        BatDauBuoc(v);
    }

    public KetQua Chay()
    {
        int soTho = tho.Length;
        while (true)
        {
            // 1. tác vụ đến
            foreach (var v in viec)
            {
                if (v.Den != t) continue;
                if (v.UuTien != 0)
                    Log($"{v.Ten} (việc khẩn) đến – xếp hàng, không chen ngang được");
                if (v.PhuThuoc.Count > 0 && !DuPhuThuoc(v))
                    Log($"{v.Ten} chờ {string.Join(" và ", v.PhuThuoc)} xong (await), không giữ luồng nào");
                else
                    KhoiDong(v);
            }
            // 2. tiến: bước CPU xong (theo thứ tự thợ), rồi I/O xong (theo thứ tự bắt đầu chờ)
            for (int i = 0; i < soTho; i++)
            {
                var x = tho[i];
                // "<= 0" thay cho "== 0" của bản Python: khi phần CPU cuối của T1 chỉ dài 1 nhịp (T1 = 0,1; 0,2;
                // 0,4 s) thì nửa sau dài 0 nhịp; với "== 0" nửa đó chạy mãi (N thành -1) và mô phỏng không dừng.
                // Với mọi tham số khác, N không bao giờ âm ở đây nên kết quả vẫn trùng bản Python.
                if (x != null && x.N <= 0)
                {
                    tho[i] = null;
                    var v = x.V;
                    if (x.Nua)
                    {
                        v.ConCho--;
                        if (v.ConCho == 0)
                        {
                            Log($"{v.Ten}: hai nửa đã xong, hợp kết quả");
                            v.K++;
                            BatDauBuoc(v);
                        }
                    }
                    else
                    {
                        v.K++;
                        BatDauBuoc(v);
                    }
                }
            }
            foreach (var x in choIo.ToList())
            {
                if (x.N == 0)
                {
                    choIo.Remove(x);
                    x.V.K++;
                    BatDauBuoc(x.V);
                }
            }
            if (viec.All(v => v.Xong >= 0))
                break;
            // 3. giao việc: thợ rảnh (theo thứ tự) lấy bước CPU đứng đầu hàng
            for (int i = 0; i < soTho; i++)
            {
                if (tho[i] == null && hang.Count > 0)
                {
                    var x = hang[0];
                    hang.RemoveAt(0);
                    tho[i] = x;
                    if (nhieu && cuoiTho.TryGetValue(x.V.Ten, out int truoc) && truoc != i && !x.Nua)
                        Log($"{x.V.Ten} chạy tiếp trên {tenTho[i]} (trước đó ở {tenTho[truoc]})");
                    cuoiTho[x.V.Ten] = i;
                }
            }
            // 4. chạy một nhịp
            for (int i = 0; i < soTho; i++)
            {
                var x = tho[i];
                if (x == null)
                    o[i].Add(new O(LapLich.RANH, ""));
                else
                {
                    x.N--;
                    if (x.V.BatDau < 0)
                        x.V.BatDau = t;
                    o[i].Add(new O(LapLich.CPU, x.V.Ten));
                }
            }
            kq.Io.Add(choIo.Count);
            foreach (var x in choIo)
                x.N--;
            t++;
        }
        kq.Tong = t;
        kq.Lan = Enumerable.Range(0, soTho).Select(i => (tenTho[i], o[i])).ToList();
        return kq;
    }
}
