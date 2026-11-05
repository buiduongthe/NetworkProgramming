using System.Net;
using System.Net.Sockets;
using System.Text;

namespace BaiMR3_2;

// Thiết bị mạng mô phỏng có giao diện dòng lệnh (CLI) qua TCP.
// Mô phỏng một bộ định tuyến kiểu Telnet để luyện tự động hóa mà không cần thiết bị thật:
//   - Đăng nhập: "Username: " / "Password: " (mật khẩu lấy từ biến môi trường DEVICE_PASSWORD).
//   - Dấu nhắc theo chế độ: R1>  R1#  R1(config)#  R1(config-if)#
//   - Lệnh: enable, terminal length 0, show version, show ip interface brief,
//     show running-config [interface <tên>], configure terminal, interface <tên>,
//     description <chữ>, ip address <ip> <mặt_nạ>, [no] shutdown, exit, end, write memory.
//   - Mỗi dòng lệnh được "vọng lại" (echo) như terminal thật; dòng kết thúc bằng CRLF.
//   - Khi chưa tắt phân trang, "show running-config" dừng sau 8 dòng với " --More-- ".
// Trạng thái cấu hình giữ trong bộ nhớ, dùng chung cho mọi phiên (có khóa).
// CẢNH BÁO: giống Telnet, mật khẩu đi dạng rõ; thiết bị thật phải dùng SSH.
static class ThietBiGia
{
    public const string Hostname = "R1";
    public static readonly string User = Environment.GetEnvironmentVariable("DEVICE_USER") ?? "admin";
    public static readonly string Password = Environment.GetEnvironmentVariable("DEVICE_PASSWORD") ?? "lab123";
    public const int Page = 8;                       // số dòng mỗi trang khi bật phân trang

    public class Iface { public string Desc = "", Ip = "", Mask = ""; public bool Shut; }
    public static readonly object StateLock = new();
    public static readonly List<(string Name, Iface I)> Interfaces =     // trạng thái ban đầu (giữ thứ tự)
    [
        ("Gi0/0", new Iface { Desc = "Lien ket R2", Ip = "10.20.1.225", Mask = "255.255.255.252", Shut = false }),
        ("Gi0/1", new Iface { Shut = true }),
        ("Gi0/2", new Iface { Shut = true }),
    ];
    static int sessions;

    public static void Log(string text) => Console.WriteLine($"[thiet-bi] {text}");

    public static Iface? Find(string name) => Interfaces.FirstOrDefault(x => x.Name == name).I;

    /// <summary>Sinh cấu hình đang chạy từ trạng thái (toàn bộ hoặc một cổng).</summary>
    public static List<string> RunningConfig(string? only = null)
    {
        var lines = only != null ? new List<string>() : ["!", $"hostname {Hostname}", "!"];
        lock (StateLock)
        {
            foreach (var (name, i) in Interfaces)
            {
                if (only != null && name != only) continue;
                lines.Add($"interface {name}");
                if (i.Desc != "") lines.Add($" description {i.Desc}");
                lines.Add(i.Ip != "" ? $" ip address {i.Ip} {i.Mask}" : " no ip address");
                if (i.Shut) lines.Add(" shutdown");
                lines.Add("!");
            }
        }
        if (only == null) lines.Add("end");
        return lines;
    }

    public static List<string> IpBrief()
    {
        var lines = new List<string> { $"{"Interface",-10}{"IP-Address",-14}{"Status",-22}Protocol" };
        lock (StateLock)
        {
            foreach (var (name, i) in Interfaces)
            {
                string status = i.Shut ? "administratively down" : "up";
                string ip = i.Ip != "" ? i.Ip : "unassigned";
                lines.Add($"{name,-10}{ip,-14}{status,-22}{(i.Shut ? "down" : "up")}");
            }
        }
        return lines;
    }

    /// <summary>Kiểm tra địa chỉ IPv4 và mặt nạ dạng chấm (các bit 1 phải liền nhau).</summary>
    public static bool ValidIpMask(string ip, string mask)
    {
        static bool V4(string s, out uint v)
        {
            v = 0;
            var p = s.Split('.');
            if (p.Length != 4) return false;
            foreach (var x in p)
            {
                if (x.Length == 0 || x.Length > 3 || !x.All(char.IsAsciiDigit) || int.Parse(x) > 255) return false;
                v = (v << 8) | uint.Parse(x);
            }
            return true;
        }
        if (!V4(ip, out _) || !V4(mask, out uint m)) return false;
        uint inv = ~m;
        return (inv & (inv + 1)) == 0;                  // ~mặt_nạ có dạng 0…01…1
    }

    public static int Run(string[] a)
    {
        string host = a.Length > 0 ? a[0] : "127.0.0.1";
        int port = a.Length > 1 ? int.Parse(a[1]) : 9000;
        var srv = new TcpListener(IPAddress.Parse(host), port);
        srv.Start();
        Log($"{Hostname} lắng nghe CLI tại {host}:{port}");
        Console.CancelKeyPress += (_, e) => { e.Cancel = true; srv.Stop(); };
        try
        {
            while (true)
            {
                Socket conn = srv.AcceptSocket();
                new Thread(() => Handle(conn)) { IsBackground = true }.Start();
            }
        }
        catch (SocketException) { }
        catch (ObjectDisposedException) { }
        Log("Dừng thiết bị");
        return 0;
    }

    static void Handle(Socket conn)
    {
        int sid = Interlocked.Increment(ref sessions);
        var addr = (IPEndPoint)conn.RemoteEndPoint!;
        try
        {
            new Session(conn, sid).Run();
        }
        catch (Exception e) when (e is EndOfStreamException || e is IOException || e is SocketException) { }
        finally
        {
            conn.Close();
            Log($"phiên #{sid} ({addr.Address}:{addr.Port}) kết thúc");
        }
    }
}

/// <summary>Một phiên CLI: đăng nhập, dấu nhắc theo chế độ, thực thi lệnh.</summary>
class Session(Socket conn, int sid)
{
    readonly BufferedStream rf = new(new NetworkStream(conn), 4096);   // đọc theo dòng và theo từng byte
    string mode = "user";
    string? iface;
    bool paging = true;

    void Send(string text)
    {
        var b = Encoding.UTF8.GetBytes(text);
        int sent = 0;
        while (sent < b.Length) sent += conn.Send(b, sent, b.Length - sent, SocketFlags.None);
    }

    /// <summary>Đọc một dòng (tới '\n'), bỏ CR/LF ở cuối; EOF → EndOfStreamException.</summary>
    string ReadLine()
    {
        var line = new List<byte>();
        while (true)
        {
            int b = rf.ReadByte();
            if (b < 0)
            {
                if (line.Count == 0) throw new EndOfStreamException();
                break;
            }
            line.Add((byte)b);
            if (b == '\n') break;
        }
        return Encoding.UTF8.GetString(line.ToArray()).TrimEnd('\r', '\n');
    }

    void Prompt()
    {
        string suffix = mode switch
        {
            "user" => ">", "enable" => "#", "config" => "(config)#", _ => "(config-if)#",
        };
        Send($"{ThietBiGia.Hostname}{suffix}");
    }

    /// <summary>In nhiều dòng; nếu đang phân trang thì dừng mỗi Page dòng chờ một phím bất kỳ.</summary>
    void Show(List<string> lines)
    {
        for (int k = 1; k <= lines.Count; k++)
        {
            Send(lines[k - 1] + "\r\n");
            if (paging && k % ThietBiGia.Page == 0 && k < lines.Count)
            {
                Send(" --More-- ");
                if (rf.ReadByte() < 0) throw new EndOfStreamException();
                Send("\r          \r");                 // xóa chữ --More-- như terminal thật
            }
        }
    }

    bool Login()
    {
        Send($"\r\n{ThietBiGia.Hostname} - thiết bị mô phỏng cho bài thực hành\r\n\r\nUsername: ");
        string user = ReadLine();
        Send("Password: ");
        string pw = ReadLine();                          // không vọng lại mật khẩu
        Send("\r\n");
        if (user != ThietBiGia.User || pw != ThietBiGia.Password)
        {
            ThietBiGia.Log($"phiên #{sid}: đăng nhập SAI (user='{user}')");
            Send("% Authentication failed\r\n");
            return false;
        }
        ThietBiGia.Log($"phiên #{sid}: đăng nhập {user}");
        return true;
    }

    public void Run()
    {
        if (!Login()) return;
        while (true)
        {
            Prompt();
            string cmd = ReadLine().Trim();
            Send(cmd + "\r\n");                          // vọng lại dòng lệnh
            if (cmd == "") continue;
            if (!Execute(cmd)) break;
        }
    }

    void Invalid() => Send("% Invalid input detected at '^' marker.\r\n");

    static List<string> Version() =>
        ["Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0", $"{ThietBiGia.Interfaces.Count} cổng GigabitEthernet"];

    bool Execute(string cmd)
    {
        var words = cmd.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        var m = mode;
        if (cmd == "exit" && (m == "user" || m == "enable")) return false;
        if (m == "user")
        {
            if (cmd == "enable") mode = "enable";
            else if (cmd == "show version") Show(Version());
            else Invalid();
        }
        else if (m == "enable")
        {
            if (cmd == "terminal length 0") paging = false;
            else if (cmd == "show version") Show(Version());
            else if (cmd == "show ip interface brief") Show(ThietBiGia.IpBrief());
            else if (cmd == "show running-config") Show(ThietBiGia.RunningConfig());
            else if (words.Length == 4 && words[0] == "show" && words[1] == "running-config" && words[2] == "interface")
            {
                if (ThietBiGia.Find(words[3]) != null) Show(ThietBiGia.RunningConfig(words[3]));
                else Invalid();
            }
            else if (cmd == "configure terminal" || cmd == "conf t")
            {
                Send("Enter configuration commands, one per line.  End with CNTL/Z.\r\n");
                mode = "config";
            }
            else if (cmd == "write memory" || cmd == "wr")
            {
                Send("Building configuration...\r\n[OK]\r\n");
                ThietBiGia.Log($"phiên #{sid}: lưu cấu hình (write memory)");
            }
            else Invalid();
        }
        else if (cmd == "end")
        {
            mode = "enable"; iface = null;
        }
        else if (m == "config")
        {
            if (words[0] == "interface" && words.Length == 2 && ThietBiGia.Find(words[1]) != null)
            {
                mode = "config-if"; iface = words[1];
            }
            else if (cmd == "exit") mode = "enable";
            else Invalid();
        }
        else if (m == "config-if") ConfigIf(cmd, words);
        return true;
    }

    void ConfigIf(string cmd, string[] words)
    {
        if (cmd == "exit")
        {
            mode = "config"; iface = null;
            return;
        }
        lock (ThietBiGia.StateLock)
        {
            var i = ThietBiGia.Find(iface!)!;
            if (words[0] == "description" && words.Length > 1) i.Desc = cmd["description ".Length..];
            else if (cmd == "no description") i.Desc = "";
            else if (words.Length == 4 && words[0] == "ip" && words[1] == "address")
            {
                if (!ThietBiGia.ValidIpMask(words[2], words[3]))
                {
                    Send("% Invalid address\r\n");
                    return;
                }
                i.Ip = words[2]; i.Mask = words[3];
            }
            else if (cmd == "shutdown") i.Shut = true;
            else if (cmd == "no shutdown") i.Shut = false;
            else
            {
                Invalid();
                return;
            }
        }
        ThietBiGia.Log($"phiên #{sid}: {iface}: {cmd}");
    }
}
