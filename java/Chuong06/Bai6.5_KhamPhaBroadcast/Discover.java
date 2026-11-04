import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;

/**
 * Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía client (Mã giả 6.7, hàm DISCOVER; Mục 6.4.2).
 * Kế thừa Mã giả 4.4 (UDP_REQUEST): gửi một yêu cầu nhưng thu thập NHIỀU phản hồi đến hết thời gian chờ.
 * Chạy: java -cp out Discover [wait_ms] [disc_port] [địa_chỉ_broadcast]   (mặc định 2000 9999 255.255.255.255)
 */
public class Discover {
    static final int DISC_PORT = 9999;                                  // CONST DISC_PORT ← 9999

    record Found(String ip, Object port, Object name) {}

    static List<Found> discover(int waitMs, int discPort, String bcast) throws IOException {
        try (DatagramSocket s = new DatagramSocket()) {                 // SOCKET(INET, DGRAM)
            s.setBroadcast(true);                                       // SETOPT(BROADCAST): bắt buộc
            long id = ThreadLocalRandom.current().nextInt(1, 1000000);  // NEXT_ID()
            byte[] q = Json.encode(Json.obj("type", "discover", "service", "chat", "id", id)).getBytes(StandardCharsets.UTF_8);
            s.send(new DatagramPacket(q, q.length, new InetSocketAddress(bcast, discPort)));
            System.out.println("[client] Phát discover id=" + id + " tới " + bcast + ":" + discPort);
            System.out.println("[client] Thu thập phản hồi trong " + waitMs + " ms");
            List<Found> found = new ArrayList<>();
            long deadline = System.currentTimeMillis() + waitMs;
            byte[] buf = new byte[2048];
            while (System.currentTimeMillis() < deadline) {
                s.setSoTimeout((int) Math.max(1, deadline - System.currentTimeMillis()));   // ≠ 0 (0 = chờ vô hạn)
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                Map<String, Object> r;
                try {
                    s.receive(p);                                       // RECVFROM(s, 2048)
                    r = Json.decodeObject(new String(p.getData(), 0, p.getLength(), StandardCharsets.UTF_8));
                } catch (SocketTimeoutException e) {
                    break;                                              // Timeout
                } catch (Json.JsonError e) {
                    continue;                                           // sai định dạng: bỏ
                }
                if (Long.valueOf(id).equals(r.get("id")))
                    found.add(new Found(p.getAddress().getHostAddress(), r.get("port"), r.get("name")));
            }
            return found;
        }
    }

    public static void main(String[] args) throws IOException {
        int waitMs = args.length > 0 ? Integer.parseInt(args[0]) : 2000;
        int discPort = args.length > 1 ? Integer.parseInt(args[1]) : DISC_PORT;
        String bcast = args.length > 2 ? args[2] : "255.255.255.255";
        List<Found> found = discover(waitMs, discPort, bcast);
        System.out.println("[client] Tìm thấy " + found.size() + " server:");
        for (Found f : found) System.out.println("[client]   " + f.name() + " tại " + f.ip() + ":" + f.port());
    }
}
