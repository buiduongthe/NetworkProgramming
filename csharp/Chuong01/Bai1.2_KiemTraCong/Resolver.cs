using System.Net;
using System.Net.Sockets;

namespace Bai1_2;

// Mã giả 1.1 – RESOLVE_HOST (chép từ Bài 1.1): in từng địa chỉ kèm họ địa chỉ; lỗi phân giải → danh sách rỗng
static class Resolver
{
    // FAMILY(a): INET cho IPv4, INET6 cho IPv6
    static string Family(IPAddress a) => a.AddressFamily == AddressFamily.InterNetworkV6 ? "INET6" : "INET";

    public static IPAddress[] ResolveHost(string name)
    {
        try
        {
            IPAddress[] addrs = Dns.GetHostAddresses(name);              // RESOLVE(name)
            foreach (var a in addrs)
                Console.WriteLine($"  {Family(a),-6} {a}");              // PRINT(FAMILY(a), ADDRESS(a))
            return addrs;
        }
        catch (SocketException e)                                        // CATCH ResolveError AS e
        {
            Console.WriteLine($"[port-check] Không phân giải được {name} (mã lỗi {(int)e.SocketErrorCode}: {e.SocketErrorCode})");
            return [];                                                   // RETURN EMPTY_LIST
        }
    }
}
