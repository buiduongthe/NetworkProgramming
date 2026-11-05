import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

/**
 * Bài mở rộng MR3.1 – Server đích để thử bộ chuyển tiếp cổng.
 *
 * Dịch vụ "chữ hoa": đọc toàn bộ dữ liệu tới khi client đóng chiều gửi (read trả về -1),
 * rồi gửi lại bản viết hoa và đóng kết nối. Vì server chỉ trả lời SAU KHI thấy EOF,
 * bộ chuyển tiếp ở giữa bắt buộc phải chuyển tiếp đúng việc đóng một chiều (half-close);
 * nếu không, client và server sẽ chờ nhau mãi.
 *
 * Chạy: java -cp out ServerDich [host] [port]      (mặc định 127.0.0.1 9000)
 */
public class ServerDich {
    static void handle(Socket conn) {
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        try (conn) {
            byte[] body = conn.getInputStream().readAllBytes();      // đọc tới EOF (client SHUTDOWN(WRITE))
            byte[] reply = new String(body, StandardCharsets.UTF_8).toUpperCase(Locale.ROOT).getBytes(StandardCharsets.UTF_8);
            conn.getOutputStream().write(reply);
            System.out.println("[dich] " + addr + " gửi " + body.length + " byte, trả " + reply.length + " byte chữ hoa");
        } catch (IOException e) {
            System.out.println("[dich] " + addr + ": lỗi " + e.getMessage());
        }
    }

    public static void main(String[] args) throws IOException {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int port = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[dich] Dừng server")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port));
            System.out.println("[dich] Server chữ hoa lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();
                Thread t = new Thread(() -> handle(conn));
                t.setDaemon(true);
                t.start();
            }
        }
    }
}
