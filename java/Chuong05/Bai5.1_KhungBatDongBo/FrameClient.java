import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;

/**
 * Bài 5.1 – Client tra cứu dùng khung JSON (minh họa Mã giả 5.1); kế thừa Mã giả 3.8 (LOOKUP_CLIENT).
 * In 4 byte tiêu đề của mỗi khung và minh họa hủy một lời gọi nhận đang chờ (Mục 5.4.1).
 * Chạy: java -cp out FrameClient [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class FrameClient {
    public static void main(String[] args) throws Exception {
        Object[] ep = Khung.parseEndpoint(args, "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        Socket s;
        try {
            s = Khung.safeConnect(host, port, 3000);
        } catch (Khung.NetError e) {
            System.out.println("[client] Không kết nối được: " + e.getMessage());
            return;
        }
        System.out.println("[client] Đã kết nối " + host + ":" + port + ". Nhập thuật ngữ cần tra (Ctrl+D để kết thúc).");
        BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        try (s) {
            String term;
            while ((term = console.readLine()) != null) {              // READ_CONSOLE()
                Map<String, Object> req = Json.obj("op", "lookup", "term", term);
                byte[] frame = Khung.makeFrame(req);
                System.out.printf("[client] Gửi khung %d byte, tiêu đề %02X %02X %02X %02X: %s%n",
                        frame.length, frame[0], frame[1], frame[2], frame[3], Json.stringify(req));
                CompletableFuture<Void> timeout = Khung.cancelAfter(5000);  // RECV_TIMEOUT 5000 ms
                Khung.linkClose(timeout, s);
                Khung.sendJson(s, req, timeout);
                Map<?, ?> resp = (Map<?, ?>) Khung.recvJson(s, timeout);
                timeout.cancel(false);                                 // xong trước hạn: gỡ hẹn giờ
                if (Long.valueOf(200).equals(resp.get("status")))
                    System.out.println("[client] " + resp.get("term") + ": " + resp.get("text"));
                else
                    System.out.println("[client] Lỗi " + resp.get("status") + ": " + resp.get("error"));
            }
            // Minh họa hủy: chờ một khung mà server không gửi, tín hiệu hủy kích hoạt sau 500 ms
            CompletableFuture<Void> ct = Khung.cancelAfter(500);       // CANCEL(ct) sau 500 ms
            Khung.linkClose(ct, s);
            try {
                Khung.recvJson(s, ct);
            } catch (CancellationException e) {
                System.out.println("[client] RECV_JSON_ASYNC bị hủy sau 500 ms (Cancelled); "
                        + "trạng thái khung không xác định nên đóng kết nối");
            }
        } catch (IOException e) {
            System.out.println("[client] Lỗi: " + e.getMessage());
        }
        System.out.println("[client] Đã đóng kết nối");               // try-with-resources = CLOSE
    }
}
