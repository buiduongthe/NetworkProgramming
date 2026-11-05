using System.Collections.Concurrent;
using System.Net.Sockets;
using System.Text;
using static BaiMR5_2.DongHo;

namespace BaiMR5_2;

// So sánh các mô hình đồng thời của .NET trên cùng một khối việc.
// Ba khối việc:
//   mang  – 20 yêu cầu mạng tới máy chủ thử (mỗi yêu cầu trễ 200 ms): việc chờ I/O;
//   cpu   – 8 tác vụ tính toán (mỗi tác vụ ≈ 100 ms): việc nặng CPU;
//   tron  – 20 tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms: async/await + đẩy CPU sang luồng khác.
// Mỗi mô hình in biểu đồ Gantt (tác vụ nào chạy trên worker nào, khi nào) và tổng thời gian.
// .NET không có GIL: các luồng (Thread, ThreadPool, Task, Parallel) chạy song song thật trên mọi CPU.
static class DongThoi
{
    const int NNet = 20, NCpu = 8;
    const int Pause = 500;           // nghỉ giữa hai mô hình (ms) để máy chủ thử báo từng "đợt" riêng
    static volatile uint sink;       // giữ kết quả để trình biên dịch JIT không bỏ vòng lặp CPU

    // ---------------- việc mạng (chặn / bất đồng bộ) và việc CPU ----------------
    /// <summary>Một yêu cầu mạng kiểu chặn (blocking): kết nối, gửi GET, chờ dòng trả lời.</summary>
    static string FetchBlocking(string host, int port, int k)
    {
        using var s = new Socket(SocketType.Stream, ProtocolType.Tcp) { ReceiveTimeout = 5000 };
        s.Connect(host, port);
        s.Send(Encoding.ASCII.GetBytes($"GET {k}\n"));
        var sb = new StringBuilder();
        var buf = new byte[64];
        int n;
        while ((n = s.Receive(buf)) > 0)
        {
            sb.Append(Encoding.ASCII.GetString(buf, 0, n));
            if (sb.ToString().Contains('\n')) break;
        }
        return sb.ToString().Trim();
    }

    /// <summary>Cùng yêu cầu nhưng bất đồng bộ: trong lúc chờ, luồng được trả lại để làm việc khác.</summary>
    static async Task<string> FetchAsync(string host, int port, int k)
    {
        using var s = new Socket(SocketType.Stream, ProtocolType.Tcp);
        await s.ConnectAsync(host, port);
        await s.SendAsync(Encoding.ASCII.GetBytes($"GET {k}\n"));
        var sb = new StringBuilder();
        var buf = new byte[64];
        int n;
        while ((n = await s.ReceiveAsync(buf)) > 0)
        {
            sb.Append(Encoding.ASCII.GetString(buf, 0, n));
            if (sb.ToString().Contains('\n')) break;
        }
        return sb.ToString().Trim();
    }

    /// <summary>Việc tính toán thuần (bộ sinh đồng dư tuyến tính), giống bản Python và Go.</summary>
    static uint CpuWork(long n)
    {
        long x = 1;
        for (long i = 0; i < n; i++) x = (x * 1103515245 + 12345) & 0x7FFFFFFF;
        return (uint)x;
    }

    /// <summary>Tìm số vòng lặp để CpuWork chạy ≈ ms mili giây trên máy này.</summary>
    static long Calibrate(int ms)
    {
        long n = 20_000_000;
        double dt = 1.0;
        for (int i = 0; i < 5; i++)          // đo nhiều lần, lấy lần nhanh nhất (lần đầu JIT còn chưa tối ưu)
        {
            double t = Now();
            sink = CpuWork(n);
            dt = Math.Min(dt, Now() - t);
        }
        return (long)(n * ms / 1000.0 / dt);
    }

    static void Show(List<(string Name, Recorder Rec, string Kind)> results)
    {
        double scale = results.Max(r => r.Rec.Total());
        Console.WriteLine();
        foreach (var (name, rec, kind) in results) Gantt(name, rec, rec.Total(), scale, kind);
        Axis(scale);
        Console.WriteLine();
        Table(results.Select(r => (r.Name, r.Rec.Total())).ToList());
        Console.WriteLine();
    }

    // ---------------- khối việc 1: mạng ----------------
    public static void RunNet(string host, int port)
    {
        Console.WriteLine($"Khối việc: {NNet} yêu cầu mạng, mỗi yêu cầu máy chủ trễ 200 ms");
        ThreadPool.GetMinThreads(out int minW, out _);
        var results = new List<(string, Recorder, string)>();

        void Timed(Recorder rec, Names names, int k)
        {
            string w = names.Current();                  // đặt tên worker lúc bắt đầu tác vụ
            double t = Now();
            FetchBlocking(host, port, k);
            rec.Add(k, w, t, Now());
        }

        // 1. Tuần tự (đồng bộ)
        var rec = new Recorder();
        for (int k = 0; k < NNet; k++)
        {
            double t = Now();
            FetchBlocking(host, port, k);
            rec.Add(k, "chính", t, Now());
        }
        results.Add(("1. tuần tự", rec, "luồng"));
        Thread.Sleep(Pause);

        // 2. Mỗi tác vụ một luồng (System.Threading.Thread)
        rec = new Recorder();
        var names = new Names("luồng");
        var ths = Enumerable.Range(0, NNet).Select(k => new Thread(() => Timed(rec, names, k))).ToList();
        ths.ForEach(th => th.Start());
        ths.ForEach(th => th.Join());
        results.Add(("2. mỗi tác vụ một luồng", rec, "luồng"));
        Thread.Sleep(Pause);

        // 3. Nhóm 5 luồng tự tạo: 5 luồng lấy việc từ một hàng đợi chung
        rec = new Recorder();
        names = new Names("luồng");
        var jobs = new ConcurrentQueue<int>(Enumerable.Range(0, NNet));
        var pool = Enumerable.Range(0, 5).Select(_ => new Thread(() =>
        {
            while (jobs.TryDequeue(out int k)) Timed(rec, names, k);
        })).ToList();
        pool.ForEach(th => th.Start());
        pool.ForEach(th => th.Join());
        results.Add(("3. nhóm 5 luồng tự tạo", rec, "luồng"));
        Thread.Sleep(Pause);

        // 4. ThreadPool + gọi CHẶN: Task.Run cho 20 việc chờ mạng – ThreadPool chỉ có sẵn ít luồng
        //    (tối thiểu = số CPU) và thêm luồng từ từ, nên các việc chặn phải xếp hàng (thread-pool starvation).
        rec = new Recorder();
        names = new Names("luồng");
        var r4 = rec; var n4 = names;
        Task.WaitAll(Enumerable.Range(0, NNet).Select(k => Task.Run(() => Timed(r4, n4, k))).ToArray());
        results.Add(("4. Task.Run + gọi chặn", rec, "luồng"));
        Thread.Sleep(Pause);

        // 5. async/await: 20 Task bất đồng bộ; lúc chờ mạng không giữ luồng nào ("chờ mạng" = hàng chung)
        rec = new Recorder();
        var r5 = rec;
        async Task One(int k)
        {
            double t = Now();
            await FetchAsync(host, port, k);
            r5.Add(k, "chờ mạng", t, Now());
        }
        Task.WhenAll(Enumerable.Range(0, NNet).Select(One)).GetAwaiter().GetResult();
        results.Add(("5. async/await (Task)", rec, "luồng"));
        Show(results);
        Console.WriteLine($"Nhận xét: việc chờ I/O không tốn CPU, nên luồng hay async đều chồng được");
        Console.WriteLine($"  thời gian chờ. Mô hình 4 chậm vì gọi hàm CHẶN trên ThreadPool (tối thiểu");
        Console.WriteLine($"  {minW} luồng, thêm dần khi thiếu); async/await (5) không giữ luồng khi chờ (*).");
    }

    // ---------------- khối việc 2: CPU ----------------
    public static void RunCpu()
    {
        long n = Calibrate(100);
        int ncpu = Environment.ProcessorCount;
        ThreadPool.GetMinThreads(out int minW, out _);
        Console.WriteLine($"Khối việc: {NCpu} tác vụ CPU, mỗi tác vụ ≈ 100 ms");
        Console.WriteLine($"Máy có {ncpu} CPU; .NET không có GIL; ThreadPool tối thiểu {minW} luồng");
        var results = new List<(string, Recorder, string)>();

        var rec = new Recorder();
        for (int k = 0; k < NCpu; k++)
        {
            double t = Now();
            sink = CpuWork(n);
            rec.Add(k, "chính", t, Now());
        }
        results.Add(("1. tuần tự", rec, "luồng"));

        void Timed(Recorder r, Names names, int k)
        {
            string w = names.Current();
            double t = Now();
            sink = CpuWork(n);
            r.Add(k, w, t, Now());
        }

        // 2. Mỗi tác vụ một Thread
        rec = new Recorder();
        var names = new Names("luồng");
        var r2 = rec; var n2 = names;
        var ths = Enumerable.Range(0, NCpu).Select(k => new Thread(() => Timed(r2, n2, k))).ToList();
        ths.ForEach(th => th.Start());
        ths.ForEach(th => th.Join());
        results.Add(("2. mỗi tác vụ một luồng", rec, "luồng"));

        // 3. Task.Run: các tác vụ xếp hàng trên ThreadPool
        rec = new Recorder();
        names = new Names("luồng");
        var r3 = rec; var n3 = names;
        Task.WaitAll(Enumerable.Range(0, NCpu).Select(k => Task.Run(() => Timed(r3, n3, k))).ToArray());
        results.Add(("3. Task.Run (ThreadPool)", rec, "luồng"));

        // 4. Parallel.For, số việc song song = số CPU
        rec = new Recorder();
        names = new Names("luồng");
        var r4 = rec; var n4 = names;
        Parallel.For(0, NCpu, new ParallelOptions { MaxDegreeOfParallelism = ncpu }, k => Timed(r4, n4, k));
        results.Add(($"4. Parallel.For ({ncpu} = số CPU)", rec, "luồng"));
        Show(results);
        Console.WriteLine("Nhận xét: .NET không có GIL nên mọi cách dùng luồng đều chạy song song");
        Console.WriteLine("  thật, nhanh gần gấp số CPU. Mô hình 2 tạo 8 luồng nhưng chỉ có số CPU");
        Console.WriteLine("  luồng chạy cùng lúc (mỗi tác vụ bị kéo dài vì chia lượt); ThreadPool và");
        Console.WriteLine("  Parallel.For giữ số luồng gần số CPU nên mỗi tác vụ chạy liền một mạch.");
    }

    // ---------------- khối việc 3: mạng + CPU ----------------
    class Lag
    {
        double max;
        public volatile bool Stop;
        public void Add(double v) { lock (this) max = Math.Max(max, v); }
        public double Max { get { lock (this) return max; } }
    }

    static async Task Heartbeat(Lag lag)
    {
        while (!lag.Stop)
        {
            double t = Now();
            await Task.Delay(10);
            lag.Add(Now() - t - 0.010);
        }
    }

    public static void RunMixed(string host, int port)
    {
        long n = Calibrate(30);
        Console.WriteLine($"Khối việc: {NNet} tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms");
        Console.WriteLine("Đo thêm độ trễ: tác vụ \"nhịp tim\" chờ 10 ms (await Task.Delay) và đo xem");
        Console.WriteLine("  thực tế được chạy tiếp muộn bao nhiêu (trễ lớn = ứng dụng đứng).");

        // mode: "pool" (không có SynchronizationContext), "inline", "task", "sem" (vòng lặp một luồng)
        async Task<double> Scenario(string mode, Recorder rec)
        {
            var lag = new Lag();
            var names = new Names("luồng");
            var sem = new SemaphoreSlim(2);
            string loopName = mode == "pool" ? "chờ mạng" : "vòng lặp";

            void Job(int k)
            {
                string w = names.Current();
                double t1 = Now();
                sink = CpuWork(n);
                rec.Add(k, w, t1, Now());
            }

            async Task One(int k)
            {
                double t = Now();
                await FetchAsync(host, port, k);
                rec.Add(k, loopName, t, Now());
                switch (mode)
                {
                    case "pool": Job(k); break;                    // CPU ngay sau await, trên luồng ThreadPool
                    case "inline":                                 // CPU ngay trên luồng vòng lặp: chặn mọi thứ
                        double t2 = Now();
                        sink = CpuWork(n);
                        rec.Add(k, "vòng lặp", t2, Now());
                        break;
                    case "task": await Task.Run(() => Job(k)); break;   // đẩy CPU sang ThreadPool
                    default:                                       // như "task" nhưng tối đa 2 việc CPU cùng lúc
                        await sem.WaitAsync();
                        try { await Task.Run(() => Job(k)); }
                        finally { sem.Release(); }
                        break;
                }
            }

            var hb = Heartbeat(lag);
            await Task.WhenAll(Enumerable.Range(0, NNet).Select(One));
            lag.Stop = true;
            await hb;
            return lag.Max;
        }

        var results = new List<(string, Recorder, string)>();
        var lags = new List<double>();
        var rec = new Recorder();
        var ra = rec;
        lags.Add(Task.Run(() => Scenario("pool", ra)).GetAwaiter().GetResult());
        results.Add(("A. CPU sau await (ThreadPool)", rec, "luồng"));
        Thread.Sleep(Pause);
        rec = new Recorder();
        var rb = rec;
        lags.Add(VongLap.Run(() => Scenario("inline", rb)));
        results.Add(("B. 1 luồng, CPU trong vòng lặp", rec, "luồng"));
        Thread.Sleep(Pause);
        rec = new Recorder();
        var rc = rec;
        lags.Add(VongLap.Run(() => Scenario("task", rc)));
        results.Add(("C. 1 luồng + Task.Run", rec, "luồng"));
        Thread.Sleep(Pause);
        rec = new Recorder();
        var rd = rec;
        lags.Add(VongLap.Run(() => Scenario("sem", rd)));
        results.Add(("D. như C, SemaphoreSlim(2)", rec, "luồng"));

        double scale = results.Max(r => r.Item2.Total());
        Console.WriteLine();
        foreach (var (name, r, kind) in results) Gantt(name, r, r.Total(), scale, kind);
        Axis(scale);
        Console.WriteLine();
        Console.WriteLine($"  {"Mô hình",-30} {"Thời gian",9} {"Trễ nhịp tim",13}");
        for (int i = 0; i < results.Count; i++)
            Console.WriteLine($"  {results[i].Item1,-30} {F(results[i].Item2.Total(), "F2"),7} s {F(lags[i] * 1000, "F0"),10} ms");
        Console.WriteLine();
        Console.WriteLine("Nhận xét: A – ứng dụng console không có SynchronizationContext, phần sau");
        Console.WriteLine("  await chạy trên ThreadPool nên CPU tự song song. B – vòng lặp một luồng");
        Console.WriteLine("  (như luồng giao diện WinForms/WPF): CPU chạy ngay trên vòng lặp chặn mọi");
        Console.WriteLine("  thứ, nhịp tim trễ hàng trăm ms. C, D – đẩy CPU sang ThreadPool bằng");
        Console.WriteLine("  Task.Run: vòng lặp thông suốt, CPU song song; D giới hạn số việc CPU.");
        Console.WriteLine("  Trễ vài chục ms còn lại ở A, C, D: bộ hẹn giờ của Task.Delay cần một");
        Console.WriteLine("  luồng ThreadPool rảnh để báo, mà các luồng đang bận tính toán.");
    }
}
