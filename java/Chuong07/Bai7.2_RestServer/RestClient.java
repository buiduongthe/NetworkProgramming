import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/**
 * Bài 7.2 – Client thử server REST: gửi nhiều yêu cầu trên MỘT kết nối giữ lại (keep-alive).
 * Mỗi dòng nhập: PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON], ví dụ  POST /api/v1/terms {"term":"QUIC","text":"..."}
 * Chạy: java -cp out RestClient [host] [port]      (mặc định 127.0.0.1 8080)
 */
public class RestClient {
    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, port), 3000);           // SAFE_CONNECT (rút gọn)
            s.setSoTimeout(5000);
            Reader r = new Reader(s.getInputStream());
            OutputStream out = s.getOutputStream();
            System.out.println("[client] Đã kết nối " + host + ":" + port + " (một kết nối cho mọi yêu cầu)");
            BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String input;
            while ((input = console.readLine()) != null) {
                String[] parts = input.trim().split(" ", 3);
                if (parts.length < 2) continue;
                String method = parts[0], path = parts[1];
                byte[] body = parts.length == 3 ? parts[2].getBytes(StandardCharsets.UTF_8) : new byte[0];
                StringBuilder h = new StringBuilder(method + " " + path + " HTTP/1.1\r\nHost: " + host + ":" + port
                        + "\r\nAccept: application/json\r\n");
                if (body.length > 0) h.append("Content-Type: application/json\r\nContent-Length: ").append(body.length).append("\r\n");
                h.append("\r\n");
                out.write(h.toString().getBytes(StandardCharsets.US_ASCII));
                out.write(body);
                out.flush();
                String statusLine = r.readLine(8192);
                if (statusLine == null) { System.out.println("[client] Server đã đóng kết nối"); break; }
                Map<String, String> hdr = r.readHeaders();
                byte[] data = r.readExact(Integer.parseInt(hdr.getOrDefault("content-length", "0")));
                System.out.println("[client] " + method + " " + path + " -> " + statusLine);
                if (hdr.containsKey("location")) System.out.println("[client]   Location: " + hdr.get("location"));
                if (data.length > 0) System.out.println("[client]   " + new String(data, StandardCharsets.UTF_8));
            }
        }
        System.out.println("[client] Đã đóng kết nối");
    }
}
