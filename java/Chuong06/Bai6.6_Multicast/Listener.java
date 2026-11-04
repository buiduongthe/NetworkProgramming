import java.io.IOException;
import java.net.DatagramPacket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.MulticastSocket;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài 6.6 – Nhận thông báo dịch vụ qua multicast (Mã giả 6.8, LISTENER; Mục 6.4.3): bật REUSEADDR trước BIND,
 * tham gia nhóm (IGMP join), cập nhật bộ nhớ đệm và xóa dịch vụ hết hạn.
 * Tham số run_ms (bổ sung để minh họa): nghe trong run_ms rồi in bộ nhớ đệm; 0 = nghe mãi.
 * Chạy: java -cp out Listener [run_ms] [mport]      (mặc định 0 9998)
 */
public class Listener {
    static final String GROUP = "239.255.42.99";                        // CONST GROUP
    static final int MPORT = 9998;                                      // CONST MPORT

    record Entry(String ip, Object port, long expires) {}

    static long nowMs() { return System.nanoTime() / 1_000_000; }

    /** REMOVE_EXPIRED: xóa dịch vụ đã quá hạn ttl_ms. */
    static void removeExpired(Map<String, Entry> cache, long now) {
        List<String> old = new ArrayList<>();
        cache.forEach((n, e) -> { if (e.expires() < now) old.add(n); });
        for (String n : old) {
            cache.remove(n);
            System.out.println("[listener] Hết hạn, xóa: " + n);
        }
    }

    static void listener(Map<String, Entry> cache, int runMs, int mport) throws IOException {
        try (MulticastSocket s = new MulticastSocket(null)) {           // SOCKET(INET, DGRAM), chưa gắn cổng
            s.setReuseAddress(true);                                    // nhiều tiến trình cùng nghe
            s.bind(new InetSocketAddress("0.0.0.0", mport));            // BIND(s, "0.0.0.0", MPORT)
            s.joinGroup(new InetSocketAddress(InetAddress.getByName(GROUP), 0), null);   // IGMP join, giao diện mặc định
            System.out.println("[listener] Tham gia nhóm " + GROUP + ", nghe cổng " + mport);
            long end = nowMs() + runMs;
            byte[] buf = new byte[2048];
            while (runMs == 0 || nowMs() < end) {
                if (runMs > 0) s.setSoTimeout((int) Math.max(1, end - nowMs()));
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                Map<String, Object> a;
                try {
                    s.receive(p);                                       // RECVFROM(s, 2048)
                    a = Json.decodeObject(new String(p.getData(), 0, p.getLength(), StandardCharsets.UTF_8));
                } catch (SocketTimeoutException e) {
                    break;
                } catch (Json.JsonError e) {
                    continue;                                           // sai định dạng: bỏ
                }
                if ("announce".equals(a.get("type"))) {
                    String name = String.valueOf(a.get("name"));
                    String ip = p.getAddress().getHostAddress();
                    if (!cache.containsKey(name)) System.out.println("[listener] Thêm " + name + " tại " + ip + ":" + a.get("port"));
                    cache.put(name, new Entry(ip, a.get("port"), nowMs() + ((Number) a.get("ttl_ms")).longValue()));
                }
                removeExpired(cache, nowMs());                          // dịch vụ hết hạn: xóa
            }
        }
    }

    public static void main(String[] args) throws IOException {
        int runMs = args.length > 0 ? Integer.parseInt(args[0]) : 0;
        Map<String, Entry> cache = new LinkedHashMap<>();
        listener(cache, runMs, args.length > 1 ? Integer.parseInt(args[1]) : MPORT);
        List<String> items = new ArrayList<>();
        cache.forEach((n, e) -> items.add(n + " tại " + e.ip() + ":" + e.port()));
        System.out.println("[listener] Bộ nhớ đệm: " + (items.isEmpty() ? "(trống)" : String.join(", ", items)));
    }
}
