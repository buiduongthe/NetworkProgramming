import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.PortUnreachableException;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Bài 4.5 – Client tra cứu UDP xử lý mã thách thức (đoạn văn sau Mã giả 4.8, Mục 4.6.1): nhận 401 kèm cookie
 * thì lưu mã và gửi lại yêu cầu có trường cookie; các yêu cầu sau dùng lại mã đó. Kế thừa Mã giả 4.4 UDP_REQUEST.
 * Chạy: java -cp out UdpCookieClient [host] [port] [cookie_ban_dau]   (mặc định 127.0.0.1 9100)
 */
public class UdpCookieClient {
    static long nextId = 0;
    /** Yêu cầu chưa có cookie được đệm khoảng trắng tới MIN_FIRST byte để thách thức 401 (khoảng 100 byte)
     *  không vượt MAX_RATIO = 3 lần yêu cầu (đoạn văn sau Mã giả 4.8). */
    static final int MIN_FIRST = 48;

    static long now() { return System.nanoTime() / 1_000_000; }                         // NOW() tính bằng ms

    /** Mã giả 4.4 – UDP_REQUEST (chép từ Bài 4.2); các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả.
     *  padTo (bổ sung): đệm khoảng trắng sau JSON tới padTo byte – JSON hợp lệ cho phép khoảng trắng cuối. */
    static Map<String, Object> udpRequest(DatagramSocket s, InetSocketAddress server, Map<String, Object> req,
                                          int timeoutMs, int maxTries, int padTo) throws IOException {
        long id = ++nextId;
        req.put("id", id);                                                               // dòng 2: NEXT_ID()
        String json = Json.stringify(req);
        byte[] payload = json.getBytes(StandardCharsets.UTF_8);                          // dòng 3
        if (payload.length < padTo)
            payload = (json + " ".repeat(padTo - payload.length)).getBytes(StandardCharsets.UTF_8);
        byte[] buf = new byte[65535];
        for (int attempt = 1; attempt <= maxTries; attempt++) {                          // dòng 4
            s.send(new DatagramPacket(payload, payload.length, server));                 // dòng 5
            System.out.println("[client]   Lần " + attempt + ": gửi id=" + id + " (" + payload.length
                    + " byte), chờ tối đa " + timeoutMs + " ms");
            long deadline = now() + timeoutMs;                                           // dòng 6
            while (now() < deadline) {                                                   // dòng 7
                s.setSoTimeout((int) Math.max(1, deadline - now()));                     // dòng 8: phần còn lại
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                Object resp;
                try {
                    s.receive(p);                                                        // dòng 10
                    resp = Json.parse(new String(buf, 0, p.getLength(), StandardCharsets.UTF_8)); // dòng 11
                } catch (SocketTimeoutException | PortUnreachableException e) {          // dòng 12–13: Timeout,
                    break;                                                               // ConnectionReset (Windows)
                } catch (IllegalArgumentException e) {                                   // dòng 14–15: không phải JSON
                    continue;
                }
                if (p.getSocketAddress().equals(server) && resp instanceof Map<?, ?> m
                        && m.get("id") instanceof Long rid && rid == id) {
                    @SuppressWarnings("unchecked") Map<String, Object> r = (Map<String, Object>) m;
                    return r;                                                            // dòng 17
                }
                System.out.println("[client]   Bỏ datagram lạ hoặc muộn từ "
                        + p.getAddress().getHostAddress() + ":" + p.getPort());
            }
            System.out.println("[client]   Hết " + timeoutMs + " ms không có phản hồi");
            timeoutMs *= 2;                                                              // dòng 19: lùi theo cấp số nhân
        }
        throw new IOException("Không có phản hồi sau " + maxTries + " lần");             // dòng 21: NetError
    }

    static Map<String, Object> lookup(DatagramSocket s, InetSocketAddress server, String term, String cookie)
            throws IOException {
        Map<String, Object> req = new LinkedHashMap<>();
        req.put("op", "lookup"); req.put("term", term);
        if (cookie != null) {
            req.put("cookie", cookie);                                                   // gửi kèm cookie đã lưu
            return udpRequest(s, server, req, 500, 4, 0);
        }
        return udpRequest(s, server, req, 500, 4, MIN_FIRST);                            // chưa có cookie: đệm yêu cầu
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        String cookie = args.length > 2 ? args[2] : null;                                // cookie ban đầu (thêm)
        InetSocketAddress server = new InetSocketAddress(InetAddress.getAllByName(host)[0], port);
        try (DatagramSocket s = new DatagramSocket()) {
            System.out.println("[client] Gửi yêu cầu tới " + host + ":" + port + " qua UDP. Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String line;
            while ((line = console.readLine()) != null) {
                String term = line.trim();
                if (term.isEmpty()) continue;
                System.out.println("[client] Tra cứu '" + term + "' (" + (cookie != null ? "kèm cookie" : "không kèm cookie") + ")");
                Map<String, Object> resp;
                try {
                    resp = lookup(s, server, term, cookie);
                    if (Long.valueOf(401).equals(resp.get("status")) && resp.get("cookie") instanceof String c) {
                        cookie = c;                                                      // lưu mã thách thức
                        System.out.println("[client] Nhận thách thức 401, lưu cookie " + cookie.substring(0, 16) + "…; gửi lại kèm cookie");
                        resp = lookup(s, server, term, cookie);                          // gửi lại đúng một lần
                    }
                } catch (IOException e) {
                    System.out.println("[client] Lỗi: " + e.getMessage());
                    continue;
                }
                if (Long.valueOf(200).equals(resp.get("status")))
                    System.out.println("[client] " + resp.get("term") + ": " + resp.get("text"));
                else
                    System.out.println("[client] Lỗi " + resp.get("status") + ": " + resp.getOrDefault("error", "không được phục vụ"));
            }
        }
        System.out.println("[client] Đã đóng socket");
    }
}
