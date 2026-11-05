using System.Diagnostics;
using System.Globalization;

namespace BaiMR5_2;

// Ghi lại khoảng thời gian chạy của từng tác vụ và vẽ biểu đồ Gantt bằng văn bản.
// Mỗi đoạn ghi (tác vụ, worker, bắt đầu, kết thúc) theo đồng hồ Stopwatch – đồng hồ đơn điệu dùng chung
// cho mọi luồng. Mỗi hàng của biểu đồ là một worker (luồng, vòng lặp, "chờ mạng"); mỗi cột là một
// khoảng thời gian bằng nhau:
//     chữ cái  = đúng một tác vụ đang chạy trên worker đó (A = tác vụ 1, B = tác vụ 2, ...)
//     *        = nhiều tác vụ cùng lúc trên một worker (đặc trưng của mô hình bất đồng bộ)
// Ký tự của một cột lấy theo thời điểm giữa cột.
static class DongHo
{
    public const int Cols = 48;
    const string Letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    /// <summary>Thời điểm hiện tại (giây) theo đồng hồ đơn điệu.</summary>
    public static double Now() => Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;

    public static string F(double v, string fmt) => v.ToString(fmt, CultureInfo.InvariantCulture);

    public static void Gantt(string title, Recorder rec, double total, double scale, string kind = "luồng", int maxRows = 8)
    {
        Console.WriteLine($"{title}: {F(total, "F2")} s");
        var segs = rec.Snapshot();
        var workers = new List<string>();
        foreach (var s in segs.OrderBy(s => s.Start))
            if (!workers.Contains(s.Worker)) workers.Add(s.Worker);
        foreach (var w in workers.Take(maxRows))
        {
            var mine = segs.Where(s => s.Worker == w).ToList();
            var cells = new char[Cols];
            for (int c = 0; c < Cols; c++)
            {
                double lo = scale * c / Cols, hi = scale * (c + 1) / Cols, mid = (lo + hi) / 2;
                var now = mine.Where(s => s.Start <= mid && mid < s.End).ToList();   // đang chạy giữa cột
                if (now.Count >= 2) cells[c] = '*';                                  // nhiều tác vụ cùng lúc
                else if (now.Count == 1) cells[c] = Letters[now[0].Task];
                else                                                                 // tác vụ ngắn hơn một cột
                {
                    var on = mine.FirstOrDefault(s => s.Start < hi && s.End > lo);
                    cells[c] = on != null ? Letters[on.Task] : ' ';
                }
            }
            Console.WriteLine($"  {w,-12}|{new string(cells)}|");
        }
        if (workers.Count > maxRows)
            Console.WriteLine($"  … và {workers.Count - maxRows} {kind} khác, mỗi {kind} chạy 1 tác vụ");
    }

    public static void Axis(double scale)
    {
        string right = $"{F(scale, "F1")} s";
        Console.WriteLine($"  {"",-12} 0{right.PadLeft(Cols)}");
    }

    /// <summary>rows: (tên mô hình, thời gian); hàng đầu là mô hình tuần tự (mốc so sánh).</summary>
    public static void Table(List<(string Name, double T)> rows)
    {
        double b = rows[0].T;
        Console.WriteLine($"  {"Mô hình",-30} {"Thời gian",9} {"Tăng tốc",9}");
        foreach (var (name, t) in rows)
            Console.WriteLine($"  {name,-30} {F(t, "F2"),7} s {F(b / t, "F1"),8}×");
    }
}

record Seg(int Task, string Worker, double Start, double End);

/// <summary>Bộ ghi các đoạn chạy; an toàn khi nhiều luồng cùng ghi.</summary>
class Recorder
{
    readonly double t0 = DongHo.Now();
    readonly List<Seg> segs = [];
    readonly object khoa = new();

    /// <summary>start, end: giá trị DongHo.Now() tuyệt đối.</summary>
    public void Add(int task, string worker, double start, double end)
    {
        lock (khoa) segs.Add(new Seg(task, worker, start - t0, end - t0));
    }

    public List<Seg> Snapshot()
    {
        lock (khoa) return [.. segs];
    }

    public double Total() => Snapshot().Max(s => s.End);
}

/// <summary>Đặt tên dễ đọc cho worker: luồng-1, luồng-2, ... theo thứ tự xuất hiện.</summary>
class Names(string prefix)
{
    readonly Dictionary<int, string> map = [];

    public string Get(int key)
    {
        lock (map)
        {
            if (!map.TryGetValue(key, out var n)) map[key] = n = $"{prefix}-{map.Count + 1}";
            return n;
        }
    }

    /// <summary>Tên của luồng đang chạy (theo ManagedThreadId).</summary>
    public string Current() => Get(Environment.CurrentManagedThreadId);
}
