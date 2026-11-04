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
 * Bài 4.2 – Client tra cứu trên UDP dùng Mã giả 4.4 UDP_REQUEST: gửi lại có lùi thời gian,
 * ghép mã định danh (Mục 4.3.1). Chạy: java -cp out UdpLookupClient [host] [port]   (mặc định 127.0.0.1 9100)
 */
public class UdpLookupClient {
    static long nextId = 0;

    static long now() { return System.nanoTime() / 1_000_000; }                         // NOW() tính bằng ms

    /** Mã giả 4.4 – UDP_REQUEST; các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả. */
    static Map<String, Object> udpRequest(DatagramSocket s, InetSocketAddress server, Map<String, Object> req,
                                          int timeoutMs, int maxTries) throws IOException {
        long id = ++nextId;
        req.put("id", id);                                                               // dòng 2: NEXT_ID()
        byte[] payload = Json.stringify(req).getBytes(StandardCharsets.UTF_8);           // dòng 3
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

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        InetSocketAddress server = new InetSocketAddress(InetAddress.getAllByName(host)[0], port);
        try (DatagramSocket s = new DatagramSocket()) {
            System.out.println("[client] Gửi yêu cầu tới " + host + ":" + port + " qua UDP (chờ 500 ms, tối đa 4 lần). "
                    + "Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String line;
            while ((line = console.readLine()) != null) {                                // READ_CONSOLE()
                String term = line.trim();
                if (term.isEmpty()) continue;
                System.out.println("[client] Tra cứu '" + term + "'");
                long t0 = now();
                Map<String, Object> req = new LinkedHashMap<>();
                req.put("op", "lookup"); req.put("term", term);
                Map<String, Object> resp;
                try {
                    resp = udpRequest(s, server, req, 500, 4);
                } catch (IOException e) {
                    System.out.println("[client] Lỗi: " + e.getMessage() + " (" + (now() - t0) + " ms)");
                    continue;
                }
                if (Long.valueOf(200).equals(resp.get("status")))
                    System.out.println("[client] " + resp.get("term") + ": " + resp.get("text") + " (" + (now() - t0) + " ms)");
                else
                    System.out.println("[client] Lỗi " + resp.get("status") + ": " + resp.get("error") + " (" + (now() - t0) + " ms)");
            }
        }
        System.out.println("[client] Đã đóng socket");
    }
}
