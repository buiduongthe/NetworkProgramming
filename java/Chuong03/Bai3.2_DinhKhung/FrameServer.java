import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Map;

/**
 * Bài 3.2 – Echo Server dùng khung JSON (tiền tố độ dài 4 byte).
 * Khung sườn ECHO_SERVER của Mã giả 3.1 (Mục 3.2.2), thay HANDLE_ECHO bằng handleFrames dùng
 * RECV_FRAME/SEND_JSON của Mã giả 3.6 (Mục 3.4.3). {"text": ...} -> {"status":200,"text":...}; sai -> 400.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out FrameServer [host] [port]      (mặc định 0.0.0.0 9000)
 */
public class FrameServer {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {                      // SOCKET(INET, STREAM)
            srv.setReuseAddress(true);                                      // SETOPT(REUSEADDR)
            srv.bind(new InetSocketAddress(host, port), 16);                // BIND + LISTEN(16)
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {                                                  // vòng chấp nhận kết nối
                try (Socket conn = srv.accept()) {                          // ACCEPT; try-with-resources = CLOSE
                    String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
                    System.out.println("[server] Client kết nối: " + addr);
                    try {
                        handleFrames(conn, addr);
                    } catch (IOException e) {      // NetError, ConnectionClosed, ProtocolError: không dừng server
                        System.out.println("[server] Lỗi với " + addr + ": " + e.getMessage());
                    }
                    System.out.println("[server] Client " + addr + " đã đóng kết nối");
                }
            }
        }
    }

    /** Vòng phục vụ một client: mỗi khung nhận được là một thông điệp hoàn chỉnh. */
    static void handleFrames(Socket conn, String addr) throws IOException {
        while (true) {
            byte[] payload;
            try {
                payload = Framing.recvFrame(conn);          // RECV_JSON tách hai bước để ghi kích thước khung
            } catch (Framing.ConnectionClosed e) {
                if (e.received == 0) return;                // đóng tại ranh giới khung: kết thúc bình thường
                throw e;                                    // đóng giữa khung: lỗi
            }
            System.out.println("[server] Nhận khung " + payload.length + " byte từ " + addr);
            Object req;
            try {
                req = Framing.jsonDecode(payload);
            } catch (Json.JsonError e) {
                Framing.sendJson(conn, Json.obj("status", 400, "error", "JSON sai"));
                continue;
            }
            if (req instanceof Map<?, ?> m && m.get("text") instanceof String text)
                Framing.sendJson(conn, Json.obj("status", 200, "text", text));
            else
                Framing.sendJson(conn, Json.obj("status", 400, "error", "Thiếu trường text"));
        }
    }
}
