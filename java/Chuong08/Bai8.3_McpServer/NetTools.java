import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.net.ConnectException;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.UnknownHostException;
import java.nio.charset.StandardCharsets;

/**
 * Bài 8.3 – Thân ba công cụ mạng: Mã giả 1.1 RESOLVE_HOST, 1.2 PORT_CHECK, 7.8 PING (chép gọn).
 * PRINT đổi thành LOG ra stderr vì mọi dòng in ra stdout chen vào luồng thông điệp MCP (Mục 8.3.5).
 */
public class NetTools {
    /** Lỗi khi thực thi công cụ (máy không phản hồi...): trả về mô hình với isError = true. */
    public static class NetError extends Exception { public NetError(String m) { super(m); } }

    /** Đầu vào bị chính sách từ chối: cũng trả về với isError = true. */
    public static class PolicyError extends Exception { public PolicyError(String m) { super(m); } }

    /** Không phân giải được tên máy (VALIDATE_TARGET): trả về với isError = true. */
    public static class ResolveError extends Exception { public ResolveError(String m) { super(m); } }

    static final PrintStream ERR = new PrintStream(new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8);

    public static void log(String text) { ERR.println("[mcp] " + text); }     // nhật ký: chỉ ghi ra STDERR

    public static String familyName(InetAddress a) { return a instanceof Inet4Address ? "INET" : "INET6"; }

    /** Mã giả 1.1 – RESOLVE_HOST(name): lỗi phân giải -> danh sách rỗng. */
    public static InetAddress[] resolveHost(String name) {
        try {
            InetAddress[] addrs = InetAddress.getAllByName(name);             // RESOLVE
            for (InetAddress a : addrs) log("  resolve: " + familyName(a) + " " + a.getHostAddress());
            return addrs;
        } catch (UnknownHostException e) {
            log("  resolve: Không phân giải được " + name);
            return new InetAddress[0];
        }
    }

    /** Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms) -> OPEN | CLOSED | FILTERED | UNRESOLVED. */
    public static String portCheck(String host, int port, int timeoutMs) {
        InetAddress[] addrs = resolveHost(host);
        if (addrs.length == 0) return "UNRESOLVED";
        String state = "FILTERED";
        for (InetAddress a : addrs) {
            try (Socket s = new Socket()) {
                s.connect(new InetSocketAddress(a, port), timeoutMs);        // CONNECT có thời gian chờ
                return "OPEN";
            } catch (ConnectException e) {
                state = "CLOSED";                                            // nhận gói RST
            } catch (IOException e) {
                // Timeout, NetworkUnreachable, HostUnreachable: thử địa chỉ tiếp theo
            }
        }
        return state;
    }

    /**
     * Mã giả 7.8 – PING. JDK không có socket RAW nên dùng InetAddress.isReachable: với quyền quản trị
     * JDK gửi ICMP Echo Request, nếu không thì thử kết nối TCP tới cổng 7 (echo). Không đọc được TTL.
     */
    public static String ping(InetAddress dst, int seq, int timeoutMs) throws NetError, java.util.concurrent.TimeoutException {
        long t0 = System.nanoTime();
        boolean ok;
        try {
            ok = dst.isReachable(timeoutMs);
        } catch (IOException e) {
            throw new NetError("Lỗi khi gửi ICMP: " + e.getMessage());
        }
        if (!ok) throw new java.util.concurrent.TimeoutException(                       // Timeout
                "Không nhận được Echo Reply trong " + timeoutMs + " ms");
        double rtt = (System.nanoTime() - t0) / 1e6;
        return String.format(java.util.Locale.ROOT, "%s: thời gian khứ hồi %.3f ms (isReachable, không có TTL)",
                dst.getHostAddress(), rtt);
    }
}
