import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;

/**
 * Bài 6.5 – Khám phá server chat trong LAN bằng broadcast: phía server (Mã giả 6.7, RESPONDER; Mục 6.4.2).
 * Nhận yêu cầu discover, chờ ngẫu nhiên 0–200 ms (giãn phản hồi) rồi trả lời bằng unicast kèm cổng dịch vụ.
 * Trong server thật, RESPONDER chạy trên một luồng (ảo) riêng bên cạnh vòng chấp nhận kết nối.
 * Chạy: java -cp out Responder [service_port] [tên] [disc_port]   (mặc định 9200 server-chat 9999)
 */
public class Responder {
    static final int DISC_PORT = 9999;                                  // CONST DISC_PORT ← 9999

    static void responder(int servicePort, String name, int discPort) throws IOException, InterruptedException {
        try (DatagramSocket s = new DatagramSocket(null)) {             // SOCKET(INET, DGRAM), chưa gắn cổng
            s.setReuseAddress(true);                                    // cho nhiều responder trên một máy
            s.bind(new InetSocketAddress("0.0.0.0", discPort));         // BIND(s, "0.0.0.0", DISC_PORT)
            System.out.println("[responder] " + name + ", cổng dịch vụ " + servicePort + ", chờ discover tại cổng " + discPort);
            byte[] buf = new byte[2048];
            while (true) {
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                s.receive(p);                                           // RECVFROM (chặn trên luồng của responder)
                Map<String, Object> q;
                try {
                    q = Json.decodeObject(new String(p.getData(), 0, p.getLength(), StandardCharsets.UTF_8));
                } catch (Json.JsonError e) {
                    continue;                                           // sai định dạng: bỏ
                }
                if (!"discover".equals(q.get("type")) || !"chat".equals(q.get("service"))) continue;
                int delay = ThreadLocalRandom.current().nextInt(0, 201);
                System.out.println("[responder] discover id=" + q.get("id") + " từ " + p.getAddress().getHostAddress()
                        + ":" + p.getPort() + ", trả lời sau " + delay + " ms");
                Thread.sleep(delay);                                    // DELAY: giãn phản hồi
                byte[] r = Json.encode(Json.obj("id", q.get("id"), "port", servicePort, "name", name)).getBytes(StandardCharsets.UTF_8);
                s.send(new DatagramPacket(r, r.length, p.getSocketAddress()));   // unicast về người hỏi
            }
        }
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        int servicePort = args.length > 0 ? Integer.parseInt(args[0]) : 9200;
        String name = args.length > 1 ? args[1] : "server-chat";
        int discPort = args.length > 2 ? Integer.parseInt(args[2]) : DISC_PORT;
        responder(servicePort, name, discPort);
    }
}
