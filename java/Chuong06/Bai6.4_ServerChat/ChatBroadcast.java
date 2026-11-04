import java.io.DataInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.Socket;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;

/**
 * Bài 6.4 – Server chat: phát tới phòng, gửi riêng và tác vụ ghi (Mã giả 6.6, Mục 6.3.3), cùng phần
 * cấu trúc dữ liệu của Mã giả 6.5 và các thủ tục mã giả chỉ nêu tên: REGISTER, UNREGISTER, JOIN_ROOM, LEAVE_ROOM.
 */
final class ChatBroadcast {
    private ChatBroadcast() {}

    /** STRUCT Client { name, sock, outbox ← BOUNDED_QUEUE(256) }. */
    static final class Client {
        final String name;
        final Socket sock;
        final OutputStream out;
        final DataInputStream in;
        final BlockingQueue<Map<String, Object>> outbox = new ArrayBlockingQueue<>(256);
        long winStart;                                                  // đếm tin trong cửa sổ 1 giây (lỗi 429)
        int winCount;

        Client(String name, Socket sock) throws IOException {
            this.name = name;
            this.sock = sock;
            this.out = sock.getOutputStream();
            this.in = new DataInputStream(sock.getInputStream());
        }

        /** outbox.close(): bỏ tin còn lại rồi đặt dấu kết thúc để tác vụ ghi dừng. */
        void closeOutbox() {
            outbox.clear();
            outbox.offer(END);
        }
    }

    static final Map<String, Object> END = Map.of();                    // dấu kết thúc hàng đợi
    static final Map<String, Client> USERS = new HashMap<>();           // tên → client
    static final Map<String, Set<String>> ROOMS = new HashMap<>();      // phòng → tập tên
    static final Object LOCK = new Object();                            // LOCK()
    static final Map<Integer, String> ERRORS = Map.of(400, "Sai cú pháp", 403, "Chưa tham gia phòng",
            404, "Không có người nhận", 409, "Tên đã được sử dụng", 429, "Gửi quá nhanh");
    static final DateTimeFormatter TS =
            DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC);

    static Map<String, Object> error(int code) { return Json.obj("type", "error", "code", code, "text", ERRORS.get(code)); }

    static String now() { return TS.format(Instant.now()); }           // NOW(): ISO 8601 UTC

    static Client register(String name, Socket sock) throws IOException {
        synchronized (LOCK) {
            if (USERS.containsKey(name)) return null;                   // trùng tên
            Client c = new Client(name, sock);
            USERS.put(name, c);
            return c;
        }
    }

    static void unregister(Client c) {
        List<String> left = new ArrayList<>();
        synchronized (LOCK) {
            USERS.remove(c.name);
            for (var e : ROOMS.entrySet()) if (e.getValue().remove(c.name)) left.add(e.getKey());
            ROOMS.values().removeIf(Set::isEmpty);
        }
        for (String r : left)                                           // phát ngoài khóa
            broadcast(r, Json.obj("type", "sys", "text", c.name + " đã rời phòng " + r), null);
    }

    static void joinRoom(Client c, String room) {
        synchronized (LOCK) {
            ROOMS.computeIfAbsent(room, k -> new HashSet<>()).add(c.name);
        }
        System.out.println("[server] " + c.name + " vào phòng " + room);
        broadcast(room, Json.obj("type", "sys", "text", c.name + " đã vào phòng " + room), null);
    }

    static void leaveRoom(Client c, String room) {
        boolean member;
        synchronized (LOCK) {
            Set<String> m = ROOMS.get(room);
            member = m != null && m.remove(c.name);
            if (member && m.isEmpty()) ROOMS.remove(room);
        }
        if (!member) { enqueue(c, error(403)); return; }
        System.out.println("[server] " + c.name + " rời phòng " + room);
        broadcast(room, Json.obj("type", "sys", "text", c.name + " đã rời phòng " + room), null);
    }

    /** BROADCAST (Mã giả 6.6, dòng 1–10); sender null: tin hệ thống. */
    static void broadcast(String room, Map<String, Object> msg, Client sender) {
        Set<String> targets = new HashSet<>();                          // phòng chưa tồn tại
        synchronized (LOCK) {                                           // giữ khóa ngắn
            if (ROOMS.containsKey(room)) targets = new HashSet<>(ROOMS.get(room));   // COPY
        }
        if (sender != null && !targets.contains(sender.name)) {
            enqueue(sender, error(403));
            return;
        }
        for (String name : targets) sendTo(name, msg, null);
    }

    /** SEND_TO (Mã giả 6.6, dòng 12–18). */
    static void sendTo(String name, Map<String, Object> msg, Client sender) {
        Client c;
        synchronized (LOCK) { c = USERS.get(name); }
        if (c != null) enqueue(c, msg);
        else if (sender != null) enqueue(sender, error(404));
    }

    /** ENQUEUE (Mã giả 6.6, dòng 20–24): offer không bao giờ chờ. */
    static void enqueue(Client c, Map<String, Object> msg) {
        if (!c.outbox.offer(msg)) {                                     // hàng đợi đầy
            System.out.println("[server] Ngắt client chậm " + c.name);
            try { c.sock.close(); } catch (IOException ignored) { }     // CLOSE(c.sock)
        }
    }

    /** WRITER (Mã giả 6.6, dòng 26–34): tác vụ ghi duy nhất của mỗi socket, chạy trên luồng ảo. */
    static void writer(Client c) {
        try {
            Map<String, Object> msg;
            while ((msg = c.outbox.take()) != END) NetUtil.sendJson(c.out, msg);
        } catch (IOException e) {                                       // NetError
            try { c.sock.close(); } catch (IOException ignored) { }     // phiên sẽ kết thúc theo
        } catch (InterruptedException ignored) {
            // dừng tác vụ ghi
        }
    }
}
