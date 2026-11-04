import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/**
 * Bài 3.3 – Client tra cứu thuật ngữ (Mã giả 3.8, Mục 3.5.1): giữ phần kết nối của ECHO_CLIENT
 * (Mã giả 3.2), thay vòng gửi nhận thô bằng SEND_JSON/RECV_JSON (Mã giả 3.6).
 * Mỗi dòng nhập là một thuật ngữ cần tra; hết dữ liệu nhập thì gửi {"op":"bye"}.
 * Chạy: java -cp out LookupClient [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class LookupClient {
    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (Socket s = NetUtil.safeConnect(host, port, 3000)) {
            s.setSoTimeout(5000);                                           // RECV_TIMEOUT 5000 ms
            System.out.println("[client] Đã kết nối " + host + ":" + port
                    + ". Nhập thuật ngữ cần tra (Ctrl+Z/Ctrl+D để kết thúc).");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String term;
            while ((term = console.readLine()) != null) {                   // READ_CONSOLE()
                Framing.sendJson(s, Json.obj("op", "lookup", "term", term));
                Map<?, ?> resp = (Map<?, ?>) Framing.recvJson(s);
                if (Long.valueOf(200).equals(resp.get("status")))
                    System.out.println("[client] " + resp.get("term") + ": " + resp.get("text"));
                else
                    System.out.println("[client] Lỗi " + resp.get("status") + ": " + resp.get("error") + " ('" + term + "')");
            }
            Framing.sendJson(s, Json.obj("op", "bye"));
        }                                                                   // try-with-resources = CLOSE
        System.out.println("[client] Đã đóng kết nối");
    }
}
