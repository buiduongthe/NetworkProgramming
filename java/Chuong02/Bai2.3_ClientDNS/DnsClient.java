import java.net.Inet6Address;
import java.net.InetAddress;
import java.net.UnknownHostException;
import java.util.Locale;
import java.util.regex.Pattern;

/**
 * Bài 2.3 – Client tra cứu DNS dùng bộ phân giải của hệ điều hành
 * (Mã giả 2.3, kế thừa Mã giả 1.1 RESOLVE_HOST; Mục 2.5 Giáo trình Lập trình mạng).
 * Biên dịch: javac -encoding UTF-8 -d out DnsClient.java
 * Chạy:      java -cp out DnsClient <tên miền | địa chỉ IP> ...
 * Ví dụ:     java -cp out DnsClient localhost 127.0.0.1 8.8.8.8
 */
public class DnsClient {
    private static final Pattern IPV4 = Pattern.compile("(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)(\\.(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)){3}");

    /** Mã giả 1.1 – RESOLVE_HOST: in và trả về danh sách địa chỉ; lỗi → danh sách rỗng. */
    static InetAddress[] resolveHost(String name) {
        try {
            InetAddress[] addrs = InetAddress.getAllByName(name);       // RESOLVE (getaddrinfo của HĐH)
            for (InetAddress a : addrs) {
                String family = (a instanceof Inet6Address) ? "INET6" : "INET";
                System.out.println("  " + family + "  " + a.getHostAddress()); // FAMILY(a), ADDRESS(a)
            }
            return addrs;
        } catch (UnknownHostException e) {                              // ResolveError
            System.out.println("  Không phân giải được " + name + " (" + e.getMessage() + ")");
            return new InetAddress[0];
        }
    }

    /** IS_IP_ADDRESS: JDK 21 chưa có InetAddress.ofLiteral (JDK 22+) nên kiểm tra dạng chuỗi. */
    static boolean isIpAddress(String q) {
        if (IPV4.matcher(q).matches()) return true;                     // IPv4 a.b.c.d, mỗi phần 0–255
        if (!q.contains(":")) return false;                             // tên miền không chứa ':'
        try {
            return InetAddress.getByName(q) instanceof Inet6Address;    // chuỗi có ':' được xử lý như IPv6, không tra DNS
        } catch (UnknownHostException e) {
            return false;
        }
    }

    public static void main(String[] args) throws UnknownHostException {
        if (args.length == 0) {                                         // dòng 1–5
            System.out.println("Cách dùng: java -cp out DnsClient <tên miền | địa chỉ IP> ...");
            return;
        }
        for (String q : args) {                                         // dòng 6
            long t0 = System.nanoTime();                                // dòng 7: t0 ← NOW()
            if (isIpAddress(q)) {                                       // dòng 8
                System.out.println("[dns] Tra cứu ngược: " + q);
                InetAddress ip = InetAddress.getByName(q);              // địa chỉ dạng số: không tra DNS
                String name = ip.getCanonicalHostName();                // REVERSE_RESOLVE: bản ghi PTR
                // Java không ném lỗi khi thiếu PTR mà trả lại chính địa chỉ dạng số → coi là ResolveError
                if (name.equals(ip.getHostAddress()) || name.equals(q))
                    System.out.println("  " + q + " → không có bản ghi PTR");
                else
                    System.out.println("  " + q + " → " + name);
            } else {
                System.out.println("[dns] Tra cứu xuôi: " + q);
                resolveHost(q);                                         // = Mã giả 1.1
            }
            double ms = (System.nanoTime() - t0) / 1e6;
            System.out.println(String.format(Locale.ROOT, "  Thời gian: %.2f ms", ms)); // dòng 18
        }
    }
}
