import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * Bài 3.1 – TCP Echo Client, phiên bản đầu chưa định khung (Mã giả 3.2, Mục 3.2.3).
 * Lưu ý: read() giả định nhận đủ phản hồi trong một lần; khắc phục ở Bài 3.2 (định khung).
 * Chạy: java -cp out EchoClient [host] [port]      (mặc định 127.0.0.1 9000)
 */
public class EchoClient {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (Socket s = safeConnect(host, port, 3000)) {
            s.setSoTimeout(5000);                                       // RECV_TIMEOUT 5000 ms
            System.out.println("[client] Đã kết nối " + host + ":" + port
                    + ". Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            InputStream in = s.getInputStream();
            byte[] buf = new byte[4096];
            String line;
            while ((line = console.readLine()) != null) {               // READ_CONSOLE()
                byte[] data = line.getBytes(StandardCharsets.UTF_8);    // ENCODE(line, "UTF-8")
                NetUtil.sendAll(s.getOutputStream(), data, data.length);
                int n = in.read(buf);                                   // giả định nhận đủ một lần
                if (n == -1) break;
                System.out.println("[client] Gửi: '" + line + "' -> Nhận: '"
                        + new String(buf, 0, n, StandardCharsets.UTF_8) + "'");
            }
            s.shutdownOutput();                                         // SHUTDOWN(WRITE)
        }
        System.out.println("[client] Đã đóng kết nối");
    }

    /** Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws IOException {
        IOException last = null;
        for (InetAddress ip : InetAddress.getAllByName(host)) {
            Socket s = new Socket();
            try {
                s.connect(new InetSocketAddress(ip, port), timeoutMs);
                return s;
            } catch (IOException e) {
                last = e;
                s.close();
            }
        }
        throw new IOException("Không kết nối được " + host + ":" + port + ": " + (last == null ? "" : last.getMessage()));
    }
}
