import java.io.ByteArrayOutputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.URI;
import java.net.URISyntaxException;
import java.net.UnknownHostException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR8.4 – Thân công cụ fetch_url: bản NGÂY THƠ và bản AN TOÀN (chống SSRF).
 *
 * Dùng bởi McpServerSsrf.java. SSRF (Server-Side Request Forgery) là lỗ hổng khi server nhận một URL
 * từ bên ngoài rồi tự đi tải URL đó: kẻ tấn công ép server truy cập dịch vụ nội bộ (máy chủ siêu dữ liệu
 * đám mây 169.254.169.254, dịch vụ chỉ nghe loopback, mạng riêng 10/8…) mà client bên ngoài không với tới.
 *
 * Hai bộ bảng được "tiêm" (inject) qua biến môi trường để demo chạy hoàn toàn offline, tất định:
 *   SSRF_RESOLVER = {"tên miền": ["ip1", "ip2", ...]}  – bảng phân giải tên giả lập; nếu danh sách có
 *                   nhiều IP thì mỗi lần gọi trả IP kế tiếp (mô phỏng DNS rebinding / TOCTOU).
 *   SSRF_DIAL     = {"ip": cổng}                        – nối tới 127.0.0.1:cổng thay cho ip thật
 *                   (để một "địa chỉ công khai" 203.0.113.x hay 169.254.169.254 trỏ về server cục bộ).
 * Hai bảng này CHỈ tồn tại cho bản demo offline; thực tế dùng DNS thật và nối thẳng tới IP đã kiểm tra.
 */
final class FetchTools {
    /** Một mạng CIDR (IPv4 hoặc IPv6) dạng mảng byte + độ dài tiền tố. */
    record Net(String text, byte[] addr, int prefix) {
        static Net of(String cidr) {
            String[] p = cidr.split("/");
            try {
                return new Net(cidr, InetAddress.getByName(p[0]).getAddress(), Integer.parseInt(p[1]));
            } catch (UnknownHostException e) {
                throw new IllegalArgumentException(cidr);
            }
        }

        boolean contains(byte[] a) {
            if (a.length != addr.length) return false;                // khác phiên bản IP
            int full = prefix / 8, rest = prefix % 8;
            for (int i = 0; i < full; i++) if (a[i] != addr[i]) return false;
            if (rest == 0) return true;
            int mask = (0xFF << (8 - rest)) & 0xFF;
            return (a[full] & mask) == (addr[full] & mask);
        }
    }

    // ---- Dải địa chỉ BỊ CHẶN (liệt kê tường minh, KHÔNG dựa vào isSiteLocalAddress vì phân loại khác nhau) ----
    static final List<Net> BLOCKED_NETS = List.of(
            Net.of("0.0.0.0/8"),        // địa chỉ "không xác định" / mạng này
            Net.of("127.0.0.0/8"),      // loopback
            Net.of("10.0.0.0/8"),       // RFC 1918
            Net.of("172.16.0.0/12"),    // RFC 1918
            Net.of("192.168.0.0/16"),   // RFC 1918
            Net.of("169.254.0.0/16"),   // link-local, gồm 169.254.169.254 (siêu dữ liệu đám mây)
            Net.of("::1/128"),          // loopback IPv6
            Net.of("::/128"),           // không xác định IPv6
            Net.of("fc00::/7"),         // IPv6 unique-local
            Net.of("fe80::/10"));       // IPv6 link-local
    static final int MAX_REDIRECT = 5;
    static final int MAX_BYTES = 64 * 1024;          // giới hạn kích thước phản hồi
    static final double HAN_GIAY = 5.0;              // giới hạn tổng thời gian một lần fetch

    /** URL bị chính sách an toàn từ chối (trả về mô hình với isError = true). */
    static final class SsrfError extends Exception {
        SsrfError(String m) { super(m); }
    }

    /** Lỗi khi thực thi (không nối được, phản hồi hỏng…). */
    static final class FetchError extends Exception {
        FetchError(String m) { super(m); }
    }

    /** Kết quả một lần fetch: mã trạng thái, thân và nhật ký từng bước. */
    record Ket(int status, byte[] body, List<String> log) { }

    // -------------------- Bảng phân giải và bảng "dial" được tiêm (chỉ cho demo) --------------------
    static final Map<String, List<String>> RESOLVER = new HashMap<>();
    static final Map<String, Integer> DIAL = new HashMap<>();

    static {
        Object r = Json.parse(System.getenv().getOrDefault("SSRF_RESOLVER", "{}"));
        for (Map.Entry<?, ?> e : ((Map<?, ?>) r).entrySet()) {
            List<String> l = new ArrayList<>();
            for (Object x : (List<?>) e.getValue()) l.add((String) x);
            RESOLVER.put((String) e.getKey(), l);
        }
        Object d = Json.parse(System.getenv().getOrDefault("SSRF_DIAL", "{}"));
        for (Map.Entry<?, ?> e : ((Map<?, ?>) d).entrySet())
            DIAL.put((String) e.getKey(), ((Number) e.getValue()).intValue());
    }

    /** Host có phải địa chỉ IP dạng số (IPv4 chấm thập phân hoặc IPv6) không – KHÔNG truy vấn DNS. */
    static boolean laDiaChiIp(String host) {
        if (host.matches("(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)(\\.(25[0-5]|2[0-4]\\d|1\\d\\d|[1-9]?\\d)){3}")) return true;
        if (!host.contains(":") || !host.matches("[0-9a-fA-F:.]+")) return false;
        try {
            InetAddress.getByName(host);                      // chuỗi chỉ gồm chữ số hex và ':' -> phân tích trực tiếp
            return true;
        } catch (UnknownHostException e) {
            return false;
        }
    }

    /**
     * Phân giải tên -> một địa chỉ IP (chuỗi). Nếu host đã là IP thì trả nguyên.
     * Với bảng giả lập: mỗi lần gọi trả IP kế tiếp trong danh sách (IP cuối "dính"), nhờ đó có thể
     * mô phỏng DNS rebinding: lần kiểm tra trả IP công khai, lần nối thật trả IP nội bộ.
     */
    static String resolveOnce(String host) throws FetchError {
        if (laDiaChiIp(host)) return host;                    // host vốn là địa chỉ IP dạng số
        List<String> lst = RESOLVER.get(host);
        if (lst != null) {
            String ip = lst.get(0);
            if (lst.size() > 1) lst.remove(0);                // advance: lần sau trả IP kế tiếp
            return ip;
        }
        try {                                                  // không có trong bảng giả lập -> DNS thật (ngoài demo)
            return InetAddress.getByName(host).getHostAddress();
        } catch (UnknownHostException e) {
            throw new FetchError("Không phân giải được tên " + host);
        }
    }

    /** Chặn nếu IP thuộc dải nội bộ/loopback/link-local… (dùng cho bản AN TOÀN). */
    static void validateIp(String ip) throws SsrfError {
        byte[] a;
        try {
            a = InetAddress.getByName(ip).getAddress();       // ip đã ở dạng số -> không truy vấn DNS
        } catch (UnknownHostException e) {
            throw new SsrfError("Chặn SSRF: địa chỉ không hợp lệ " + ip);
        }
        for (Net net : BLOCKED_NETS)
            if (net.contains(a)) throw new SsrfError("Chặn SSRF: địa chỉ " + ip + " thuộc dải cấm " + net.text());
    }

    /** Mở socket tới (ip, port). Bảng DIAL (chỉ demo) cho phép ánh xạ ip -> 127.0.0.1:cổng_cục_bộ. */
    static Socket dial(String ip, int port, int timeoutMs) throws IOException {
        Socket s = new Socket();
        Integer local = DIAL.get(ip);
        if (local != null) s.connect(new InetSocketAddress("127.0.0.1", local), timeoutMs);
        else s.connect(new InetSocketAddress(InetAddress.getByName(ip), port), timeoutMs);
        return s;
    }

    /** Các thành phần URL đã tách: scheme, có thông tin đăng nhập không, host, port, path (kèm query). */
    record Url(String scheme, boolean userInfo, String host, int port, String path) { }

    static Url splitUrl(String url) throws FetchError {
        URI u;
        try {
            u = new URI(url);
        } catch (URISyntaxException e) {
            throw new FetchError("URL không hợp lệ: " + url);
        }
        String scheme = u.getScheme() == null ? "" : u.getScheme().toLowerCase();
        String host = u.getHost() == null ? "" : u.getHost().toLowerCase();
        if (host.startsWith("[") && host.endsWith("]")) host = host.substring(1, host.length() - 1);   // [::1] -> ::1
        int port = u.getPort() != -1 ? u.getPort() : (scheme.equals("https") ? 443 : 80);
        String path = u.getRawPath() == null || u.getRawPath().isEmpty() ? "/" : u.getRawPath();
        if (u.getRawQuery() != null && !u.getRawQuery().isEmpty()) path += "?" + u.getRawQuery();
        return new Url(scheme, u.getRawUserInfo() != null, host, port, path);
    }

    /** Ghép địa chỉ chuyển hướng (có thể tương đối) với URL hiện tại – tương đương urljoin. */
    static String join(String base, String location) throws FetchError {
        try {
            return new URI(base).resolve(new URI(location)).toString();
        } catch (URISyntaxException e) {
            throw new FetchError("Location không hợp lệ: " + location);
        }
    }

    record Http(int status, Map<String, String> headers, byte[] body) { }

    /** Gửi GET HTTP/1.0 (Connection: close) tới IP đã chọn, đọc phản hồi (giới hạn kích thước). */
    static Http httpGet(String ip, String host, int port, String path, double timeout, long limitBytes) throws IOException {
        ByteArrayOutputStream data = new ByteArrayOutputStream();
        try (Socket s = dial(ip, port, (int) (timeout * 1000))) {
            s.setSoTimeout((int) (timeout * 1000));
            String req = "GET " + path + " HTTP/1.0\r\nHost: " + host + "\r\nAccept: */*\r\nConnection: close\r\n\r\n";
            s.getOutputStream().write(req.getBytes(StandardCharsets.UTF_8));
            InputStream in = s.getInputStream();
            byte[] buf = new byte[4096];
            while (data.size() < limitBytes) {
                int n = in.read(buf);
                if (n == -1) break;
                data.write(buf, 0, n);
            }
        }
        byte[] all = data.toByteArray();
        int sep = -1;
        for (int i = 0; i + 3 < all.length; i++)
            if (all[i] == '\r' && all[i + 1] == '\n' && all[i + 2] == '\r' && all[i + 3] == '\n') { sep = i; break; }
        String head = new String(all, 0, sep < 0 ? all.length : sep, StandardCharsets.ISO_8859_1);
        byte[] body = sep < 0 ? new byte[0] : java.util.Arrays.copyOfRange(all, sep + 4, all.length);
        String[] lines = head.split("\r\n", -1);
        String[] first = lines[0].split(" ");
        int status = first.length > 1 ? Integer.parseInt(first[1]) : 0;
        Map<String, String> headers = new LinkedHashMap<>();
        for (int i = 1; i < lines.length; i++) {
            int c = lines[i].indexOf(':');
            if (c >= 0) headers.put(lines[i].substring(0, c).strip().toLowerCase(), lines[i].substring(c + 1).strip());
        }
        return new Http(status, headers, body);
    }

    static boolean laChuyenHuong(int st) {
        return st == 301 || st == 302 || st == 303 || st == 307 || st == 308;
    }

    /**
     * Bản NGÂY THƠ: KHÔNG kiểm tra gì. Phân giải tên khi "kiểm tra" rồi phân giải LẠI khi nối
     * (lỗi TOCTOU), theo mọi chuyển hướng, không giới hạn. Đây là ví dụ dễ bị SSRF – không dùng thật.
     */
    static Ket fetchNaive(String url) throws FetchError, IOException {
        List<String> log = new ArrayList<>();
        for (int k = 0; k <= MAX_REDIRECT; k++) {
            Url u = splitUrl(url);
            resolveOnce(u.host());                            // "kiểm tra" hình thức rồi vứt đi (vô dụng)
            String ip = resolveOnce(u.host());                // nối bằng lần phân giải MỚI -> rebinding lọt
            log.add("nối tới " + u.host() + " (" + ip + "):" + u.port() + u.path());
            Http r = httpGet(ip, u.host(), u.port(), u.path(), HAN_GIAY, 1L << 30);
            if (laChuyenHuong(r.status()) && r.headers().containsKey("location")) {
                url = join(url, r.headers().get("location"));
                log.add("theo chuyển hướng -> " + url);
                continue;
            }
            return new Ket(r.status(), r.body(), log);
        }
        throw new FetchError("Quá nhiều chuyển hướng");
    }

    /** Bản AN TOÀN: lần lượt áp các lớp phòng thủ SSRF. */
    static Ket fetchSafe(String url) throws SsrfError, FetchError, IOException {
        List<String> log = new ArrayList<>();
        long deadline = System.nanoTime() + (long) (HAN_GIAY * 1e9);
        for (int hop = 0; hop <= MAX_REDIRECT; hop++) {
            Url u = splitUrl(url);
            if (!u.scheme().equals("http") && !u.scheme().equals("https"))       // (1) chỉ cho http/https
                throw new SsrfError("Chặn SSRF: giao thức '" + u.scheme() + "' không được phép");
            if (u.userInfo())                                                     // (2) loại bỏ thông tin đăng nhập
                log.add("gỡ bỏ thông tin đăng nhập nhúng trong URL");
            if (u.host().isEmpty()) throw new SsrfError("Chặn SSRF: URL thiếu tên máy");
            String ip = resolveOnce(u.host());                                    // (3) phân giải MỘT lần
            validateIp(ip);                                                       // (4) chặn dải nội bộ/loopback/link-local
            double conLai = (deadline - System.nanoTime()) / 1e9;
            if (conLai <= 0) throw new FetchError("Hết thời gian cho phép");      // (5) giới hạn tổng thời gian
            log.add("cho phép " + u.host() + " (" + ip + "):" + u.port() + u.path() + "; nối tới ĐÚNG IP đã kiểm tra");
            // (6) nối tới chính IP đã kiểm tra (chống DNS rebinding/TOCTOU), (7) giới hạn kích thước
            Http r = httpGet(ip, u.host(), u.port(), u.path(), Math.min(2.0, conLai), MAX_BYTES);
            if (laChuyenHuong(r.status()) && r.headers().containsKey("location")) {
                if (hop >= MAX_REDIRECT)                                          // (8) giới hạn số lần chuyển hướng
                    throw new SsrfError("Chặn SSRF: vượt số lần chuyển hướng cho phép");
                url = join(url, r.headers().get("location"));
                log.add("kiểm tra lại chặng chuyển hướng -> " + url);
                continue;                                                         // (9) kiểm tra lại MỌI chặng
            }
            return new Ket(r.status(), r.body(), log);
        }
        throw new FetchError("Quá nhiều chuyển hướng");
    }

    static final PrintStream ERR = new PrintStream(new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8);

    /** Nhật ký chỉ ghi ra stderr (stdout dành riêng cho JSON-RPC). */
    static void logErr(String... parts) {
        ERR.println("[ssrf] " + String.join(" ", parts));
    }
}
