using System.Net.Sockets;
using System.Numerics;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace BaiMR3_2;

// Tự động hóa cấu hình thiết bị qua CLI (kiểu expect/Netmiko, chỉ dùng socket).
// Dữ liệu CLI là luồng byte KHÔNG có khung thông điệp, nên client phải "đọc cho tới khi gặp dấu nhắc"
// (READ_UNTIL) – một dạng định khung bằng dấu phân cách, giống READ_LINE ở Bài 3.2 nhưng dấu phân cách
// là biểu thức chính quy.
// Quy trình:
//   1. Đăng nhập, vào chế độ đặc quyền (enable), tắt phân trang (terminal length 0).
//   2. Với mỗi cổng trong tệp trạng thái mong muốn: đọc cấu hình hiện tại, phân tích,
//      so sánh và CHỈ sinh lệnh cho phần khác biệt (lũy đẳng – idempotent).
//   3. Áp dụng lệnh (hoặc chỉ in ra với --thu), đọc lại để kiểm chứng, lưu cấu hình.
// Tài khoản lấy từ biến môi trường DEVICE_USER, DEVICE_PASSWORD (không ghi cứng trong mã).
class CliError(string msg) : Exception(msg);

class CliSession : IDisposable
{
    public static readonly Regex Prompt = new(@"[\w.-]+(\([\w-]+\))?[>#]$");   // R1>  R1#  R1(config)#  R1(config-if)#
    readonly Socket sock;
    readonly TimeSpan timeout;
    readonly Decoder dec = Encoding.UTF8.GetDecoder();   // giải mã an toàn khi ký tự UTF-8 bị chia giữa hai gói
    string buf = "";

    public CliSession(string host, int port, double timeoutSec = 5.0)
    {
        timeout = TimeSpan.FromSeconds(timeoutSec);
        sock = new Socket(SocketType.Stream, ProtocolType.Tcp);
        using var cts = new CancellationTokenSource(timeout);
        sock.ConnectAsync(host, port, cts.Token).AsTask().GetAwaiter().GetResult();
    }

    /// <summary>READ_UNTIL: đọc dồn vào bộ đệm cho tới khi phần cuối khớp mẫu; trả về phần đã đọc.</summary>
    public string ReadUntil(Regex pattern)
    {
        var deadline = DateTime.UtcNow + timeout;
        var bytes = new byte[4096];
        var chars = new char[4096];
        while (true)
        {
            string text = buf.TrimEnd(' ');
            if (pattern.IsMatch(text))
            {
                buf = "";
                return text;
            }
            if (buf.Contains("% Authentication failed")) throw new CliError("Đăng nhập thất bại");
            var remaining = deadline - DateTime.UtcNow;
            if (remaining <= TimeSpan.Zero)
                throw new CliError($"Hết thời gian chờ dấu nhắc, đã nhận: '{buf[Math.Max(0, buf.Length - 60)..]}'");
            sock.ReceiveTimeout = Math.Max(1, (int)remaining.TotalMilliseconds);
            int n = sock.Receive(bytes);
            if (n == 0) throw new CliError("Thiết bị đóng kết nối");
            int c = dec.GetChars(bytes, 0, n, chars, 0);
            buf += new string(chars, 0, c);
        }
    }

    public void SendLine(string line)
    {
        var b = Encoding.UTF8.GetBytes(line + "\n");
        int sent = 0;
        while (sent < b.Length) sent += sock.Send(b, sent, b.Length - sent, SocketFlags.None);
    }

    public void Login(string user, string password)
    {
        ReadUntil(new Regex("Username:$"));
        SendLine(user);
        ReadUntil(new Regex("Password:$"));
        SendLine(password);
        ReadUntil(Prompt);
    }

    /// <summary>Gửi một lệnh, chờ dấu nhắc kế tiếp, trả về phần kết quả (bỏ dòng vọng lại và dấu nhắc).</summary>
    public List<string> Command(string cmd)
    {
        SendLine(cmd);
        var output = ReadUntil(Prompt).Replace("\r\n", "\n").Split('\n').ToList();
        if (output.Count > 0 && output[0].Trim() == cmd) output.RemoveAt(0);   // bỏ dòng vọng lại
        var body = output.Take(Math.Max(0, output.Count - 1)).ToList();          // bỏ dòng dấu nhắc
        if (body.Any(l => l.StartsWith('%')))
            throw new CliError($"Thiết bị từ chối lệnh '{cmd}': {body[0]}");
        return body;
    }

    public void Dispose()
    {
        try { SendLine("exit"); } catch (SocketException) { }
        sock.Close();
    }
}

/// <summary>Cấu hình của một cổng, cùng dạng với tệp mong muốn (ip ở dạng "địa_chỉ/tiền_tố").</summary>
record IfaceState(string MoTa, string Ip, bool Bat);

static class TuDongHoa
{
    static IfaceState ParseInterface(List<string> lines)
    {
        string moTa = "", ip = "";
        bool bat = true;
        foreach (var raw in lines)
        {
            string line = raw.Trim();
            if (line.StartsWith("description ")) moTa = line["description ".Length..];
            else if (line.StartsWith("ip address "))
            {
                var w = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
                ip = $"{w[2]}/{MaskToPrefix(w[3])}";
            }
            else if (line == "shutdown") bat = false;
        }
        return new IfaceState(moTa, ip, bat);
    }

    static uint ToU32(string dotted) => dotted.Split('.').Aggregate(0u, (v, p) => (v << 8) | uint.Parse(p));
    static int MaskToPrefix(string mask) => BitOperations.PopCount(ToU32(mask));
    static string PrefixToMask(int len)
    {
        uint m = len == 0 ? 0 : uint.MaxValue << (32 - len);
        return $"{m >> 24}.{(m >> 16) & 255}.{(m >> 8) & 255}.{m & 255}";
    }

    /// <summary>Chỉ sinh lệnh cho phần khác biệt giữa cấu hình hiện tại và mong muốn.</summary>
    static List<string> DiffCommands(IfaceState cur, IfaceState want)
    {
        var cmds = new List<string>();
        if (cur.MoTa != want.MoTa) cmds.Add(want.MoTa != "" ? $"description {want.MoTa}" : "no description");
        if (cur.Ip != want.Ip)
        {
            var p = want.Ip.Split('/');
            cmds.Add($"ip address {p[0]} {PrefixToMask(int.Parse(p[1]))}");
        }
        if (cur.Bat != want.Bat) cmds.Add(want.Bat ? "no shutdown" : "shutdown");
        return cmds;
    }

    public static int Run(string[] a)
    {
        if (a.Length < 3)
        {
            Console.WriteLine("Cách dùng: dotnet run -- tu_dong <host> <port> <muc_tieu.json> [--thu]");
            return 2;
        }
        string host = a[0], path = a[2];
        int port = int.Parse(a[1]);
        bool dryRun = a.Skip(3).Contains("--thu");
        using var doc = JsonDocument.Parse(File.ReadAllText(path, Encoding.UTF8));
        string user = Environment.GetEnvironmentVariable("DEVICE_USER") ?? "admin";
        string password = Environment.GetEnvironmentVariable("DEVICE_PASSWORD") ?? "";
        Console.WriteLine($"[tu-dong] Kết nối {host}:{port}, tài khoản {user}");
        if (dryRun) Console.WriteLine("[tu-dong] Chế độ chạy thử: chỉ in lệnh, không áp dụng");
        try
        {
            using var cli = new CliSession(host, port);
            cli.Login(user, password);
            cli.Command("enable");
            cli.Command("terminal length 0");                 // tắt --More-- để đọc trọn kết quả
            Console.WriteLine($"[tu-dong] {cli.Command("show version")[0]}");
            int total = 0;
            foreach (var item in doc.RootElement.GetProperty("cong").EnumerateArray())
            {
                string name = item.GetProperty("ten").GetString()!;
                var want = new IfaceState(item.GetProperty("mo_ta").GetString()!, item.GetProperty("ip").GetString()!,
                                          item.GetProperty("bat").GetBoolean());
                var cur = ParseInterface(cli.Command($"show running-config interface {name}"));
                var cmds = DiffCommands(cur, want);
                if (cmds.Count == 0)
                {
                    Console.WriteLine($"[tu-dong] {name}: đã đúng, không cần thay đổi");
                    continue;
                }
                Console.WriteLine($"[tu-dong] {name}: cần {cmds.Count} lệnh");
                foreach (var c in cmds) Console.WriteLine($"           + {c}");
                if (dryRun) continue;
                cli.Command("configure terminal");
                cli.Command($"interface {name}");
                foreach (var c in cmds) cli.Command(c);
                cli.Command("end");
                var after = ParseInterface(cli.Command($"show running-config interface {name}"));
                if (after != want)                            // đọc lại để kiểm chứng (record so sánh theo giá trị)
                    throw new CliError($"{name}: cấu hình sau khi áp dụng chưa khớp: {after}");
                total += cmds.Count;
            }
            if (total > 0)
            {
                cli.Command("write memory");
                Console.WriteLine($"[tu-dong] Đã áp dụng và kiểm chứng {total} lệnh, đã lưu cấu hình");
            }
            else if (!dryRun) Console.WriteLine("[tu-dong] Thiết bị đã ở đúng trạng thái mong muốn (lũy đẳng)");
            Console.WriteLine("[tu-dong] show ip interface brief:");
            foreach (var line in cli.Command("show ip interface brief")) Console.WriteLine("  " + line);
        }
        catch (Exception e) when (e is CliError || e is SocketException || e is IOException || e is OperationCanceledException)
        {
            Console.WriteLine($"[tu-dong] LỖI: {e.Message}");
            return 1;
        }
        return 0;
    }
}
