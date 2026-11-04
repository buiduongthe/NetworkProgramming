using System.Net;
using System.Net.Sockets;

namespace Bai1_2;

// Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms): thử kết nối tới từng địa chỉ, kết luận OPEN/CLOSED/FILTERED/UNRESOLVED
static class PortChecker
{
    public static string PortCheck(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs = Resolver.ResolveHost(host);                  // dòng 2: lỗi phân giải → rỗng
        if (addrs.Length == 0) return "UNRESOLVED";                      // dòng 3
        string state = "FILTERED";                                       // dòng 4: chưa địa chỉ nào phản hồi
        foreach (var a in addrs)                                         // dòng 5: FOR EACH a IN addrs
        {
            var s = new Socket(a.AddressFamily, SocketType.Stream, ProtocolType.Tcp);   // dòng 6: SOCKET(FAMILY(a), STREAM)
            try
            {
                using var cts = new CancellationTokenSource(timeoutMs);  // dòng 7: CONNECT_TIMEOUT
                s.ConnectAsync(a, port, cts.Token).AsTask().GetAwaiter().GetResult();   // dòng 9: bắt tay ba bước
                Console.WriteLine($"[port-check] {a} cổng {port}: kết nối thành công");
                return "OPEN";                                           // dòng 10
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused)   // dòng 11
            {
                Console.WriteLine($"[port-check] {a} cổng {port}: bị từ chối (RST)");
                state = "CLOSED";                                        // dòng 12
            }
            catch (Exception e) when (e is OperationCanceledException
                                      || e is SocketException { SocketErrorCode: SocketError.TimedOut })   // dòng 13: Timeout
            {
                Console.WriteLine($"[port-check] {a} cổng {port}: hết thời gian chờ");
                continue;                                                // dòng 14
            }
            catch (SocketException e) when (e.SocketErrorCode is SocketError.NetworkUnreachable
                                                              or SocketError.HostUnreachable)   // dòng 13
            {
                Console.WriteLine($"[port-check] {a} cổng {port}: không có đường đi ({e.SocketErrorCode})");
                continue;
            }
            finally
            {
                s.Dispose();                                             // dòng 16: FINALLY CLOSE(s)
            }
        }
        return state;                                                    // dòng 19: CLOSED hoặc FILTERED
    }
}
