import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;

/**
 * Bài 4.1 – UDP Echo Server (Mã giả 4.1, Mục 4.2.1 Giáo trình Lập trình mạng).
 * Không có LISTEN/ACCEPT; một socket phục vụ mọi client.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out UdpEchoServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class UdpEchoServer {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);                     // dòng 2
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        // dòng 3–4: SOCKET(INET, DGRAM) + BIND (DatagramSocket gộp hai bước)
        try (DatagramSocket s = new DatagramSocket(new InetSocketAddress(host, port))) {
            System.out.println("[server] Đang lắng nghe UDP tại " + host + ":" + port);
            Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
            byte[] buf = new byte[65535];
            while (true) {                                                               // dòng 5: LOOP
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                s.receive(p);                                                            // dòng 6: datagram nguyên vẹn
                System.out.println("[server] Nhận " + p.getLength() + " byte từ "
                        + p.getAddress().getHostAddress() + ":" + p.getPort());          // dòng 7
                s.send(new DatagramPacket(buf, p.getLength(), p.getSocketAddress()));    // dòng 8: trả lời đúng người gửi
            }
        }
    }
}
