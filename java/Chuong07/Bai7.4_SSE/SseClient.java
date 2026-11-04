import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.BiConsumer;

/**
 * Bài 7.4 – Bộ đọc luồng Server-Sent Events (Mã giả 7.5 SSE_READ, Mục 7.3.3 Giáo trình Lập trình mạng).
 * Kế thừa: Mã giả 7.1 (gửi GET, đọc tiêu đề, giải mã chunked), 3.5 (READ_LINE).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out SseClient [host] [port] [đường_dẫn]   (mặc định 127.0.0.1 8080 /events)
 */
public class SseClient {
    static String lastEventId;
    static Integer reconnectMs;

    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        String path = args.length >= 3 ? args[2] : "/events";
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, port), 3000);           // SAFE_CONNECT (rút gọn)
            s.setSoTimeout(30000);
            s.getOutputStream().write(("GET " + path + " HTTP/1.1\r\nHost: " + host + ":" + port
                    + "\r\nAccept: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n")
                    .getBytes(StandardCharsets.US_ASCII));
            Reader r = new Reader(s.getInputStream());
            String status = r.readLine(8192);
            Map<String, String> hdr = r.readHeaders();
            System.out.println("[client] " + status + "; Content-Type: " + hdr.get("content-type"));
            if (!status.split(" ")[1].equals("200")) return;
            if (hdr.getOrDefault("transfer-encoding", "").toLowerCase().endsWith("chunked")) {
                System.out.println("[client] Thân dùng chunked: đặt bộ đọc dòng sau lớp giải mã khối");
                r = new Reader(new ChunkedInputStream(r));
            }
            sseRead(r, (type, data) ->                                  // in '\n' dạng hai ký tự
                    System.out.println("[client] Sự kiện '" + type + "': '" + data.replace("\n", "\\n") + "'"));
            System.out.println("[client] Server đóng luồng; id cuối = " + lastEventId + ", retry = " + reconnectMs + " ms");
        }
    }

    /** Mã giả 7.5: r là Reader đặt sau phần tiêu đề. */
    static void sseRead(Reader r, BiConsumer<String, String> onEvent) throws IOException {
        String type = "message";
        List<String> data = new ArrayList<>();
        String line;
        while ((line = r.readLine(65536)) != null) {
            if (line.isEmpty()) {                                       // dòng trống: hết một sự kiện
                if (!data.isEmpty()) onEvent.accept(type, String.join("\n", data));
                type = "message"; data = new ArrayList<>();
            } else if (!line.startsWith(":")) {                        // ":" mở đầu dòng chú thích
                int i = line.indexOf(':');
                String field = i >= 0 ? line.substring(0, i) : line, value = i >= 0 ? line.substring(i + 1) : "";
                if (value.startsWith(" ")) value = value.substring(1);   // STRIP_ONE_SPACE
                switch (field) {
                    case "event" -> type = value;
                    case "data" -> data.add(value);
                    case "id" -> lastEventId = value;
                    case "retry" -> reconnectMs = Integer.parseInt(value);
                    default -> { }                                      // trường lạ: bỏ qua
                }
            }
        }                                                               // null: server đóng luồng
    }
}
