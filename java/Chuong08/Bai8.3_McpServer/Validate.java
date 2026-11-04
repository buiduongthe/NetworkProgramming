import java.net.InetAddress;
import java.net.UnknownHostException;
import java.util.ArrayList;
import java.util.List;

/** Bài 8.3 – Mã giả 8.7: kiểm soát đích của công cụ mạng (Mục 8.5.4); mọi đích phải thuộc ALLOWED_NETS. */
public class Validate {
    /** Một mạng CIDR: địa chỉ mạng + độ dài tiền tố. */
    record Cidr(byte[] net, int prefix, String text) {
        boolean contains(InetAddress a) {                                    // so khớp tiền tố từng bit
            byte[] b = a.getAddress();
            if (b.length != net.length) return false;                        // IPv4 khác IPv6
            for (int i = 0; i < prefix; i++) {
                int mask = 0x80 >> (i % 8);
                if ((b[i / 8] & mask) != (net[i / 8] & mask)) return false;
            }
            return true;
        }
        @Override public String toString() { return text; }
    }

    static final List<Cidr> ALLOWED_NETS = parseCidrs(System.getenv("ALLOWED_NETS"));

    /** PARSE_CIDRS: CIDR phân cách dấu phẩy; biến vắng -> danh sách rỗng (chặn mọi đích). */
    static List<Cidr> parseCidrs(String text) {
        List<Cidr> out = new ArrayList<>();
        if (text == null) return out;
        for (String x : text.split(",")) {
            x = x.trim();
            if (x.isEmpty()) continue;
            String[] parts = x.split("/");
            if (!parts[0].matches("[0-9a-fA-F:.]+")) throw new IllegalArgumentException("CIDR không hợp lệ: " + x);
            try {
                byte[] net = InetAddress.getByName(parts[0]).getAddress();   // chỉ nhận địa chỉ dạng số
                int prefix = parts.length > 1 ? Integer.parseInt(parts[1]) : net.length * 8;
                if (prefix < 0 || prefix > net.length * 8) throw new IllegalArgumentException("Tiền tố sai: " + x);
                for (int i = prefix; i < net.length * 8; i++)
                    if ((net[i / 8] & (0x80 >> (i % 8))) != 0) throw new IllegalArgumentException("Có bit máy: " + x);
                out.add(new Cidr(net, prefix, x));
            } catch (UnknownHostException | NumberFormatException e) {
                throw new IllegalArgumentException("CIDR không hợp lệ: " + x);
            }
        }
        return out;
    }

    /** IS_IP_ADDRESS cho JDK 21 (chưa có InetAddress.ofLiteral): kiểm tra dạng chuỗi trước – bốn số thập phân
     *  0–255 phân cách bằng dấu chấm, hoặc chuỗi chứa ':' của IPv6 – vì getByName sẽ tra DNS nếu nhận tên miền. */
    static boolean isIpAddress(String h) {
        if (h.matches("\\d{1,3}(\\.\\d{1,3}){3}")) {
            for (String part : h.split("\\.")) if (Integer.parseInt(part) > 255) return false;
            return true;
        }
        if (!h.contains(":") || !h.matches("[0-9A-Fa-f:.]+")) return false;
        try { InetAddress.getByName(h); return true; }                       // dạng số: không tra DNS
        catch (UnknownHostException e) { return false; }
    }

    static String validateName(Object host) throws NetTools.PolicyError {
        if (host instanceof String h && isIpAddress(h)) return h;            // dòng 3: kể cả IPv6 "::1"
        if (!(host instanceof String h) || h.length() > 253 || !h.matches("[A-Za-z0-9.-]+"))
            throw new NetTools.PolicyError("Tên máy không hợp lệ");
        return h;
    }

    static InetAddress validateTarget(Object host) throws NetTools.PolicyError, NetTools.ResolveError {
        String name = validateName(host);
        InetAddress[] addrs;
        try {
            addrs = InetAddress.getAllByName(name);                          // RESOLVE
        } catch (UnknownHostException e) {
            throw new NetTools.ResolveError("Không phân giải được " + name);
        }
        for (InetAddress a : addrs) {
            boolean ok = ALLOWED_NETS.stream().anyMatch(n -> n.contains(a)); // IN_ANY
            if (!ok) throw new NetTools.PolicyError("Ngoài phạm vi cho phép: " + a.getHostAddress());
        }
        return addrs[0];                      // dùng đúng địa chỉ đã kiểm tra (chống DNS rebinding)
    }

    static int validatePort(Object p) throws NetTools.PolicyError {
        if (!(p instanceof Long n) || n < 1 || n > 65535)                    // IS_INTEGER, phạm vi
            throw new NetTools.PolicyError("Cổng không hợp lệ");
        return n.intValue();
    }
}
