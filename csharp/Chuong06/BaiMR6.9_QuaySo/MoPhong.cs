using System.Diagnostics;
using System.Text.Json.Nodes;

namespace BaiMR6_9;

// Bài mở rộng MR6.9 – Bộ mô phỏng tải: n người dùng ảo trên k kết nối, rào xuất phát, pipelining.
//
// Phần KHUNG (từ "KHUNG" trở xuống) giống nhau ở các bài tranh chấp; mỗi bài chỉ sửa phần NGHIỆP VỤ:
//   - NguCanh: số quà còn lại mà một kết nối biết, cập nhật từ cap_nhat;
//   - NguoiAo: kịch bản của MỘT người dùng ảo: quay `lan` lần (mặc định 1). Người thứ 100, 200, 300… dùng TÊN
//     của người liền trước (một người quay trên hai thiết bị cùng lúc) – máy chủ phải từ chối lượt thứ hai
//     ("da_quay"); lan > 1 thì các lượt sau cũng phải bị từ chối.
//   - ThamSoDatLai: {} – giữ số quà và xác suất p hiện tại của máy chủ (đặt bằng server --p, khach … dat_lai p=…,
//     hoặc ô tham số của server_gui).
//
// Một lượt: kết nối k socket (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi luồng đứng chờ
// ở RÀO XUẤT PHÁT (System.Threading.Barrier) -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người mình chở trong
// MỘT lần Send; trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian tổng, yc/s, khứ hồi
// p50/p95/p99, rồi hỏi thong_ke của máy chủ (sai phạm theo sổ kiểm toán).
//
// Chạy:  dotnet run -- mo_phong <host> <cổng> [--n 100000] [--k 200] [--lan 1] [--che-do khoa] [--dat-lai]
//                               [--so-sanh] [--tre-ms 2]
static class MoPhong
{
    // ===================================================================== NGHIỆP VỤ (MR6.9 – quay số)
    public const int SoLanMacDinh = 1;                   // mỗi người quay 1 lần
    public const int CongMacDinh = 42730;
    static readonly OrderedDictionary<string, object> ThamSoDatLai = [];   // giữ số quà, p hiện tại của máy chủ
    const int HaiThietBi = 100;                          // người thứ 100, 200, … quay bằng tên người liền trước

    /// <summary>Số quà còn lại mà một kết nối biết (chỉ để tham khảo – máy chủ mới quyết định).</summary>
    sealed class NguCanh(JsonObject trangThai)
    {
        public long ConLai = Chung.SoL(trangThai, "con_lai");

        public void CapNhat(JsonObject tin) => ConLai = Chung.SoL(tin, "con_lai");

        public void NhanKq(JsonObject tin) { }
    }

    /// <summary>Quay `lan` lần; người thứ 100, 200, … mang tên người liền trước (thiết bị thứ hai của người đó).</summary>
    sealed class NguoiAo
    {
        readonly string ten;
        int con;

        public NguoiAo(string ten, NguCanh nc, int lan)
        {
            int i = int.Parse(ten[1..], Chung.Ic);
            this.ten = i % HaiThietBi == 0 ? $"u{i - 1}" : ten;
            con = lan;
        }

        JsonObject? Quay()
        {
            if (con <= 0) return null;
            con--;
            return new JsonObject { ["loai"] = "quay", ["nguoi"] = ten };
        }

        public JsonObject? YeuCauDau() => Quay();

        public JsonObject? SauKq(JsonObject tin) => Quay();
    }

    // ===================================================================== KHUNG (giống nhau ở các bài)
    sealed class KetQuaKetNoi
    {
        public readonly List<double> Rtt = [];
        public long Ok, TuChoi;
        public readonly OrderedDictionary<string, long> LyDo = [];
        public string? Loi;
        public long TXong;
    }

    /// <summary>Luồng của một kết nối: chào -> chờ rào -> gửi lô yêu cầu đầu -> đọc, gửi tiếp tới khi xong.</summary>
    static void ChayKetNoi(string host, int cong, int i, List<string> dsTen, int lan, Barrier rao, CancellationTokenSource huy,
                           KetQuaKetNoi kq)
    {
        var kn = new KetNoiKhach();
        try
        {
            kn.KetNoi(host, cong, $"mp{i + 1}", soNguoi: dsTen.Count);
            var nc = new NguCanh(kn.TrangThai!);
            kn.KhiCapNhat = nc.CapNhat;
            int con = dsTen.Count;                       // số người chưa xong

            (string, JsonObject, Action<JsonObject, double>?)? Gui(NguoiAo na, JsonObject? yc)
            {
                if (yc == null)
                {
                    con--;
                    return null;
                }
                string loai = Chung.Chuoi(yc, "loai")!;
                yc.Remove("loai");
                return (loai, yc, (tin, rtt) => Nhan(na, tin, rtt));
            }

            void Nhan(NguoiAo na, JsonObject tin, double rtt)   // chạy trên chính luồng này (trong VongDoc)
            {
                kq.Rtt.Add(rtt);
                if (Chung.Dung(tin, "ok")) kq.Ok++;
                else
                {
                    kq.TuChoi++;
                    string m = Chung.Chuoi(tin, "ma_loi") ?? "khac";
                    kq.LyDo[m] = kq.LyDo.GetValueOrDefault(m) + 1;
                }
                nc.NhanKq(tin);
                if (Gui(na, na.SauKq(tin)) is { } tiep) kn.GuiYc(tiep.Item1, tiep.Item3, tiep.Item2);
            }

            var dsNguoi = dsTen.Select(ten => new NguoiAo(ten, nc, lan)).ToList();
            rao.SignalAndWait(huy.Token);                // RÀO XUẤT PHÁT: mọi kết nối sẵn sàng mới bắn
            var lo = dsNguoi.Select(na => Gui(na, na.YeuCauDau())).Where(x => x != null).Select(x => x!.Value).ToList();
            if (lo.Count > 0) kn.GuiLo(lo);
            kn.VongDoc(() => con == 0);
            if (con > 0) kq.Loi = "máy chủ đóng kết nối giữa chừng";
        }
        catch (Exception e)                              // lỗi trước rào: phá rào để luồng chính biết
        {
            kq.Loi = e.Message;
            huy.Cancel();
        }
        finally
        {
            kq.TXong = Stopwatch.GetTimestamp();
            kn.Dong();
        }
    }

    static string P(double? v) => v is { } x ? Chung.F(x, "F1") : "–";

    /// <summary>Một lượt mô phỏng; trả về một hàng của bảng so sánh hoặc null nếu lỗi.</summary>
    public static HangSoSanh? ChayLuot(string host, int cong, int n, int k, int lan, Action<string> bao, string? cheDo = null,
                                       bool datLai = false, double? treMs = null)
    {
        k = Math.Max(1, Math.Min(k, n));
        var qt = new QuanTri(host, cong, "mo_phong");
        try
        {
            if (treMs != null) qt.Tre(treMs.Value);
            if (datLai) qt.DatLai(ThamSoDatLai);         // đặt lại trước (máy chủ in tổng kết lượt trước đúng chế độ)
            if (cheDo != null) qt.CheDo(cheDo);
            cheDo = Chung.Chuoi(qt.ThongKe(), "che_do");
            bao($"=== {cheDo}: {n} người × {lan} lần, {k} kết nối ===");
            var nhom = Enumerable.Range(0, k).Select(_ => new List<string>()).ToList();
            for (int i = 0; i < n; i++) nhom[i % k].Add($"u{i + 1}");
            using var rao = new Barrier(k + 1);
            using var huy = new CancellationTokenSource();
            var ketQua = Enumerable.Range(0, k).Select(_ => new KetQuaKetNoi()).ToList();
            var luong = Enumerable.Range(0, k).Select(i => new Thread(() => ChayKetNoi(host, cong, i, nhom[i], lan, rao, huy, ketQua[i]))
            { IsBackground = true }).ToList();
            foreach (var t in luong) t.Start();
            try
            {
                if (!rao.SignalAndWait(TimeSpan.FromSeconds(60), huy.Token)) throw new OperationCanceledException();
            }
            catch (OperationCanceledException)
            {
                huy.Cancel();
                string loi = ketQua.FirstOrDefault(r => r.Loi != null)?.Loi ?? "hết thời gian chờ";
                bao($"Không mở được đủ {k} kết nối: {loi}");
                return null;
            }
            long t0 = Stopwatch.GetTimestamp();
            bao($"Rào xuất phát: {k}/{k} kết nối sẵn sàng -> bắn");
            foreach (var t in luong) t.Join();
            long t1 = ketQua.Max(r => r.TXong);
            var loiKn = ketQua.Where(r => r.Loi != null).Select(r => r.Loi!).ToList();
            if (loiKn.Count > 0) bao($"{loiKn.Count} kết nối lỗi: {loiKn[0]}");
            var rtt = ketQua.SelectMany(r => r.Rtt).Order().ToList();
            long ok = ketQua.Sum(r => r.Ok), tuChoi = ketQua.Sum(r => r.TuChoi);
            var lyDo = new OrderedDictionary<string, long>();
            foreach (var r in ketQua)
                foreach (var (m, s) in r.LyDo) lyDo[m] = lyDo.GetValueOrDefault(m) + s;
            long tong = ok + tuChoi;
            double giay = Math.Max((double)(t1 - t0) / Stopwatch.Frequency, 1e-6);
            long ycS = (long)Math.Round(tong / giay);
            var p = new[] { 50, 95, 99 }.ToDictionary(x => x, x => Chung.PhanVi(rtt, x));
            bao($"{tong} yc xong trong {Chung.F(giay, "F2")} s = {ycS} yc/s");
            bao("Khứ hồi ms: " + string.Join(" ", p.Select(x => $"p{x.Key} {P(x.Value)}")));
            bao($"Thành công {ok}, từ chối {tuChoi}");
            if (lyDo.Count > 0)
                bao("  lý do: " + string.Join(", ", lyDo.OrderByDescending(x => x.Value).Select(x => $"{x.Key} {x.Value}")));
            var tk = qt.ThongKe();
            bao($"Máy chủ: xung đột {Chung.SoL(tk, "xung_dot")}, thử lại {Chung.SoL(tk, "thu_lai")},"
                + $" hàng đợi max {Chung.SoL(tk, "hang_doi_max")}");
            bao($"Máy chủ xử lý µs: p50 {Chung.SoUs(tk["p50_us"])} p95 {Chung.SoUs(tk["p95_us"])} p99 {Chung.SoUs(tk["p99_us"])}");
            bao(Chung.DongSaiPham(tk));
            foreach (var v in (tk["vi_du_sai_pham"] as JsonArray ?? []).Take(2)) bao($"  ví dụ: {Chung.ChuoiPy(v)}");
            bao(Chung.Chuoi(tk, "tom_tat") ?? "");
            return new HangSoSanh(cheDo!, ok, Chung.SoL(tk, "sai_pham_tong"), Chung.SoL(tk, "xung_dot"), ycS, p[99], tuChoi,
                                  Chung.Chuoi(tk, "tom_tat") ?? "");
        }
        finally { qt.Dong(); }
    }

    /// <summary>Chạy lần lượt 4 chế độ (đặt lại trạng thái trước mỗi lần); trả về các hàng; trả lại chế độ cũ.</summary>
    public static List<HangSoSanh> SoSanh(string host, int cong, int n, int k, int lan, Action<string> bao, double? treMs = null)
    {
        var qt = new QuanTri(host, cong, "so_sanh");
        try
        {
            string goc = Chung.Chuoi(qt.ThongKe(), "che_do")!;
            var hang = new List<HangSoSanh>();
            foreach (var cd in Chung.CheDo)
            {
                var h = ChayLuot(host, cong, n, k, lan, bao, cd, true, treMs);
                if (h == null) break;
                hang.Add(h);
            }
            qt.CheDo(goc);
            return hang;
        }
        finally { qt.Dong(); }
    }

    public static int Run(string[] args)
    {
        var a = new DoiSo(args, "--dat-lai", "--so-sanh");
        string host = a.VT(0, "127.0.0.1");
        int cong = int.Parse(a.VT(1, CongMacDinh.ToString(Chung.Ic)), Chung.Ic);
        int n = a.Int("--n", 200), lan = a.Int("--lan", SoLanMacDinh);
        int k = a.Int("--k", 0) is > 0 and var kk ? kk : Math.Min(n, 200);
        string? cheDo = a.Lay("--che-do");
        if (cheDo != null && !Chung.CheDo.Contains(cheDo))
        {
            Console.WriteLine($"--che-do: chọn một trong {string.Join(", ", Chung.CheDo)}");
            return 2;
        }
        static void Bao(string text) => Console.WriteLine($"[mo_phong] {text}");
        if (a.Co("--so-sanh"))
        {
            var hang = SoSanh(host, cong, n, k, lan, Bao, a.DoubleN("--tre-ms"));
            Bao($"Bảng so sánh ({n} người × {lan} lần, {Math.Min(k, n)} kết nối):");
            foreach (var d in Chung.BangSoSanh(hang)) Console.WriteLine(d);
        }
        else ChayLuot(host, cong, n, k, lan, Bao, cheDo, a.Co("--dat-lai"), a.DoubleN("--tre-ms"));
        return 0;
    }
}
