import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;

/**
 * Bài 2.2 – Server thử nghiệm tối giản: chấp nhận kết nối, in địa chỉ client rồi đóng ngay.
 * Chỉ dùng để tạo trường hợp kết nối thành công; chuỗi SOCKET – BIND – LISTEN – ACCEPT trình bày ở Chương 3.
 * Chạy: java -cp out TestServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class TestServer {
    public static void main(String[] args) throws IOException, ConfigError {
        Object[] ep = Endpoint.parse(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {                   // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                  // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 16);            // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                try (Socket conn = srv.accept()) {                      // ACCEPT; try-with-resources = CLOSE ngay
                    System.out.println("[server] Client kết nối: "
                            + conn.getInetAddress().getHostAddress() + ":" + conn.getPort());
                }
            }
        }
    }
}
