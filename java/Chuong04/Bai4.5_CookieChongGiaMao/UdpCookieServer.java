import java.io.IOException;
import java.net.DatagramPacket;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.PortUnreachableException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.GeneralSecurityException;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.Map;
import javax.crypto.Mac;
import javax.crypto.spec.SecretKeySpec;

/**
 * Bài 4.5 – Server tra cứu UDP xác thực địa chỉ nguồn bằng mã thách thức không trạng thái
 * (Mã giả 4.8 COOKIE, HANDLE_DATAGRAM; Mục 4.6.1). Kế thừa Mã giả 4.3 – thay thân vòng lặp bằng HANDLE_DATAGRAM.
 * Chạy (trong thư mục bài, nơi có glossary.json): java -cp out UdpCookieServer [host] [port]  (mặc định 0.0.0.0 9100)
 */
public class UdpCookieServer {
    static final int MAX_DGRAM = 1200, MAX_RATIO = 3;
    static final byte[] KEY = new byte[32];                                              // dòng 1: RANDOM_BYTES(32)
    static Map<String, Object> DICT;

    /** PROCESS_REQUEST: nghiệp vụ, độc lập với socket (kế thừa nguyên vẹn). */
    static Map<String, Object> processRequest(Map<String, Object> req) {
        Map<String, Object> r = new LinkedHashMap<>();
        Object op = req.get("op");
        if ("lookup".equals(op)) {
            Object term = req.get("term");
            if (term instanceof String t && DICT.containsKey(t)) {
                r.put("status", 200); r.put("term", t); r.put("text", DICT.get(t));
            } else {
                r.put("status", 404); r.put("error", "Không có thuật ngữ");
            }
        } else if ("list".equals(op)) {
            r.put("status", 200); r.put("terms", new ArrayList<>(DICT.keySet()));
        } else {
            r.put("status", 400); r.put("error", "Sai thao tác");
        }
        return r;
    }

    static byte[] toBytes(Map<String, Object> obj) { return Json.stringify(obj).getBytes(StandardCharsets.UTF_8); }

    /** dòng 2–5: mã thách thức gắn với địa chỉ (ip, cổng) và khung thời gian w. */
    static String cookie(DatagramPacket p, long w) {
        try {
            Mac mac = Mac.getInstance("HmacSHA256");
            mac.init(new SecretKeySpec(KEY, "HmacSHA256"));
            String msg = p.getAddress().getHostAddress() + ":" + p.getPort() + ":" + w;
            return HexFormat.of().formatHex(mac.doFinal(msg.getBytes(StandardCharsets.UTF_8)));
        } catch (GeneralSecurityException e) {
            throw new IllegalStateException(e);
        }
    }

    /** SAFE_EQUAL: so sánh thời gian hằng, không phụ thuộc vị trí ký tự khác nhau đầu tiên. */
    static boolean safeEqual(String a, String b) {
        return MessageDigest.isEqual(a.getBytes(StandardCharsets.UTF_8), b.getBytes(StandardCharsets.UTF_8));
    }

    /** Mã giả 4.8, dòng 6–25; datagram bị bỏ chỉ được ghi nhật ký, không có phản hồi nào. */
    @SuppressWarnings("unchecked")
    static void handleDatagram(DatagramSocket s, DatagramPacket p) throws IOException {
        int n = p.getLength();
        String addr = p.getAddress().getHostAddress() + ":" + p.getPort();
        if (n > MAX_DGRAM) {                                                             // dòng 7: quá lớn: bỏ
            System.out.println("[server] Bỏ datagram " + n + " byte > MAX_DGRAM từ " + addr);
            return;
        }
        Object o;
        try {                                                                            // dòng 8–9 (UTF-8 chặt)
            o = Json.parse(StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(p.getData(), 0, n)).toString());
        } catch (IllegalArgumentException | CharacterCodingException e) {                // dòng 10–11: bỏ im lặng
            System.out.println("[server] Bỏ datagram không phải JSON từ " + addr);
            return;
        }
        if (!(o instanceof Map)) {                                                       // dòng 13
            System.out.println("[server] Bỏ datagram không phải đối tượng JSON từ " + addr);
            return;
        }
        Map<String, Object> req = (Map<String, Object>) o;
        long w = System.currentTimeMillis() / 1000 / 60;                                 // dòng 14
        Object c = req.get("cookie");
        boolean ok = c instanceof String cs && (safeEqual(cs, cookie(p, w)) || safeEqual(cs, cookie(p, w - 1))); // dòng 15–16
        String state = c == null ? "không có" : ok ? "hợp lệ" : "không hợp lệ";
        System.out.println("[server] Nhận " + n + " byte từ " + addr + " (id=" + Json.stringify(req.get("id")) + ", cookie: " + state + ")");
        Map<String, Object> resp = processRequest(req);                                  // dòng 17
        resp.put("id", req.get("id"));
        byte[] out = toBytes(resp);                                                      // dòng 18
        if (out.length > MAX_DGRAM) {                                                    // giữ kiểm tra của Mã giả 4.3
            resp = new LinkedHashMap<>();
            resp.put("id", req.get("id")); resp.put("status", 413); resp.put("error", "Quá lớn");
            out = toBytes(resp);
        }
        if (!ok && out.length > MAX_RATIO * n) {                                         // dòng 19
            Map<String, Object> ch = new LinkedHashMap<>();                              // dòng 20
            ch.put("id", req.get("id")); ch.put("status", 401); ch.put("cookie", cookie(p, w));
            byte[] small = toBytes(ch);                                                  // dòng 21: phản hồi nhỏ
            if (small.length > MAX_RATIO * n) {                                          // dòng 22: thách thức cũng quá lớn
                System.out.println("[server]   -> phản hồi " + out.length + " byte, thách thức " + small.length
                        + " byte đều > " + MAX_RATIO + " × " + n + " byte: không gửi gì");
                return;
            }
            System.out.println("[server]   -> phản hồi " + out.length + " byte > " + MAX_RATIO + " × " + n
                    + " byte: gửi thách thức 401 (" + small.length + " byte)");
            out = small;
        } else {
            System.out.println("[server]   -> gửi phản hồi status=" + resp.get("status") + " (" + out.length + " byte)");
        }
        s.send(new DatagramPacket(out, out.length, p.getSocketAddress()));               // dòng 24
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws IOException {
        new SecureRandom().nextBytes(KEY);
        DICT = (Map<String, Object>) Json.parse(Files.readString(Path.of("glossary.json")));
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (DatagramSocket s = new DatagramSocket(new InetSocketAddress(host, port))) {
            System.out.println("[server] Đang lắng nghe UDP tại " + host + ":" + port + ", từ điển " + DICT.size()
                    + " thuật ngữ, MAX_RATIO = " + MAX_RATIO);
            Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
            byte[] buf = new byte[65535];
            while (true) {
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                try { s.receive(p); } catch (PortUnreachableException e) { continue; }
                handleDatagram(s, p);
            }
        }
    }
}
