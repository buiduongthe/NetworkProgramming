using System.Net;
using System.Net.Sockets;

namespace Bai2_2;

// Lỗi mạng đã phân loại: Kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}; Retry cho biết có nên thử lại
class NetError(string kind, bool retry, Exception? cause) : Exception(kind, cause)
{
    public string Kind { get; } = kind;
    public bool Retry { get; } = retry;
}

static class Net
{
    // Mã giả 2.2 – SAFE_CONNECT(host, port, timeout_ms)
    public static Socket SafeConnect(string host, int port, int timeoutMs)
    {
        IPAddress[] addrs;
        try
        {
            addrs = Dns.GetHostAddresses(host);                           // dòng 2–6: RESOLVE
        }
        catch (SocketException e)                                         // tên miền không tồn tại
        {
            throw new NetError("NOT_FOUND", retry: false, cause: e);
        }
        int refused = 0; Exception? lastError = null;                     // dòng 7
        foreach (IPAddress a in addrs)                                    // dòng 8
        {
            var s = new Socket(a.AddressFamily, SocketType.Stream, ProtocolType.Tcp); // dòng 9
            bool ok = false;
            // dòng 10: .NET không có tùy chọn CONNECT_TIMEOUT; dùng ConnectAsync với CancellationToken
            using var cts = new CancellationTokenSource(timeoutMs);
            try
            {
                s.ConnectAsync(a, port, cts.Token).AsTask().GetAwaiter().GetResult(); // dòng 11: CONNECT
                ok = true;
                return s;                                                 // thành công: trả socket
            }
            catch (SocketException e) when (e.SocketErrorCode == SocketError.ConnectionRefused)
            {
                refused++; lastError = e;                                 // dòng 14–15: nhận gói RST
            }
            catch (SocketException e) when (e.SocketErrorCode is SocketError.TimedOut
                       or SocketError.NetworkUnreachable or SocketError.HostUnreachable)
            {
                lastError = e;                                            // dòng 16–17: thử địa chỉ kế tiếp
            }
            catch (OperationCanceledException e)                          // hết timeoutMs → Timeout
            {
                lastError = new TimeoutException($"Hết thời gian chờ {timeoutMs} ms", e);
            }
            finally                                                       // dòng 18–20
            {
                if (!ok) s.Dispose();                                     // chỉ đóng khi thất bại
            }
        }
        if (refused > 0 && refused == addrs.Length)                       // dòng 22–24
            throw new NetError("REFUSED", retry: false, cause: lastError);
        throw new NetError("UNREACHABLE", retry: true, cause: lastError); // dòng 25
    }

    // Mô tả ngắn nguyên nhân: kiểu ngoại lệ + mã lỗi SocketError (nếu có) + thông điệp
    public static string Describe(Exception? e) => e switch
    {
        null => "(không rõ)",
        SocketException se => $"SocketException {se.SocketErrorCode}: {se.Message}",
        _ => $"{e.GetType().Name}: {e.Message}",
    };
}
