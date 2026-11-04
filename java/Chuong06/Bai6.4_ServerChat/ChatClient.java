import java.io.BufferedReader;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;
import java.util.concurrent.locks.ReentrantLock;
import java.util.function.Consumer;

/**
 * Bài 6.4 – Client chat dòng lệnh tối giản dùng lớp ChatClient (chép gọn từ Mã giả 5.5, Mục 5.3.3).
 * Lệnh: /join phòng, /leave phòng, /pm tên nội dung, /ping, /quit; dòng khác là tin msg gửi tới phòng hiện tại.
 * Bộ hẹn giờ gửi ping mỗi 30 giây. Hết dữ liệu nhập: chờ 1 giây để nhận nốt rồi ngắt kết nối.
 * Chạy: java -cp out ChatClient [host] [port] [tên]      (mặc định 127.0.0.1 9200 user)
 */
public class ChatClient {
    Socket sock;
    OutputStream out;
    volatile boolean cancelled;                                         // tín hiệu hủy (cts)
    final ReentrantLock sendLock = new ReentrantLock();                 // send_lock
    final Consumer<Map<String, Object>> onMessage;
    final Consumer<String> onStatus;

    ChatClient(Consumer<Map<String, Object>> onMessage, Consumer<String> onStatus) {
        this.onMessage = onMessage;
        this.onStatus = onStatus;
    }

    void connect(String host, int port, String name) throws IOException {
        sock = NetUtil.safeConnect(host, port, 3000);                   // SAFE_CONNECT, 3000 ms
        out = sock.getOutputStream();
        try {
            NetUtil.sendJson(out, Json.obj("type", "hello", "name", name));
        } catch (IOException e) {
            sock.close();                                               // không để rò socket
            throw e;
        }
        onStatus.accept("connected");                                   // báo trước khi vòng nhận chạy nền
        Thread.ofVirtual().start(this::receiveLoop);                    // RUN_BACKGROUND
    }

    void receiveLoop() {
        try {
            DataInputStream in = new DataInputStream(sock.getInputStream());
            while (true) onMessage.accept(NetUtil.recvJson(in));
        } catch (IOException | Json.JsonError e) {
            if (cancelled) return;                                      // ngắt kết nối chủ động
            try { sock.close(); } catch (IOException ignored) { }       // giải phóng socket
            onStatus.accept("lost: " + (e instanceof NetUtil.ConnectionClosed cc && cc.received == 0
                    ? "server đã đóng kết nối" : e.getMessage()));
        }
    }

    void send(Map<String, Object> m) throws IOException {
        sendLock.lock();                                                // mỗi lúc gửi một khung
        try {
            NetUtil.sendJson(out, m);
        } finally {
            sendLock.unlock();
        }
    }

    void disconnect() {
        cancelled = true;                                               // hủy trước, đóng socket sau
        try { sock.close(); } catch (IOException ignored) { }
    }

    @SuppressWarnings("unchecked")
    static void show(Map<String, Object> m) {
        Object t = m.get("type");
        String ts = String.valueOf(m.getOrDefault("ts", ""));
        String hhmmss = ts.length() >= 19 ? ts.substring(11, 19) : "";
        String line;
        if ("welcome".equals(t)) {
            List<Object> rooms = (List<Object>) m.get("rooms");
            line = "[client] Chào mừng! Trực tuyến: " + String.join(", ", (List<String>) (List<?>) m.get("users"))
                    + "; phòng: " + (rooms.isEmpty() ? "(chưa có)" : String.join(", ", (List<String>) (List<?>) rooms));
        } else if ("msg".equals(t)) line = "[client] " + hhmmss + " [" + m.get("room") + "] " + m.get("from") + ": " + m.get("text");
        else if ("pm".equals(t)) line = "[client] " + hhmmss + " [riêng] " + m.get("from") + ": " + m.get("text");
        else if ("sys".equals(t)) line = "[client] * " + m.get("text");
        else if ("error".equals(t)) line = "[client] Lỗi " + m.get("code") + ": " + m.get("text");
        else if ("pong".equals(t)) line = "[client] pong";
        else line = "[client] " + Json.encode(m);
        System.out.println(line);
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9200);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        String name = args.length > 2 ? args[2] : "user";
        ChatClient c = new ChatClient(ChatClient::show, s -> System.out.println("connected".equals(s)
                ? "[client] Đã kết nối " + host + ":" + port + ", gửi hello với tên " + name
                : "[client] Mất kết nối: " + s.substring(6)));
        try {
            c.connect(host, port, name);
        } catch (IOException e) {
            System.out.println("[client] Không kết nối được " + host + ":" + port + ": " + e.getMessage());
            return;
        }
        Thread.ofVirtual().start(() -> {                                // bộ hẹn giờ nhịp tim 30 giây
            try {
                while (true) { Thread.sleep(30000); c.send(Json.obj("type", "ping")); }
            } catch (IOException | InterruptedException ignored) { }
        });
        BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String room = null, line;                                       // phòng hiện tại
        boolean quit = false;
        while ((line = console.readLine()) != null) {                   // READ_CONSOLE
            line = line.trim();
            if (line.isEmpty()) continue;
            System.out.println("[client] > " + line);
            String[] p = line.split(" ", 3);
            try {
                if (p[0].equals("/join") && p.length > 1) { room = line.substring(6); c.send(Json.obj("type", "join", "room", room)); }
                else if (p[0].equals("/leave") && p.length > 1) c.send(Json.obj("type", "leave", "room", line.substring(7)));
                else if (p[0].equals("/pm") && p.length == 3) c.send(Json.obj("type", "pm", "to", p[1], "text", p[2]));
                else if (p[0].equals("/ping")) c.send(Json.obj("type", "ping"));
                else if (p[0].equals("/quit")) { quit = true; break; }
                else if (line.startsWith("/")) System.out.println("[client] Lệnh không hợp lệ");
                else if (room == null) System.out.println("[client] Chưa chọn phòng: dùng /join tên_phòng");
                else c.send(Json.obj("type", "msg", "room", room, "text", line));
            } catch (IOException e) {
                System.out.println("[client] Gửi lỗi: " + e.getMessage());
                quit = true;
                break;
            }
        }
        if (!quit) Thread.sleep(1000);                                  // hết dữ liệu nhập: chờ nhận nốt
        c.disconnect();
        System.out.println("[client] Đã ngắt kết nối");
    }
}
