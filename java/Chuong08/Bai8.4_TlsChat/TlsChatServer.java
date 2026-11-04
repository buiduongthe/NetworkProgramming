import java.io.FileDescriptor;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PrintStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.util.*;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import javax.net.ssl.KeyManagerFactory;
import javax.net.ssl.SSLContext;
import javax.net.ssl.SSLSocket;

/**
 * Bài 8.4 – Server chat tối giản có TLS và xác thực token (Mã giả 8.6 phía server; Mục 8.4.5).
 * Kế thừa Mã giả 6.5–6.6 (CHAT_SESSION, BROADCAST, WRITER – bản rút gọn: hello, join, msg, ping); Java dùng
 * luồng ảo với mã chặn thay cho ASYNC (Mục 5.2.3). Cấu hình: TLS_CERT (tệp PKCS12), TLS_KEY_PASS, CHAT_JWT_KEY.
 * Chạy: java -cp out TlsChatServer [host] [port]      (mặc định 0.0.0.0 9200)
 */
public class TlsChatServer {
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
    static final Map<String, Client> users = new HashMap<>();
    static final Map<String, Set<String>> rooms = new HashMap<>();
    static final Object lock = new Object();
    static final Map<String, Object> CLOSE = Map.of();                     // dấu hiệu đóng hộp thư đi

    static class HelloError extends Exception { final int code; HelloError(int c, String t) { super(t); code = c; } }

    static class Client {
        final String name; final Socket sock; final OutputStream out;
        final BlockingQueue<Map<String, Object>> outbox = new ArrayBlockingQueue<>(256);   // BOUNDED_QUEUE(256)
        Client(String n, Socket s) throws IOException { name = n; sock = s; out = s.getOutputStream(); }
    }

    static void log(String t) { OUT.println("[server] " + t); }

    static void enqueue(Client c, Map<String, Object> msg) {
        if (!c.outbox.offer(msg)) {                                        // hàng đợi đầy: ngắt client chậm
            log("Ngắt client chậm " + c.name);
            try { c.sock.close(); } catch (IOException ignored) { }
        }
    }

    static void sendTo(String name, Map<String, Object> msg, Client sender) {
        Client c;
        synchronized (lock) { c = users.get(name); }
        if (c != null) enqueue(c, msg);
        else if (sender != null) enqueue(sender, Json.obj("type", "error", "code", 404, "text", "Không có người nhận"));
    }

    static void broadcast(String room, Map<String, Object> msg, Client sender) {
        List<String> targets;
        synchronized (lock) { targets = new ArrayList<>(rooms.getOrDefault(room, Set.of())); }   // giữ khóa ngắn
        if (sender != null && !targets.contains(sender.name)) {
            enqueue(sender, Json.obj("type", "error", "code", 403, "text", "Chưa tham gia phòng"));
            return;
        }
        for (String name : targets) sendTo(name, msg, null);
    }

    static void writer(Client c) {                                          // tác vụ ghi duy nhất của mỗi client
        try {
            Map<String, Object> msg;
            while ((msg = c.outbox.take()) != CLOSE) Frame.sendJson(c.out, msg);
        } catch (IOException | InterruptedException e) {
            try { c.sock.close(); } catch (IOException ignored) { }
        }
    }

    /** CHAT_SESSION (Mã giả 6.5) trên kết nối đã bọc TLS; hello mang token thay cho tên. */
    static void chatSession(SSLSocket conn, String addr) {
        Client c = null;
        Thread w = null;
        try (conn) {
            try {
                InputStream in = conn.getInputStream();
                conn.setSoTimeout(10000);                                   // hello trong 10 giây
                Map<String, Object> h = Frame.recvJson(in);
                if (!"hello".equals(h.get("type"))) throw new HelloError(400, "Thông điệp đầu phải là hello");
                Map<String, Object> claims;
                try { claims = Jwt.verify(Frame.str(h.get("token")), Jwt.secret("CHAT_JWT_KEY"), Jwt.ISSUER, "chat"); }
                catch (Jwt.JwtError e) { throw new HelloError(401, "Token không hợp lệ: " + e.getMessage()); }
                String sub = (String) claims.get("sub");
                synchronized (lock) { if (!users.containsKey(sub)) users.put(sub, c = new Client(sub, conn)); }   // REGISTER
                if (c == null) throw new HelloError(409, "Tên đã được sử dụng");
                log(c.name + " đăng nhập từ " + addr + " (tên lấy từ claim sub)");
                Client cc = c;
                w = Thread.ofVirtual().start(() -> writer(cc));                // RUN_BACKGROUND(WRITER(c))
                List<Object> names;
                synchronized (lock) { names = new ArrayList<>(new TreeSet<>(users.keySet())); }
                enqueue(c, Json.obj("type", "welcome", "name", c.name, "users", names));
                conn.setSoTimeout(90000);                                   // ba chu kỳ nhịp tim
                while (true) {
                    Map<String, Object> m = Frame.recvJson(in);
                    String room = Frame.str(m.get("room"));
                    switch (String.valueOf(m.get("type"))) {
                        case "join" -> {
                            synchronized (lock) { if (room != null) rooms.computeIfAbsent(room, k -> new HashSet<>()).add(cc.name); }
                            broadcast(room, Json.obj("type", "sys", "text", c.name + " đã vào phòng " + room), null);
                        }
                        case "msg" -> broadcast(room, Json.obj("type", "msg", "from", c.name, "room", room,
                                "text", m.get("text"), "ts", System.currentTimeMillis() / 1000.0), c);
                        case "ping" -> enqueue(c, Json.obj("type", "pong"));
                        default -> enqueue(c, Json.obj("type", "error", "code", 400, "text", "Loại thông điệp không hỗ trợ"));
                    }
                }
            } catch (HelloError e) {                                        // lời chào không hợp lệ
                log("Từ chối " + addr + ": " + e.code + " " + e.getMessage());
                Frame.sendJson(conn.getOutputStream(), Json.obj("type", "error", "code", e.code, "text", e.getMessage()));
            } catch (IOException e) {                                       // đóng kết nối, hết thời gian chờ, lỗi giao thức
                if (c == null) log("Kết nối " + addr + " đóng trước khi đăng nhập: " + e.getMessage());
            } finally {
                if (c != null) {                                            // UNREGISTER, đóng hộp thư đi
                    synchronized (lock) { users.remove(c.name); for (Set<String> s : rooms.values()) s.remove(c.name); }
                    c.outbox.offer(CLOSE);
                    if (w != null) w.join(1000);
                    log(c.name + " rời phiên");
                }
            }
        } catch (IOException | InterruptedException ignored) { }           // CLOSE(conn) do try-with-resources
    }

    /** Mã giả 8.6 – TLS_SESSION(conn, addr): bắt tay TLS ở vai trò server trước phiên chat. */
    static void tlsSession(SSLContext ctx, Socket raw) {
        String addr = raw.getInetAddress().getHostAddress() + ":" + raw.getPort();
        SSLSocket tls;
        try {
            tls = (SSLSocket) ctx.getSocketFactory().createSocket(raw, null, raw.getPort(), true);
            tls.setUseClientMode(false);                                    // vai trò SERVER
            tls.setEnabledProtocols(new String[]{"TLSv1.3", "TLSv1.2"});  // min ← TLS1_2
            tls.setSoTimeout(10000);
            tls.startHandshake();                                           // TLS_WRAP_ASYNC(conn, SERVER, cert)
        } catch (IOException e) {
            log("Bắt tay TLS lỗi " + addr + ": " + e.getMessage());
            try { raw.close(); } catch (IOException ignored) { }
            return;
        }
        log("Bắt tay TLS thành công " + addr + ": " + tls.getSession().getProtocol() + ", " + tls.getSession().getCipherSuite());
        chatSession(tls, addr);                                             // tls thay cho conn
    }

    public static void main(String[] args) throws Exception {
        String host = args.length > 0 ? args[0] : Objects.requireNonNullElse(System.getenv("APP_HOST"), "0.0.0.0");
        int port = args.length > 1 ? Integer.parseInt(args[1]) : Integer.parseInt(Objects.requireNonNullElse(System.getenv("APP_PORT"), "9200"));
        // LOAD_CERT_AND_KEY(ENV("TLS_CERT"), SECRET("TLS_KEY_PASS")): nạp một lần khi khởi động
        char[] pass = Jwt.secret("TLS_KEY_PASS").toCharArray();
        KeyStore ks = KeyStore.getInstance("PKCS12");
        try (FileInputStream f = new FileInputStream(Jwt.secret("TLS_CERT"))) { ks.load(f, pass); }
        KeyManagerFactory kmf = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm());
        kmf.init(ks, pass);
        SSLContext ctx = SSLContext.getInstance("TLS");
        ctx.init(kmf.getKeyManagers(), null, null);
        try (ServerSocket srv = new ServerSocket()) {
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 16);
            log("Đang lắng nghe (TLS ≥ 1.2) tại " + host + ":" + port);
            while (true) {
                Socket raw = srv.accept();
                Thread.ofVirtual().start(() -> tlsSession(ctx, raw));      // mỗi kết nối một luồng ảo
            }
        }
    }
}
