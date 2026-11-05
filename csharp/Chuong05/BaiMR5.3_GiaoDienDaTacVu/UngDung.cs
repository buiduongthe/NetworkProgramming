using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Globalization;
using System.Windows.Forms;

namespace BaiMR5_3;

// Cửa sổ WinForms: giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song.
// Quy tắc vàng của lập trình giao diện: CHỈ luồng giao diện được chạm vào điều khiển, và luồng giao diện
// KHÔNG BAO GIỜ được chờ mạng. Bản C# dùng cách được khuyến nghị trên .NET (Mục 5.3.2, cách 2):
//   - phần điều phối mỗi tác vụ (RunTask: xin lượt, cập nhật hàng, báo kết quả) là hàm async trên luồng
//     giao diện; SynchronizationContext của WinForms đưa phần tiếp theo về luồng giao diện nên mã gán trực
//     tiếp vào điều khiển (không cần hàng đợi như bản Python);
//   - phần mạng chạy bằng await Task.Run(...) trên thread pool: đoạn ĐỒNG BỘ đầu tiên của một hàm async
//     chạy trên luồng gọi nó, và lần gọi HttpClient đầu tiên (khởi tạo + biên dịch JIT) mất hàng trăm ms –
//     để trên luồng giao diện thì cửa sổ khựng ngay lần bấm đầu (đo được 733 ms trước khi sửa);
//   - tiến trình báo qua Progress<T>, hủy qua CancellationTokenSource của từng tác vụ;
//   - bộ giới hạn đồng thời (Limiter) đổi được bằng thanh trượt khi đang chạy;
//   - đồng hồ và hoạt hình chạy bằng System.Windows.Forms.Timer 20 ms; khoảng cách thật giữa hai nhịp được
//     đo lại – nếu luồng giao diện bị chặn, khoảng cách này tăng vọt (giao diện "đứng").
// Nút "Làm SAI" chạy đúng việc tải tệp đó bằng lời gọi CHẶN ngay trên luồng giao diện để thấy giao diện đứng.

/// <summary>Một tác vụ đang hiển thị: các điều khiển trên một hàng và nguồn hủy của nó.</summary>
class TaskRow(int id, string name)
{
    public readonly int Id = id;
    public readonly string Name = name;
    public readonly CancellationTokenSource Cts = new();
    public bool Done;
    public readonly Label Lbl = new() { Text = $"#{id} {name}", Width = 170, AutoEllipsis = true };
    public readonly ProgressBar Bar = new() { Width = 300, Maximum = 100 };
    public readonly Label Status = new() { Text = "chờ lượt", Width = 260, AutoEllipsis = true };
    public readonly Button BtnCancel = new() { Text = "Hủy", Width = 60 };
    public Control[] Widgets => [Lbl, Bar, Status, BtnCancel];
}

/// <summary>Quả bóng chạy qua lại và cung tròn quay – đứng yên ngay khi luồng giao diện bị chặn.</summary>
class HoatHinh : Control
{
    public double T;

    public HoatHinh()
    {
        DoubleBuffered = true;                              // vẽ vào bộ đệm trước: không nhấp nháy
        Size = new Size(260, 36);
        BackColor = Color.FromArgb(0xF4, 0xF6, 0xFA);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        float x = (float)(110 + 100 * Math.Sin(T * 3));
        using var ball = new SolidBrush(Color.FromArgb(0x24, 0x57, 0xC5));
        g.FillEllipse(ball, x, 8, 20, 20);
        using var pen = new Pen(Color.FromArgb(0xE0, 0x7B, 0x00), 4);
        g.DrawArc(pen, 222, 2, 32, 32, (float)(T * 360 % 360), 270);
    }
}

class UngDung : Form
{
    const int TickMs = 20;
    static readonly (int Kb, int Delay)[] Files =
        [(256, 60), (256, 90), (384, 70), (256, 120), (320, 50), (256, 100), (192, 80), (256, 40)];
    static readonly string[] DnsNames = ["router.lab", "may-in.lab", "camera-1.lab", "nas.lab", "khong-co.lab"];

    readonly string host;
    readonly int port;
    readonly Limiter limiter = new(3);
    readonly List<TaskRow> tasks = [];
    int nextId = 1;
    readonly Stopwatch sw = Stopwatch.StartNew();
    double last;
    int ticks;
    readonly Dictionary<string, double[]> gaps = [];       // giai đoạn -> [số nhịp, tổng, lớn nhất] (ms)

    // ---- điều khiển ----
    readonly Label lblClock = new() { AutoSize = true, Font = new Font(FontFamily.GenericMonospace, 18, FontStyle.Bold) };
    readonly HoatHinh anim = new();
    readonly Label lblStats = new() { AutoSize = true, ForeColor = Color.FromArgb(0x33, 0x33, 0x33),
                                      Text = "Khoảng cách giữa hai nhịp đồng hồ (đặt 20 ms): đang đo…" };
    readonly NumericUpDown spinN = new() { Minimum = 1, Maximum = Files.Length, Value = 5, Width = 45 };
    readonly Button btnDl = new() { Text = "Tải tệp", AutoSize = true };
    readonly Button btnPorts = new() { Text = "Kiểm tra cổng", AutoSize = true };
    readonly Button btnDns = new() { Text = "Tra cứu tên", AutoSize = true };
    readonly TrackBar trackLimit = new() { Minimum = 1, Maximum = 8, Value = 3, Width = 120, TickFrequency = 1 };
    readonly Label lblLimit = new() { Text = "3", AutoSize = true };
    readonly Button btnCancelAll = new() { Text = "Hủy tất cả", AutoSize = true };
    readonly Button btnBad = new() { Text = "Làm SAI (tải trên luồng GUI)", AutoSize = true,
                                     BackColor = Color.FromArgb(0xF6, 0xD5, 0xD9) };
    readonly TableLayoutPanel tblTasks = new() { ColumnCount = 4, AutoSize = true, Dock = DockStyle.Top };
    readonly TextBox txtLog = new() { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Vertical,
                                      Dock = DockStyle.Fill, Font = new Font(FontFamily.GenericMonospace, 9) };
    readonly System.Windows.Forms.Timer timer = new() { Interval = TickMs };

    // ---- truy cập cho kịch bản minh họa tự động (DemoGui.cs) ----
    internal List<TaskRow> Tasks => tasks;
    internal NumericUpDown SpinN => spinN;
    internal TrackBar TrackLimit => trackLimit;
    internal Button BtnDl => btnDl;
    internal Button BtnPorts => btnPorts;
    internal Button BtnDns => btnDns;
    internal Button BtnCancelAll => btnCancelAll;
    internal Button BtnBad => btnBad;
    internal IReadOnlyDictionary<string, double[]> Gaps => gaps;
    internal bool BadPending;                              // nhịp kế tiếp tính vào "luồng GUI bị chặn"
    internal bool SkipGap;                                 // bỏ qua một nhịp (kịch bản đang chụp ảnh)

    public UngDung(string host, int port)
    {
        this.host = host;
        this.port = port;
        Text = "MR5.3 – Giao diện đa tác vụ (WinForms)";
        ClientSize = new Size(980, 680);
        Build();
        last = sw.Elapsed.TotalSeconds;
        timer.Tick += (_, _) => Tick();
        timer.Start();
        FormClosing += (_, _) =>                           // đóng cửa sổ: dừng đồng hồ, hủy mọi tác vụ
        {
            timer.Stop();
            foreach (var t in tasks) t.Cts.Cancel();
        };
    }

    // ---------- dựng giao diện ----------
    void Build()
    {
        var root = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 1, RowCount = 5, Padding = new Padding(6) };
        for (int i = 0; i < 4; i++) root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 100));

        var top = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill };
        anim.Margin = new Padding(12, 6, 3, 3);
        top.Controls.AddRange(new Control[] { lblClock, anim });

        var bar = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill };
        bar.Controls.AddRange(new Control[] { spinN, btnDl, btnPorts, btnDns,
            new Label { Text = "Giới hạn đồng thời", AutoSize = true, Margin = new Padding(16, 8, 3, 3) },
            trackLimit, lblLimit, btnCancelAll, btnBad });
        lblLimit.Margin = new Padding(0, 8, 8, 3);

        var grpTasks = new GroupBox { Text = "Tác vụ", AutoSize = true, Dock = DockStyle.Fill, MinimumSize = new Size(0, 60) };
        grpTasks.Controls.Add(tblTasks);
        var grpLog = new GroupBox { Text = "Nhật ký", Dock = DockStyle.Fill };
        grpLog.Controls.Add(txtLog);

        root.Controls.Add(top, 0, 0);
        root.Controls.Add(lblStats, 0, 1);
        root.Controls.Add(bar, 0, 2);
        root.Controls.Add(grpTasks, 0, 3);
        root.Controls.Add(grpLog, 0, 4);
        Controls.Add(root);

        btnDl.Click += (_, _) => StartDownloads();
        btnPorts.Click += (_, _) => StartPorts();
        btnDns.Click += (_, _) => StartDns();
        btnCancelAll.Click += (_, _) => CancelAll();
        btnBad.Click += (_, _) => Bad();
        trackLimit.ValueChanged += (_, _) =>
        {
            limiter.Set(trackLimit.Value);
            lblLimit.Text = trackLimit.Value.ToString();
        };
    }

    // ---------- nhịp đồng hồ: phải luôn chạy đều ----------
    void Tick()
    {
        double now = sw.Elapsed.TotalSeconds;
        double gap = (now - last) * 1000;
        last = now;
        if (SkipGap) SkipGap = false;
        else if (ticks > 0)
        {
            string phase;
            if (BadPending)
            {
                phase = "luồng GUI bị chặn";
                BadPending = false;
            }
            else phase = tasks.Any(t => !t.Done) ? "chạy song song" : "rỗi";
            if (!gaps.TryGetValue(phase, out var g)) gaps[phase] = g = [0, 0, 0];
            g[0]++; g[1] += gap; g[2] = Math.Max(g[2], gap);
        }
        ticks++;
        lblClock.Text = DateTime.Now.ToString("HH:mm:ss.fff", CultureInfo.InvariantCulture);
        anim.T = now;
        anim.Invalidate();                                  // vẽ lại hoạt hình
        lblStats.Text = StatsText();
    }

    string StatsText()
    {
        var parts = new List<string> { "Khoảng cách thật giữa hai nhịp đồng hồ (đặt 20 ms):" };
        foreach (var phase in new[] { "chạy song song", "luồng GUI bị chặn" })
            if (gaps.TryGetValue(phase, out var g))
                parts.Add($"  • {phase}: tối đa {g[2]:F0} ms, trung bình {g[1] / g[0]:F1} ms ({g[0]} nhịp)");
        if (parts.Count == 1) parts.Add("  đang đo…");
        return string.Join("\n", parts);
    }

    internal void Log(string text)
    {
        string line = $"{sw.Elapsed.TotalSeconds,6:F2} s  {text}";
        Console.WriteLine($"[gui] {line}");
        txtLog.AppendText(line + Environment.NewLine);
    }

    // ---------- tạo và chạy tác vụ ----------
    /// <summary>Thêm đúng một hàng cho tác vụ mới (không dựng lại cả bảng – dựng lại tốn hàng trăm ms).</summary>
    void AddRow(TaskRow t)
    {
        tblTasks.SuspendLayout();
        int r = tasks.Count - 1;
        tblTasks.RowCount = tasks.Count;
        for (int c = 0; c < 4; c++) tblTasks.Controls.Add(t.Widgets[c], c, r);
        tblTasks.ResumeLayout();
    }

    /// <summary>Dựng lại cả bảng (chỉ dùng sau khi xóa các tác vụ đã xong).</summary>
    void Relayout()
    {
        tblTasks.SuspendLayout();
        tblTasks.Controls.Clear();
        tblTasks.RowCount = tasks.Count;
        for (int r = 0; r < tasks.Count; r++)
            for (int c = 0; c < 4; c++) tblTasks.Controls.Add(tasks[r].Widgets[c], c, r);
        tblTasks.ResumeLayout();
    }

    void ClearFinished()
    {
        foreach (var t in tasks.Where(t => t.Done).ToList())
        {
            tasks.Remove(t);
            foreach (var w in t.Widgets) w.Dispose();
        }
        Relayout();
    }

    void Spawn(string name, Func<IProgress<(int, string)>, CancellationToken, Task<string>> job)
    {
        var t = new TaskRow(nextId++, name);
        t.BtnCancel.Click += (_, _) => t.Cts.Cancel();
        tasks.Add(t);
        AddRow(t);
        _ = RunTask(t, job);                                // không chờ: tác vụ chạy song song với giao diện
    }

    /// <summary>Chạy trên luồng giao diện nhưng không chặn nó: xin lượt, làm việc, báo kết quả.</summary>
    async Task RunTask(TaskRow t, Func<IProgress<(int, string)>, CancellationToken, Task<string>> job)
    {
        var progress = new Progress<(int Pct, string Text)>(p =>
        {
            if (t.Done) return;                             // báo muộn sau khi tác vụ đã xong: bỏ qua
            t.Bar.Value = Math.Clamp(p.Pct, 0, 100);
            t.Status.Text = p.Text;
        });
        if (!await limiter.AcquireAsync(t.Cts.Token))
        {
            Finish(t, "đã hủy (khi đang chờ lượt)");
            return;
        }
        try
        {
            t.Status.Text = "đang chạy…";
            // Task.Run: toàn bộ phần mạng (kể cả đoạn đồng bộ trước await đầu tiên) chạy trên thread pool;
            // tiến trình vẫn về luồng giao diện qua Progress<T>, kết quả về sau await như cũ.
            Finish(t, await Task.Run(() => job(progress, t.Cts.Token)));
        }
        catch (OperationCanceledException)
        {
            Finish(t, "đã hủy");
        }
        catch (Exception e)
        {
            Finish(t, $"lỗi: {e.Message}");
        }
        finally
        {
            limiter.Release();
        }
    }

    void Finish(TaskRow t, string text)
    {
        t.Done = true;
        t.Status.Text = text;
        t.BtnCancel.Enabled = false;
        Log($"{t.Name}: {text}");
    }

    void StartDownloads()
    {
        ClearFinished();
        int n = (int)spinN.Value;
        Log($"Bấm Tải tệp: {n} tệp, giới hạn đồng thời {trackLimit.Value}");
        foreach (var (kb, delay) in Files.Take(n))
            Spawn($"Tải {kb} KB ({delay} ms/khối)", (p, ct) => TacVu.DownloadAsync(host, port, kb, delay, p, ct));
    }

    void StartPorts()
    {
        ClearFinished();
        int[] ports = [port, port + 1, port + 2, port + 3];
        Log($"Bấm Kiểm tra cổng: {string.Join(", ", ports)}");
        Spawn("Kiểm tra 4 cổng", (p, ct) => TacVu.CheckPortsAsync(host, ports, p, ct));
    }

    void StartDns()
    {
        ClearFinished();
        Log($"Bấm Tra cứu tên: {DnsNames.Length} tên qua UDP");
        Spawn($"Tra cứu {DnsNames.Length} tên", (p, ct) => TacVu.LookupNamesAsync(host, port, DnsNames, p, ct));
    }

    void CancelAll()
    {
        var running = tasks.Where(t => !t.Done).ToList();
        Log($"Bấm Hủy tất cả: {running.Count} tác vụ");
        foreach (var t in running) t.Cts.Cancel();
    }

    /// <summary>CÁCH SAI: chờ mạng ngay trên luồng giao diện – cả cửa sổ đứng cho tới khi xong.</summary>
    void Bad()
    {
        Log("Bấm Làm SAI: tải 256 KB ngay trên luồng giao diện…");
        double t = sw.Elapsed.TotalSeconds;
        try
        {
            TacVu.DownloadBlocking(host, port, 256, 80);
        }
        catch (Exception e)
        {
            Log($"lỗi: {e.Message}");
        }
        double dt = (sw.Elapsed.TotalSeconds - t) * 1000;
        BadPending = true;
        Log($"Luồng giao diện bị chặn {dt:F0} ms: đồng hồ, nút, thanh tiến trình đứng yên");
    }
}
