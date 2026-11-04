import java.io.IOException;
import java.net.ConnectException;
import java.net.Inet6Address;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.NoRouteToHostException;
import java.net.Socket;
import java.net.SocketException;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.util.List;

/**
 * Bài 1.2 – Kiểm tra trạng thái một cổng TCP (Mã giả 1.2 – PORT_CHECK, Mục 1.2.5 Giáo trình Lập trình mạng),
 * kế thừa Mã giả 1.1 – RESOLVE_HOST (Mục 1.2.3). Kết quả: OPEN, CLOSED, FILTERED hoặc UNRESOLVED.
 * Chỉ kiểm tra trên máy và mạng thuộc quyền quản lý của người thực hiện.
 * Biên dịch: javac -encoding UTF-8 -d out PortCheck.java
 * Chạy:      java -cp out PortCheck <host> <port> [timeout_ms]      (mặc định timeout_ms = 2000)
 */
public class PortCheck {
    public static void main(String[] args) throws IOException {
        if (args.length < 2) {
            System.out.println("Cách dùng: java -cp out PortCheck <host> <port> [timeout_ms]");
            System.exit(1);
        }
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        int timeoutMs = args.length > 2 ? Integer.parseInt(args[2]) : 2000;
        System.out.println("[port-check] Kiểm tra " + host + ":" + port + " (thời gian chờ " + timeoutMs + " ms)");
        String state = portCheck(host, port, timeoutMs);
        System.out.println("[port-check] Kết quả: " + host + ":" + port + " -> " + state);
    }

    /** FAMILY(a): INET cho IPv4, INET6 cho IPv6. */
    static String family(InetAddress a) {
        return (a instanceof Inet6Address) ? "INET6" : "INET";
    }

    /** Mã giả 1.1 – RESOLVE_HOST(name) (chép từ Bài 1.1): danh sách địa chỉ; rỗng nếu lỗi. */
    static List<InetAddress> resolveHost(String name) {
        try {
            List<InetAddress> addrs = List.of(InetAddress.getAllByName(name));   // RESOLVE(name)
            for (InetAddress a : addrs) {
                System.out.printf("  %-6s %s%n", family(a), a.getHostAddress());  // PRINT(FAMILY(a), ADDRESS(a))
            }
            return addrs;
        } catch (UnknownHostException e) {                                        // CATCH ResolveError AS e
            System.out.println("[port-check] Không phân giải được " + name
                    + " (UnknownHostException: " + e.getMessage() + ")");
            return List.of();                                                     // RETURN EMPTY_LIST
        }
    }

    /** Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms). */
    static String portCheck(String host, int port, int timeoutMs) throws IOException {
        List<InetAddress> addrs = resolveHost(host);           // dòng 2: lỗi phân giải → rỗng
        if (addrs.isEmpty()) return "UNRESOLVED";              // dòng 3
        String state = "FILTERED";                              // dòng 4: chưa địa chỉ nào phản hồi
        for (InetAddress a : addrs) {                           // dòng 5: FOR EACH a IN addrs
            String ip = a.getHostAddress();
            Socket s = new Socket();                            // dòng 6: SOCKET(FAMILY(a), STREAM) – họ chọn theo a khi connect
            try {
                s.connect(new InetSocketAddress(a, port), timeoutMs);   // dòng 7 + 9: CONNECT có thời gian chờ
                System.out.println("[port-check] " + ip + " cổng " + port + ": kết nối thành công");
                return "OPEN";                                  // dòng 10
            } catch (ConnectException e) {                      // dòng 11: CATCH ConnectionRefused
                if (e.getMessage() == null || !e.getMessage().contains("refused")) {   // ETIMEDOUT của HĐH ≈ Timeout
                    System.out.println("[port-check] " + ip + " cổng " + port + ": hết thời gian chờ");
                    continue;
                }
                System.out.println("[port-check] " + ip + " cổng " + port + ": bị từ chối (RST)");
                state = "CLOSED";                               // dòng 12
            } catch (SocketTimeoutException e) {                // dòng 13: CATCH Timeout
                System.out.println("[port-check] " + ip + " cổng " + port + ": hết thời gian chờ");
                continue;                                       // dòng 14
            } catch (NoRouteToHostException e) {                // dòng 13: HostUnreachable
                System.out.println("[port-check] " + ip + " cổng " + port + ": không có đường đi (" + e.getMessage() + ")");
                continue;
            } catch (SocketException e) {                       // dòng 13: NetworkUnreachable
                if (e.getMessage() == null || !e.getMessage().contains("unreachable")) throw e;   // lỗi khác: thoát ra
                System.out.println("[port-check] " + ip + " cổng " + port + ": không có đường đi (" + e.getMessage() + ")");
                continue;
            } finally {
                s.close();                                      // dòng 16: FINALLY CLOSE(s)
            }
        }
        return state;                                           // dòng 19: CLOSED hoặc FILTERED
    }
}
