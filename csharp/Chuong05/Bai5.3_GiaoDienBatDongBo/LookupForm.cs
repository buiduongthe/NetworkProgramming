using System.Drawing;
using System.Net;
using System.Net.Sockets;
using System.Text.Json.Nodes;
using System.Windows.Forms;

namespace Bai5_3;

// Cửa sổ tra cứu WinForms.
// Nút Kết nối: Mã giả 5.3 – cách (1) luồng nền + UI_INVOKE (Control.BeginInvoke).
// Nút Tra cứu: Mã giả 5.4 – cách (2) trình xử lý sự kiện async; sau await, SynchronizationContext của
// WinForms đưa phần tiếp theo về luồng giao diện nên mã gán trực tiếp vào điều khiển.
class LookupForm : Form
{
    readonly TextBox txtHost = new() { Width = 130 };
    readonly TextBox txtPort = new() { Width = 60 };
    readonly Button btnConnect = new() { Text = "Kết nối", Width = 90 };
    readonly TextBox txtTerm = new() { Width = 300 };
    readonly Button btnLookup = new() { Text = "Tra cứu", Width = 90, Enabled = false };
    readonly TextBox txtResult = new() { Multiline = true, ReadOnly = true, Width = 560, Height = 54 };
    readonly Label lblStatus = new() { Text = "Chưa kết nối", Width = 300 };
    readonly Label lblClock = new() { Width = 240, ForeColor = Color.DimGray, TextAlign = ContentAlignment.MiddleRight };
    readonly System.Windows.Forms.Timer clock = new() { Interval = 100 };  // đồng hồ: chứng tỏ UI không đứng hình
    readonly CancellationTokenSource cts = new();                        // ct: hủy khi đóng cửa sổ (Mục 5.4)
    NetworkStream? conn;                                                 // conn: trường của cửa sổ
    int ticks;

    public LookupForm(string host, int port)
    {
        Text = "Tra cứu thuật ngữ – Bài 5.3";
        ClientSize = new Size(600, 190);
        txtHost.Text = host;
        txtPort.Text = port.ToString();
        var panel = new FlowLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(10) };
        panel.Controls.AddRange(new Control[] {
            new Label { Text = "Máy chủ:", Width = 80 }, txtHost, new Label { Text = "Cổng:", Width = 50 }, txtPort, btnConnect,
            new Label { Text = "Thuật ngữ:", Width = 80 }, txtTerm, btnLookup, txtResult, lblStatus, lblClock });
        panel.SetFlowBreak(btnConnect, true);
        panel.SetFlowBreak(btnLookup, true);
        panel.SetFlowBreak(txtResult, true);
        Controls.Add(panel);
        btnConnect.Click += (_, _) => OnConnectClick();
        btnLookup.Click += OnLookupClick;
        clock.Tick += (_, _) => lblClock.Text = $"Đồng hồ giao diện: {++ticks / 10.0:F1} s";
        clock.Start();
        FormClosing += (_, _) => { cts.Cancel(); conn?.Dispose(); };    // đóng cửa sổ: hủy tác vụ, đóng kết nối
    }

    void UiInvoke(Action proc) => BeginInvoke(proc);                     // UI_INVOKE: dạng không đồng bộ
    void ShowStatus(string text) => lblStatus.Text = text;

    // ---- Mã giả 5.3: cách (1) luồng nền và UI_INVOKE ----------------------------------------
    void OnConnectClick()                                                // trên luồng giao diện
    {
        btnConnect.Enabled = false;
        string host = txtHost.Text;
        int port = int.Parse(txtPort.Text);
        new Thread(() => ConnectWorker(host, port)) { IsBackground = true }.Start();  // SPAWN_THREAD
    }

    void ConnectWorker(string host, int port)                            // Mã giả 5.3, dòng 5–14: luồng nền
    {
        try
        {
            Socket c = SafeConnect(host, port, 3000);                    // dòng 7: chặn luồng nền
            UiInvoke(() => { conn = new NetworkStream(c, ownsSocket: true); btnLookup.Enabled = true; });  // dòng 8: SET_FIELD
            UiInvoke(() => ShowStatus("Đã kết nối"));                    // dòng 9
        }
        catch (NetError e)                                              // dòng 10–12
        {
            UiInvoke(() => ShowStatus("Lỗi: " + e.Code));
            UiInvoke(() => btnConnect.Enabled = true);                   // SET_ENABLED(btnConnect, TRUE)
        }
    }

    // ---- Mã giả 5.4: cách (2) trình xử lý sự kiện bất đồng bộ --------------------------------
    async void OnLookupClick(object? sender, EventArgs e)                // async void chỉ dùng cho trình xử lý sự kiện
    {
        btnLookup.Enabled = false;                                       // chống bấm lặp
        lblStatus.Text = "Đang tra cứu...";
        try
        {
            var req = new JsonObject { ["op"] = "lookup", ["term"] = txtTerm.Text };
            await Khung.SendJsonAsync(conn!, req, cts.Token);
            JsonNode resp = await Khung.RecvJsonAsync(conn!, cts.Token);    // UI không bị chặn
            txtResult.Text = (int?)resp["status"] == 200                     // dòng 8–12; đã ở luồng giao diện
                ? (string?)resp["text"] : $"({resp["status"]}) {resp["error"]}";
            lblStatus.Text = "Xong";
        }
        catch (OperationCanceledException)
        {
            return;                                                      // cửa sổ đã đóng
        }
        catch (Exception ex) when (ex is NetError || ex is ProtocolException || ex is ConnectionClosedException || ex is IOException)
        {
            lblStatus.Text = "Lỗi: " + ex.Message;
        }
        finally
        {
            if (!IsDisposed) btnLookup.Enabled = true;
        }
    }

    // Mã giả 2.2 – SAFE_CONNECT bản chặn (gọi trên luồng nền)
    static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs;
        try { addrs = Dns.GetHostAddresses(host); }
        catch (SocketException e) { throw new NetError("NOT_FOUND", false, e); }
        int refused = 0;
        Exception? last = null;
        foreach (var ip in addrs)
        {
            var s = new Socket(ip.AddressFamily, SocketType.Stream, ProtocolType.Tcp);
            using var timeout = new CancellationTokenSource(timeoutMs);   // CONNECT_TIMEOUT
            try
            {
                s.ConnectAsync(ip, port, timeout.Token).AsTask().GetAwaiter().GetResult();
                return s;
            }
            catch (SocketException ex) when (ex.SocketErrorCode == SocketError.ConnectionRefused) { refused++; last = ex; s.Dispose(); }
            catch (Exception ex) when (ex is SocketException || ex is OperationCanceledException) { last = ex; s.Dispose(); }
        }
        if (refused > 0 && refused == addrs.Length) throw new NetError("REFUSED", false, last);
        throw new NetError("UNREACHABLE", true, last);
    }
}
