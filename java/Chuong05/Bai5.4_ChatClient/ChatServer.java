import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.time.LocalTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Bài 5.4 – Server chat tối giản để chạy thử lớp ChatClient: chỉ phần giao thức chat (Mục 6.3.1) mà
 * Mã giả 5.5 dùng: hello → welcome (409 nếu trùng tên), msg → chuyển tiếp tới mọi người trong một phòng
 * chung, ping → pong, sys khi có người vào, ra. Bản đầy đủ ở Chương 6. Mỗi client một khóa gửi.
 * Chạy: java -cp out ChatServer [host] [port]      (mặc định 0.0.0.0 9200)
 */
public class ChatServer {
    record Peer(Socket sock, ReentrantLock lock) {}
    static final Map<String, Peer> USERS = new ConcurrentHashMap<>();   // tên → client
    static final CompletableFuture<Void> NO_CANCEL = Khung.newCancellation();

    static void sendTo(String name, Object obj) throws IOException {
        Peer p = USERS.get(name);
        if (p == null) return;
        p.lock().lock();                                             // mỗi socket mỗi lúc chỉ ghi một khung
        try { Khung.sendJson(p.sock(), obj, NO_CANCEL); } finally { p.lock().unlock(); }
    }

    static void broadcast(Object obj) {
        for (String name : new ArrayList<>(USERS.keySet())) {        // sao chép danh sách rồi mới gửi
            try { sendTo(name, obj); } catch (IOException ignored) { }   // client vừa rời: bỏ qua
        }
    }

    static void chatSession(Socket conn) {
        String name = null;
        try (conn) {
            conn.setSoTimeout(10000);                                // hello trong 10 giây
            Map<?, ?> h = (Map<?, ?>) Khung.recvJson(conn, NO_CANCEL);
            Object n = h.get("name");
            if (!"hello".equals(h.get("type")) || !(n instanceof String s) || s.isEmpty()) {
                Khung.sendJson(conn, Json.obj("type", "error", "code", 400, "text", "Cần hello"), NO_CANCEL);
                return;
            }
            if (USERS.putIfAbsent(s, new Peer(conn, new ReentrantLock())) != null) {
                Khung.sendJson(conn, Json.obj("type", "error", "code", 409, "text", "Tên đã được dùng"), NO_CANCEL);
                return;
            }
            name = s;
            System.out.println("[server] " + name + " đã kết nối");
            sendTo(name, Json.obj("type", "welcome", "users", new ArrayList<>(USERS.keySet())));
            broadcast(Json.obj("type", "sys", "text", name + " đã vào phòng"));
            conn.setSoTimeout(90000);
            while (true) {
                Map<?, ?> m = (Map<?, ?>) Khung.recvJson(conn, NO_CANCEL);
                Object type = m.get("type");
                if ("msg".equals(type)) {
                    System.out.println("[server] " + name + ": " + m.get("text"));
                    Object room = m.get("room") instanceof String r ? r : "chung";
                    broadcast(Json.obj("type", "msg", "from", name, "room", room, "text", m.get("text"),
                            "ts", LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm:ss"))));
                } else if ("ping".equals(type)) {
                    sendTo(name, Json.obj("type", "pong"));
                } else {
                    sendTo(name, Json.obj("type", "error", "code", 400, "text", "Sai loại thông điệp"));
                }
            }
        } catch (IOException | RuntimeException e) {
            // kết thúc phiên bình thường hoặc do lỗi
        } finally {
            if (name != null && USERS.remove(name) != null) {
                System.out.println("[server] " + name + " đã rời phòng");
                broadcast(Json.obj("type", "sys", "text", name + " đã rời phòng"));
            }
        }
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = Khung.parseEndpoint(args, "0.0.0.0", 9200);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        Runtime.getRuntime().addShutdownHook(new Thread(() -> System.out.println("[server] Dừng server")));
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            System.out.println("[server] Đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();
                Thread.ofVirtual().start(() -> chatSession(conn));     // mỗi kết nối một luồng ảo
            }
        }
    }
}
