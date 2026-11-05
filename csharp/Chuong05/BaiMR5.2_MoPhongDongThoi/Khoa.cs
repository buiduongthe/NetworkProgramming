using static BaiMR5_2.DongHo;

namespace BaiMR5_2;

// Tranh chấp dữ liệu (race condition), khóa, thao tác nguyên tử và tránh bế tắc (deadlock).
// 1. Tranh chấp: 4 luồng cùng "đọc – nhường CPU – ghi" lên một biến đếm chung. Bước nhường CPU
//    (Thread.Yield(): giống gọi mạng hay ghi nhật ký giữa chừng) cho phép luồng khác chen vào giữa lúc
//    đọc và lúc ghi → mất lần cộng. Sửa bằng lock, hoặc Interlocked.Increment (cộng trong một lệnh).
//    Không có GIL nên trên máy nhiều CPU các luồng còn chạy song song thật – lỗi lộ ra rất nhanh.
// 2. Bế tắc: luồng T1 giữ khóa X rồi chờ khóa Y, luồng T2 giữ Y rồi chờ X – cả hai chờ nhau mãi.
//    Bài dùng Monitor.TryEnter(khóa, 500 ms) để phát hiện thay vì treo chương trình; cách tránh là luôn
//    lấy khóa theo cùng một thứ tự.
static class Khoa
{
    const int NThreads = 4, NIter = 5_000;

    class Counter { public long Value; }

    static void RaceDemo()
    {
        Console.WriteLine($"Tranh chấp dữ liệu: {NThreads} luồng × {NIter} lần \"đọc – nhường CPU – ghi\"");
        Console.WriteLine($"  lên một biến đếm chung (kết quả đúng phải là {NThreads * NIter}).");
        var guard = new object();
        var cases = new (string Name, Action<Counter> Body)[]
        {
            ("không khóa", c =>
            {
                for (int i = 0; i < NIter; i++)
                {
                    long v = c.Value;          // đọc
                    Thread.Yield();            // nhường CPU: luồng khác có thể chen vào đây
                    c.Value = v + 1;           // ghi – có thể đè lên kết quả của luồng khác
                }
            }),
            ("có khóa (lock)", c =>
            {
                for (int i = 0; i < NIter; i++)
                {
                    lock (guard)               // đoạn găng: chỉ một luồng được vào
                    {
                        long v = c.Value;
                        Thread.Yield();
                        c.Value = v + 1;
                    }
                }
            }),
            ("Interlocked", c =>
            {
                for (int i = 0; i < NIter; i++)
                {
                    Interlocked.Increment(ref c.Value);   // đọc – cộng – ghi trong một lệnh của CPU
                    Thread.Yield();
                }
            }),
        };
        foreach (var (name, body) in cases)
        {
            var c = new Counter();
            double t = Now();
            var ths = Enumerable.Range(0, NThreads).Select(_ => new Thread(() => body(c))).ToList();
            ths.ForEach(th => th.Start());
            ths.ForEach(th => th.Join());
            double dt = Now() - t;
            int total = NThreads * NIter;
            string verdict = c.Value == total ? "đúng" : $"mất {total - c.Value} lần cộng";
            Console.WriteLine($"  {name,-16}: {c.Value,6} / {total} → {verdict} ({F(dt, "F2")} s)");
        }
    }

    static void DeadlockDemo()
    {
        Console.WriteLine();
        Console.WriteLine("Bế tắc (deadlock): T1 chuyển X→Y, T2 chuyển Y→X, mỗi luồng cần cả");
        Console.WriteLine("  hai khóa; số dư ban đầu X = 100, Y = 100.");
        var bal = new Dictionary<string, int> { ["X"] = 100, ["Y"] = 100 };
        var locks = new Dictionary<string, object> { ["X"] = new(), ["Y"] = new() };
        var outLock = new object();

        void Say(string text)
        {
            lock (outLock) Console.WriteLine($"  {text}");
        }

        void Transfer(string name, string src, string dst, int amount, bool ordered)
        {
            var (first, second) = ordered ? (string.CompareOrdinal(src, dst) < 0 ? (src, dst) : (dst, src)) : (src, dst);
            lock (locks[first])
            {
                Say($"{name} giữ khóa {first}, chờ khóa {second}");
                Thread.Sleep(50);                          // để luồng kia kịp giữ khóa còn lại
                if (!Monitor.TryEnter(locks[second], 500))
                {
                    Say($"{name}: chờ {second} quá 0,5 s → nghi bế tắc, nhả {first}, bỏ cuộc");
                    return;
                }
                try
                {
                    bal[src] -= amount;
                    bal[dst] += amount;
                    Say($"{name}: đã chuyển {amount} từ {src} sang {dst}");
                }
                finally
                {
                    Monitor.Exit(locks[second]);
                }
            }
        }

        foreach (bool ordered in new[] { false, true })
        {
            if (ordered) Console.WriteLine("Cách tránh: mọi luồng lấy khóa theo cùng thứ tự (X trước, Y sau):");
            var t1 = new Thread(() => Transfer("T1", "X", "Y", 10, ordered));
            var t2 = new Thread(() => Transfer("T2", "Y", "X", 5, ordered));
            t1.Start();
            Thread.Sleep(10);
            t2.Start();
            t1.Join();
            t2.Join();
            Console.WriteLine($"  Số dư: X = {bal["X"]}, Y = {bal["Y"]} (tổng {bal["X"] + bal["Y"]})");
        }
    }

    public static void Run()
    {
        RaceDemo();
        DeadlockDemo();
    }
}
