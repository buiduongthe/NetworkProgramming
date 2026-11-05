using System.Globalization;
using System.Text.Json.Nodes;
using static BaiMR6_2.Chung;

namespace BaiMR6_2;

// Phân tích bản ghi WAV: âm nào có mặt, ở mức bao nhiêu, trong từng đoạn.
// Mỗi client phát một âm sin có tần số riêng, nên đo mức ở từng tần số bằng thuật toán Goertzel là biết
// trong bản ghi "ai đang nói" và to bao nhiêu. Bản ghi được chia đoạn theo các mốc sự kiện mà client đã lưu
// (tệp <tên>_moc.json); bỏ qua 0,15 s đầu mỗi đoạn vì âm thanh tới tai người nghe trễ hơn thông báo TCP
// (độ sâu bộ đệm jitter + một nhịp trộn). Nếu không có tệp mốc, chia đều mỗi --cua-so giây.
static class PhanTichWav
{
    const double BoDau = 0.15;        // giây bỏ qua ở đầu mỗi đoạn (độ trễ)
    const double NganNhat = 0.3;      // đoạn ngắn hơn (sau khi bỏ đầu) thì gộp vào mốc kế tiếp

    public static int Run(string[] args)
    {
        var pos = new List<string>();
        double cuaSo = 1.0, nguong = -50.0;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--cua-so") cuaSo = double.Parse(args[++i], CultureInfo.InvariantCulture);
            else if (args[i] == "--nguong") nguong = double.Parse(args[++i], CultureInfo.InvariantCulture);
            else pos.Add(args[i]);
        }
        string wav = pos[0];
        var am = pos.Skip(1).Select(x => (F: double.Parse(x.Split(':')[0], CultureInfo.InvariantCulture), N: x.Split(':')[1])).ToList();
        var mau = DocWav(wav);
        double dai = mau.Length / (double)TanSoMau;
        Console.WriteLine($"[phan_tich] {wav}: {F(dai, "F2")} s, 16000 Hz, 16 bit, mono");
        Console.WriteLine($"[phan_tich] Goertzel, cửa sổ Hann; RMS dBFS; --- = vắng (<{nguong.ToString(CultureInfo.InvariantCulture)})");

        // ---- Mốc chia đoạn
        string duongMoc = Path.Combine(Path.GetDirectoryName(wav) ?? "", Path.GetFileNameWithoutExtension(wav) + "_moc.json");
        string? tenMinh = null;
        var moc = new List<(double T, string S)>();
        if (File.Exists(duongMoc))
        {
            var d = JsonNode.Parse(File.ReadAllText(duongMoc))!.AsObject();
            tenMinh = d["ten"]?.GetValue<string>();
            moc.Add((0.0, "bắt đầu ghi"));
            foreach (var m in d["moc"]!.AsArray()) moc.Add((m!["t"]!.GetValue<double>(), m["su_kien"]!.GetValue<string>()));
        }
        else
            for (int i = 0; i < (int)(dai / cuaSo); i++) moc.Add((i * cuaSo, $"cửa sổ {i + 1}"));
        var nhom = new List<(double T, List<string> Ds)>();       // gộp các mốc quá gần nhau thành một
        foreach (var (t, s) in moc)
        {
            if (nhom.Count > 0 && t - nhom[^1].T < BoDau + NganNhat) nhom[^1].Ds.Add(s);
            else nhom.Add((t, [s]));
        }

        // ---- Đo mức từng âm trong từng đoạn
        Console.WriteLine("Đoạn  Từ-đến (s) " + string.Concat(am.Select(x => $"{x.N,2} {x.F.ToString(CultureInfo.InvariantCulture),-4}")) + " Thay đổi");
        double[]? truoc = null;
        var ketQua = new List<double[]>();
        for (int i = 0; i < nhom.Count; i++)
        {
            double dau = nhom[i].T + BoDau;
            double cuoi = i + 1 < nhom.Count ? nhom[i + 1].T : dai;
            if (cuoi - dau < 0.1) continue;
            var doan = mau.AsSpan((int)(dau * TanSoMau), (int)(cuoi * TanSoMau) - (int)(dau * TanSoMau));
            var muc = new double[am.Count];
            for (int j = 0; j < am.Count; j++)
            {
                double m = MucAmDbfs(doan, am[j].F);
                muc[j] = m >= nguong ? m : double.NegativeInfinity;
            }
            var doi = new List<string>();
            if (truoc != null)
            {
                for (int j = 0; j < am.Count; j++)
                {
                    double m0 = truoc[j], m1 = muc[j];
                    if (double.IsNegativeInfinity(m0) && !double.IsNegativeInfinity(m1)) doi.Add($"{am[j].N} bật");
                    else if (!double.IsNegativeInfinity(m0) && double.IsNegativeInfinity(m1)) doi.Add($"{am[j].N} tắt");
                    else if (!double.IsNegativeInfinity(m0) && Math.Abs(m1 - m0) >= 1.0)
                        doi.Add($"{am[j].N} {(m1 - m0 >= 0 ? "+" : "")}{F(m1 - m0, "F1")}dB");
                }
            }
            Console.WriteLine($"{i + 1,3}   {F(dau, "F2"),4}-{F(cuoi, "F2"),4} " + string.Concat(muc.Select(m => ChuDb(m, 7)))
                              + "  " + string.Join(", ", doi));
            truoc = muc;
            ketQua.Add(muc);
        }
        Console.WriteLine("Sự kiện mở đầu mỗi đoạn:");
        for (int i = 0; i < nhom.Count; i++)
            Console.WriteLine($"{i + 1,3} {F(nhom[i].T, "F2"),5}s " + string.Join("; ", nhom[i].Ds));

        // ---- Kiểm tra mix-minus: người nghe không được nghe tiếng của chính mình
        for (int j = 0; j < am.Count; j++)
        {
            if (am[j].N != tenMinh) continue;
            double lonNhat = ketQua.Max(m => m[j]);
            bool ok = double.IsNegativeInfinity(lonNhat);
            Console.WriteLine($"Mix-minus: {am[j].N} không nghe tiếng mình ({am[j].F.ToString(CultureInfo.InvariantCulture)} Hz): " +
                              (ok ? "vắng mọi đoạn -> ĐÚNG" : $"{F(lonNhat, "F1")} dBFS -> SAI"));
        }
        return 0;
    }
}
