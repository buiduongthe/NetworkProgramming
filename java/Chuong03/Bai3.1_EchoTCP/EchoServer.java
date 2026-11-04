import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;

/**
 * Bài 3.1 – TCP Echo Server tuần tự (Mã giả 3.1, Mục 3.2.2 Giáo trình Lập trình mạng).
 * Biên dịch: javac -d out *.java
 * Chạy:      java -cp out EchoServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class EchoServer {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {                  // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                  // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 16);            // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {                                              // vòng chấp nhận kết nối
                try (Socket conn = srv.accept()) {                      // ACCEPT; try-with-resources = CLOSE
                    String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
                    System.out.println("[server] Client kết nối: " + addr);
                    try {
                        handleEcho(conn);
                    } catch (IOException e) {                           // lỗi một client không dừng server
                        System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
                    }
                    System.out.println("[server] Client " + addr + " đã đóng kết nối");
                }
            }
        }
    }

    /** Vòng phục vụ một client: nhận bao nhiêu gửi trả bấy nhiêu. */
    static void handleEcho(Socket conn) throws IOException {
        InputStream in = conn.getInputStream();
        byte[] buf = new byte[4096];
        while (true) {
            int n = in.read(buf);                                       // RECV: trả về -1 khi client đóng chiều gửi
            if (n == -1) break;
            NetUtil.sendAll(conn.getOutputStream(), buf, n);            // SEND_ALL
        }
    }
}
