import java.io.DataInputStream;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.ArrayList;
import java.util.Map;
import java.util.TreeSet;
import java.util.regex.Pattern;

/**
 * Bài 6.4 – Server chat nhiều phòng (Mã giả 6.5, Mục 6.3.3): vòng chấp nhận kết nối kế thừa Mã giả 6.3
 * (mỗi kết nối một luồng ảo), tác vụ phiên CHAT_SESSION. Giao thức: Bảng đặc tả giao thức chat (Mục 6.3.1).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out ChatServer [host] [port]      (mặc định 0.0.0.0 9200)
 */
public class ChatServer {
    static final Pattern NAME_RE = Pattern.compile("^[\\w-]{1,32}$", Pattern.UNICODE_CHARACTER_CLASS);
    static final int RATE_LIMIT = 10;                                   // tối đa 10 tin msg/pm mỗi giây

    static final class HelloError extends Exception {
        final int code;
        HelloError(int code) { super("Lỗi " + code); this.code = code; }
    }

    static boolean valid(Object s) { return s instanceof String str && NAME_RE.matcher(str).matches(); }

    static boolean rateOk(ChatBroadcast.Client c) {
        long now = System.currentTimeMillis();
        if (now - c.winStart >= 1000) { c.winStart = now; c.winCount = 0; }
        return ++c.winCount <= RATE_LIMIT;
    }

    public static void main(String[] args) throws IOException {
        Object[] ep = NetUtil.parseEndpoint(args, "0.0.0.0", 9200);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        try (ServerSocket srv = new ServerSocket()) {                  // như Mã giả 6.3
            srv.setReuseAddress(true);
            srv.bind(new InetSocketAddress(host, port), 128);
            System.out.println("[server] Server chat đang lắng nghe tại " + host + ":" + port);
            while (true) {
                Socket conn = srv.accept();                             // ACCEPT_ASYNC
                Thread.ofVirtual().start(() -> chatSession(conn, NetUtil.addr(conn)));   // RUN_BACKGROUND
            }
        }
    }

    /** CHAT_SESSION (Mã giả 6.5, dòng 4–33). */
    static void chatSession(Socket conn, String addr) {
        ChatBroadcast.Client c = null;                                  // chưa đăng ký tên
        try {
            conn.setSoTimeout(10000);                                   // 10 giây cho hello
            Map<String, Object> h = NetUtil.recvJson(new DataInputStream(conn.getInputStream()));
            if (!"hello".equals(h.get("type")) || !valid(h.get("name"))) throw new HelloError(400);
            c = ChatBroadcast.register((String) h.get("name"), conn);   // null nếu trùng tên
            if (c == null) throw new HelloError(409);
            System.out.println("[server] " + c.name + " đăng ký từ " + addr);
            ChatBroadcast.Client me = c;
            Thread.ofVirtual().start(() -> ChatBroadcast.writer(me));   // RUN_BACKGROUND(WRITER(c))
            Map<String, Object> welcome;
            synchronized (ChatBroadcast.LOCK) {
                welcome = Json.obj("type", "welcome", "users", new ArrayList<>(new TreeSet<>(ChatBroadcast.USERS.keySet())),
                        "rooms", new ArrayList<>(new TreeSet<>(ChatBroadcast.ROOMS.keySet())));
            }
            ChatBroadcast.enqueue(c, welcome);
            conn.setSoTimeout(90000);                                   // 3 chu kỳ nhịp tim
            while (true) {
                Map<String, Object> m = NetUtil.recvJson(c.in);
                Object t = m.get("type"), room = m.get("room"), text = m.get("text"), to = m.get("to");
                if (("msg".equals(t) || "pm".equals(t)) && !rateOk(c))
                    ChatBroadcast.enqueue(c, ChatBroadcast.error(429));
                else if ("join".equals(t) && valid(room))
                    ChatBroadcast.joinRoom(c, (String) room);
                else if ("leave".equals(t) && room instanceof String r)
                    ChatBroadcast.leaveRoom(c, r);
                else if ("msg".equals(t) && room instanceof String r && text instanceof String x)
                    ChatBroadcast.broadcast(r, Json.obj("type", "msg", "from", c.name, "room", r,
                            "text", x, "ts", ChatBroadcast.now()), c);
                else if ("pm".equals(t) && to instanceof String r && text instanceof String x)
                    ChatBroadcast.sendTo(r, Json.obj("type", "pm", "from", c.name, "text", x,
                            "ts", ChatBroadcast.now()), c);
                else if ("ping".equals(t))
                    ChatBroadcast.enqueue(c, Json.obj("type", "pong"));
                else                                                    // OTHERWISE
                    ChatBroadcast.enqueue(c, ChatBroadcast.error(400));
            }
        } catch (HelloError e) {                                        // lời chào không hợp lệ
            System.out.println("[server] Từ chối " + addr + ": lỗi " + e.code);
            try {
                NetUtil.sendJson(conn.getOutputStream(), ChatBroadcast.error(e.code));
            } catch (IOException ignored) { }
        } catch (IOException | Json.JsonError e) {
            // kết thúc phiên bình thường hoặc do lỗi (ConnectionClosed, Timeout, NetError, ProtocolError)
        } finally {
            if (c != null) {
                ChatBroadcast.unregister(c);
                c.closeOutbox();
                System.out.println("[server] " + c.name + " rời server");
            }
            try { conn.close(); } catch (IOException ignored) { }       // CLOSE(conn)
        }
    }
}
