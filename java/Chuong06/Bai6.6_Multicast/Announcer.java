import java.io.IOException;
import java.net.DatagramPacket;
import java.net.InetSocketAddress;
import java.net.MulticastSocket;
import java.net.StandardSocketOptions;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.ThreadLocalRandom;

/**
 * Bài 6.6 – Phát thông báo dịch vụ qua multicast (Mã giả 6.8, ANNOUNCER; Mục 6.4.3).
 * Kế thừa Mã giả 6.7: thay địa chỉ broadcast bằng địa chỉ nhóm 239.255.42.99 (dải quản trị cục bộ).
 * Tham số count (bổ sung để minh họa): dừng sau count lần phát; 0 = phát mãi.
 * Chạy: java -cp out Announcer [service_port] [tên] [period_ms] [count] [mport]   (mặc định 9200 server-chat 5000 0 9998)
 */
public class Announcer {
    static final String GROUP = "239.255.42.99";                        // CONST GROUP
    static final int MPORT = 9998;                                      // CONST MPORT

    static void announcer(int servicePort, String name, int periodMs, int count, int mport)
            throws IOException, InterruptedException {
        try (MulticastSocket s = new MulticastSocket()) {               // SOCKET(INET, DGRAM)
            s.setTimeToLive(1);                                         // MULTICAST_TTL 1: chỉ trong mạng con
            s.setOption(StandardSocketOptions.IP_MULTICAST_LOOP, true); // nhận cả trên máy gửi
            InetSocketAddress dest = new InetSocketAddress(GROUP, mport);
            System.out.println("[announcer] Phát tới nhóm " + GROUP + ":" + mport + ", chu kỳ " + periodMs + " ms");
            int n = 0;
            while (count == 0 || n < count) {
                byte[] a = Json.encode(Json.obj("type", "announce", "name", name, "port", servicePort,
                        "ttl_ms", 3 * periodMs)).getBytes(StandardCharsets.UTF_8);
                s.send(new DatagramPacket(a, a.length, dest));
                n++;
                System.out.println("[announcer] Lần " + n + ": announce " + name + ", cổng " + servicePort
                        + ", ttl_ms=" + 3 * periodMs);
                Thread.sleep(periodMs + ThreadLocalRandom.current().nextInt(0, periodMs / 10 + 1));   // có nhiễu
            }
            System.out.println("[announcer] Dừng phát sau " + n + " lần");
        }
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        announcer(args.length > 0 ? Integer.parseInt(args[0]) : 9200, args.length > 1 ? args[1] : "server-chat",
                args.length > 2 ? Integer.parseInt(args[2]) : 5000, args.length > 3 ? Integer.parseInt(args[3]) : 0,
                args.length > 4 ? Integer.parseInt(args[4]) : MPORT);
    }
}
