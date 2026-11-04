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

/**
 * Bài 4.1 – UDP Echo Client có thời gian chờ (Mã giả 4.2, Mục 4.2.1).
 * Chạy: java -cp out UdpEchoClient [host] [port]      (mặc định 127.0.0.1 9000)
 */
public class UdpEchoClient {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);                   // dòng 2
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        InetSocketAddress server = new InetSocketAddress(InetAddress.getAllByName(host)[0], port); // dòng 3
        try (DatagramSocket s = new DatagramSocket()) {                                  // dòng 4: SOCKET(DGRAM)
            s.setSoTimeout(2000);                                                        // dòng 5: RECV_TIMEOUT 2000 ms
            System.out.println("[client] Gửi tới " + host + ":" + port
                    + " qua UDP. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            byte[] buf = new byte[65535];
            String line;
            while ((line = console.readLine()) != null) {                                // dòng 6: READ_CONSOLE()
                byte[] data = line.getBytes(StandardCharsets.UTF_8);
                s.send(new DatagramPacket(data, data.length, server));                   // dòng 7: SENDTO
                try {
                    DatagramPacket p = new DatagramPacket(buf, buf.length);
                    s.receive(p);                                                        // dòng 9: RECVFROM
                    System.out.println("[client] Gửi: '" + line + "' -> Nhận: '"
                            + new String(buf, 0, p.getLength(), StandardCharsets.UTF_8) + "'");
                } catch (SocketTimeoutException | PortUnreachableException e) {          // dòng 11: CATCH Timeout, ConnectionReset
                    System.out.println("[client] Gửi: '" + line + "' -> Không có phản hồi: datagram có thể đã mất");
                }
            }
        }                                                                                // dòng 15: CLOSE
        System.out.println("[client] Đã đóng socket");
    }
}
