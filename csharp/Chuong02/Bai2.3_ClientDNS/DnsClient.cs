using System.Diagnostics;
using System.Globalization;
using System.Net;
using System.Net.Sockets;

namespace Bai2_3;

// Mã giả 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành; kế thừa Mã giả 1.1 RESOLVE_HOST
static class DnsClient
{
    public static int Run(string[] args)
    {
        if (args.Length == 0)                                             // dòng 1–5
        {
            Console.WriteLine("Cách dùng: dotnet run -- <tên miền | địa chỉ IP> ...");
            return 0;
        }
        foreach (string q in args)                                        // dòng 6
        {
            var t0 = Stopwatch.StartNew();                                // dòng 7: t0 ← NOW()
            if (IPAddress.TryParse(q, out IPAddress? ip))                 // dòng 8: IS_IP_ADDRESS
            {
                Console.WriteLine($"[dns] Tra cứu ngược: {q}");
                try
                {
                    string name = Dns.GetHostEntry(ip).HostName;          // REVERSE_RESOLVE: bản ghi PTR
                    Console.WriteLine($"  {q} → {name}");
                }
                catch (SocketException)                                   // ResolveError
                {
                    Console.WriteLine($"  {q} → không có bản ghi PTR");
                }
            }
            else
            {
                Console.WriteLine($"[dns] Tra cứu xuôi: {q}");
                ResolveHost(q);                                           // = Mã giả 1.1
            }
            string ms = t0.Elapsed.TotalMilliseconds.ToString("F2", CultureInfo.InvariantCulture);
            Console.WriteLine($"  Thời gian: {ms} ms");                   // dòng 18
        }
        return 0;
    }

    // Mã giả 1.1 – RESOLVE_HOST: in và trả về danh sách địa chỉ; lỗi → danh sách rỗng
    static IPAddress[] ResolveHost(string name)
    {
        try
        {
            IPAddress[] addrs = Dns.GetHostAddresses(name);               // RESOLVE (getaddrinfo của HĐH)
            foreach (IPAddress a in addrs)
            {
                string family = a.AddressFamily == AddressFamily.InterNetworkV6 ? "INET6" : "INET";
                Console.WriteLine($"  {family}  {a}");                    // FAMILY(a), ADDRESS(a)
            }
            return addrs;
        }
        catch (SocketException e)                                         // ResolveError
        {
            Console.WriteLine($"  Không phân giải được {name} (mã {e.SocketErrorCode}: {e.Message})");
            return [];
        }
    }
}
