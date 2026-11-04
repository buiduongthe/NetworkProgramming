import java.net.Inet6Address;
import java.net.InetAddress;
import java.net.UnknownHostException;
import java.util.List;

/**
 * Bài 1.1 – Phân giải tên miền thành danh sách địa chỉ
 * (Mã giả 1.1 – RESOLVE_HOST, Mục 1.2.3 Giáo trình Lập trình mạng).
 * Với mỗi tên, gọi bộ phân giải của hệ điều hành (getaddrinfo), in từng địa chỉ kèm họ địa chỉ;
 * lỗi phân giải được bắt và trả danh sách rỗng.
 * Biên dịch: javac -encoding UTF-8 -d out ResolveHost.java
 * Chạy:      java -cp out ResolveHost [tên ...]      (mặc định: localhost)
 */
public class ResolveHost {
    public static void main(String[] args) {
        String[] names = args.length > 0 ? args : new String[] {"localhost"};
        for (String name : names) {
            System.out.println("[resolve] Phân giải tên: " + name);
            List<InetAddress> addrs = resolveHost(name);
            System.out.println("[resolve] " + name + " -> " + addrs.size() + " địa chỉ");
        }
    }

    /** FAMILY(a): INET cho IPv4, INET6 cho IPv6. */
    static String family(InetAddress a) {
        return (a instanceof Inet6Address) ? "INET6" : "INET";
    }

    /** Mã giả 1.1 – RESOLVE_HOST(name): trả về danh sách địa chỉ; rỗng nếu lỗi. */
    static List<InetAddress> resolveHost(String name) {
        try {
            List<InetAddress> addrs = List.of(InetAddress.getAllByName(name));   // RESOLVE(name)
            for (InetAddress a : addrs) {                                         // FOR EACH a IN addrs
                System.out.printf("  %-6s %s%n", family(a), a.getHostAddress());  // PRINT(FAMILY(a), ADDRESS(a))
            }
            return addrs;                                                         // RETURN addrs
        } catch (UnknownHostException e) {                                        // CATCH ResolveError AS e
            System.out.println("[resolve] Không phân giải được " + name
                    + " (UnknownHostException: " + e.getMessage() + ")");
            return List.of();                                                     // RETURN EMPTY_LIST
        }
    }
}
