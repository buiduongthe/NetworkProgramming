import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.WebSocket;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Bài 7.3 – Client WebSocket dùng java.net.http.WebSocket (thư viện tự bắt tay, che khung, ghép khung).
 * Đọc từng dòng từ bàn phím, gửi khung văn bản, in khung trả lời; cuối cùng đóng với mã 1000.
 * Chạy: java -cp out WsClient [host] [port]      (mặc định 127.0.0.1 8080)
 */
public class WsClient {
    public static void main(String[] args) throws Exception {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        BlockingQueue<String> inbox = new LinkedBlockingQueue<>();    // sự kiện từ luồng của thư viện
        WebSocket.Listener listener = new WebSocket.Listener() {
            final StringBuilder text = new StringBuilder();
            public CompletionStage<?> onText(WebSocket ws, CharSequence data, boolean last) {
                text.append(data);                                      // ghép đến khung có FIN = 1
                if (last) { inbox.add("T" + text); text.setLength(0); }
                ws.request(1);
                return null;
            }
            public CompletionStage<?> onClose(WebSocket ws, int code, String reason) {
                inbox.add("C" + code);
                return null;
            }
        };
        WebSocket ws = HttpClient.newBuilder().connectTimeout(Duration.ofSeconds(3)).build()
                .newWebSocketBuilder().buildAsync(URI.create("ws://" + host + ":" + port + "/"), listener)
                .get(3, TimeUnit.SECONDS);                              // hoàn tất khi nhận 101 và Accept hợp lệ
        System.out.println("[client] Bắt tay thành công: HTTP/1.1 101 Switching Protocols");
        BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = console.readLine()) != null) {
            ws.sendText(line, true).join();
            String ev = inbox.poll(5, TimeUnit.SECONDS);
            if (ev == null) { System.out.println("[client] Hết thời gian chờ phản hồi"); break; }
            if (ev.startsWith("C")) { System.out.println("[client] Server đóng kết nối, mã " + ev.substring(1)); return; }
            System.out.println("[client] Nhận: '" + ev.substring(1) + "'");
        }
        ws.sendClose(WebSocket.NORMAL_CLOSURE, "").join();             // khung đóng 1000
        String ev = inbox.poll(5, TimeUnit.SECONDS);
        System.out.println("[client] Server xác nhận đóng, mã " + (ev != null ? ev.substring(1) : "?"));
        System.out.println("[client] Đã đóng kết nối");
    }
}
