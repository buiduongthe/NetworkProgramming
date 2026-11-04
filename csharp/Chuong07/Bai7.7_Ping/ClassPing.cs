using System.Net.NetworkInformation;
using System.Security.Cryptography;

namespace Bai7_7;

// Lớp Ping của .NET: gọi API ICMP của hệ điều hành, không phải tự tạo gói và tính tổng kiểm tra
static class ClassPing
{
    public static int Run(string host, int count, int timeoutMs)
    {
        Console.WriteLine($"[ping] PING {host} bằng lớp System.Net.NetworkInformation.Ping, dữ liệu 32 byte");
        using var pinger = new Ping();
        byte[] data = RandomNumberGenerator.GetBytes(32);
        int ok = 0; long total = 0;
        for (int seq = 1; seq <= count; seq++)
        {
            PingReply reply = pinger.Send(host, timeoutMs, data);
            if (reply.Status == IPStatus.Success)
            {
                ok++; total += reply.RoundtripTime;
                string ttl = reply.Options != null ? reply.Options.Ttl.ToString() : "?";
                Console.WriteLine($"[ping] Trả lời từ {reply.Address}: seq={seq} ttl={ttl} thời gian={reply.RoundtripTime} ms");
            }
            else Console.WriteLine($"[ping] seq={seq}: {reply.Status}");
            if (seq < count) Thread.Sleep(1000);
        }
        string avg = ok > 0 ? $"{(double)total / ok:F3} ms" : "không có";
        Console.WriteLine($"[ping] Đã gửi {count}, nhận {ok}, mất {100 * (count - ok) / count}%; RTT trung bình {avg}");
        return 0;
    }
}
