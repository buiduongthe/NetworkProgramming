import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * Bài 7.4 – Server SSE nhỏ: phát luồng text/event-stream bằng chunked; một sự kiện cố ý bị cắt giữa dòng
 * qua hai khối. Mỗi kết nối một luồng (Mã giả 6.1).
 * Chạy: java -cp out SseServer [host] [port]      (mặc định 0.0.0.0 8080)
 */
public class SseServer {
    static final String[] EVENTS = {
        ": nhip tim\n\n",
        "retry: 3000\n\n",
        "id: 1\nevent: delta\ndata: {\"text\":\"Giao thức\"}\n\n",
        "id: 2\nevent: delta\ndata:{\"text\":\" TCP\"}\n\n",              // không có dấu cách sau ':' cũng hợp lệ
        "id: 3\ndata: dòng thứ nhất\ndata: dòng th",                     // sự kiện bị cắt giữa dòng ...
        "ứ hai\n\n",                                                     // ... và kết thúc ở khối sau
        "event: done\ndata: [DONE]\n\n",
    };

    public static void main(String[] args) throws IOException {
        String host = args.length >= 1 ? args[0] : "0.0.0.0";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 8080;
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();
                Thread t = new Thread(() -> serve(conn));
                t.setDaemon(true);
                t.start();
            }
        }
    }

    static void serve(Socket conn) {
        String addr = conn.getInetAddress().getHostAddress() + ":" + conn.getPort();
        try (conn) {
            Reader r = new Reader(conn.getInputStream());
            String line = r.readLine(8192);
            if (line == null) return;
            r.readHeaders();
            OutputStream out = conn.getOutputStream();
            if (!line.split(" ")[1].equals("/events")) {
                out.write("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".getBytes(StandardCharsets.US_ASCII));
                return;
            }
            System.out.println("[server] Client " + addr + " đăng ký luồng: " + line);
            out.write(("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n"
                    + "Cache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
            for (int i = 0; i < EVENTS.length; i++) {
                byte[] b = EVENTS[i].getBytes(StandardCharsets.UTF_8);  // một khối: độ dài hex, CRLF, dữ liệu, CRLF
                out.write((Integer.toHexString(b.length).toUpperCase() + "\r\n").getBytes(StandardCharsets.US_ASCII));
                out.write(b);
                out.write("\r\n".getBytes(StandardCharsets.US_ASCII));
                out.flush();
                System.out.println("[server] Gửi khối " + (i + 1) + " (" + b.length + " byte): '" + EVENTS[i].replace("\n", "\\n") + "'");
                Thread.sleep(300);                                      // sự kiện đến dần theo thời gian
            }
            out.write("0\r\n\r\n".getBytes(StandardCharsets.US_ASCII));  // khối 0: kết thúc thân
            out.flush();
            System.out.println("[server] Kết thúc luồng cho " + addr);
        } catch (IOException | InterruptedException e) {
            System.out.println("[server] Client " + addr + " ngắt kết nối: " + e.getMessage());
        }
    }
}
