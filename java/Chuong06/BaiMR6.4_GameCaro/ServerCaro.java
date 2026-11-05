import java.io.IOException;
import java.io.InputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.4 – Server Caro nhiều phòng trên TCP, khám phá trong LAN bằng UDP broadcast.
 *
 * Thiết kế (mở rộng từ MR3.3):
 *  - Luồng chấp nhận + một luồng đọc cho mỗi kết nối: chỉ nhận khung JSON, đẩy sự kiện vào hàng đợi.
 *  - Luồng khám phá (RESPONDER của Bài 6.5): nhận yêu cầu discover trên UDP, đẩy sự kiện vào hàng đợi.
 *  - MỘT luồng trò chơi giữ toàn bộ trạng thái (sảnh, các phòng, bàn cờ), xử lý tuần tự từng sự kiện.
 *    Hạn chót của nước đi trong mọi phòng được gom lại: chờ hàng đợi đến hạn chót sớm nhất.
 *  - Server là trọng tài: kiểm tra lượt, tọa độ, ô trống, phát hiện thắng; client chỉ hiển thị.
 *  - Mỗi nước đi hợp lệ được phát (broadcast) tới cả hai người chơi và mọi người xem trong phòng.
 * Trạng thái của một phòng: CHO (thiếu người) -> DANG_CHOI -> KET_THUC -(cả hai xin chơi lại)-> DANG_CHOI.
 *
 * Chạy: java -cp out ServerCaro [host] [port] [--kham-pha 9999] [--ten Caro] [--het-gio-ms 30000]
 */
public class ServerCaro {
    static final String ROOM_WAITING = "CHO", ROOM_PLAYING = "DANG_CHOI", ROOM_FINISHED = "KET_THUC";
    static final Map<String, String> REASON_TEXT = Map.of("five", "5 quân liên tiếp", "timeout", "đối thủ hết giờ",
            "left", "đối thủ rời phòng", "draw", "hòa (kín bàn)");
    static final Map<String, String> ERROR_TEXT = new HashMap<>();
    static {
        String[][] e = {{"BAD_JSON", "Khung không phải đối tượng JSON"}, {"NEED_HELLO", "Phải gửi hello trước"},
            {"BAD_NAME", "Tên phải là chuỗi 1–16 ký tự"}, {"NAME_TAKEN", "Tên đã có người dùng"},
            {"UNKNOWN_TYPE", "Loại thông điệp không hỗ trợ"}, {"ALREADY_IN_ROOM", "Bạn đang ở trong một phòng"},
            {"BAD_ROOM_NAME", "Tên phòng phải là chuỗi 1–16 ký tự"}, {"ROOM_EXISTS", "Phòng đã tồn tại"},
            {"NO_SUCH_ROOM", "Không có phòng này"}, {"ROOM_FULL", "Phòng đã đủ hai người chơi"},
            {"NOT_IN_ROOM", "Bạn chưa vào phòng nào"}, {"NOT_PLAYER", "Người xem không được đánh"},
            {"GAME_NOT_RUNNING", "Ván chưa bắt đầu hoặc đã kết thúc"}, {"NOT_YOUR_TURN", "Chưa đến lượt bạn"},
            {"BAD_MOVE", "Nước đi phải có r, c là số nguyên"}, {"OUT_OF_BOARD", "Ô nằm ngoài bàn cờ"},
            {"CELL_TAKEN", "Ô đã có quân"}};
        for (String[] x : e) ERROR_TEXT.put(x[0], x[1]);
    }

    /** Sự kiện gửi tới luồng trò chơi: open, msg, closed, discover. */
    record Event(String kind, Socket conn, String addr, SocketAddress udp, Map<String, Object> msg) {}

    // ---------------------------------------------------------------- các luồng nhận
    static void reader(Socket conn, BlockingQueue<Event> events) {
        try {
            InputStream in = conn.getInputStream();
            while (true) {
                byte[] p = GiaoThuc.recvFrame(in);
                events.add(new Event("msg", conn, null, null, GiaoThuc.decodeJSON(p, p.length)));
            }
        } catch (IOException e) {
            events.add(new Event("closed", conn, null, null, null));
        }
    }

    static void acceptor(ServerSocket ln, BlockingQueue<Event> events) {
        while (true) {
            Socket conn;
            try {
                conn = ln.accept();
                conn.setTcpNoDelay(true);                  // tắt Nagle: nước đi là khung nhỏ, gửi ngay
            } catch (IOException e) {
                return;
            }
            events.add(new Event("open", conn, conn.getInetAddress().getHostAddress() + ":" + conn.getPort(), null, null));
            Thread.ofVirtual().start(() -> reader(conn, events));
        }
    }

    /** RESPONDER (Mã giả 6.7): nhận discover, để luồng trò chơi trả lời (nó mới biết số phòng). */
    static void responder(DatagramSocket udp, BlockingQueue<Event> events) {
        byte[] buf = new byte[2048];
        while (true) {
            DatagramPacket p = new DatagramPacket(buf, buf.length);
            try {
                udp.receive(p);
            } catch (IOException e) {
                return;
            }
            Map<String, Object> q = GiaoThuc.decodeJSON(p.getData(), p.getLength());
            if (q != null && GiaoThuc.str(q, "type").equals("discover") && GiaoThuc.str(q, "service").equals(GiaoThuc.DISC_SERVICE))
                events.add(new Event("discover", null, null, p.getSocketAddress(), q));
        }
    }

    // ---------------------------------------------------------------- trạng thái
    static final class Client {
        final Socket conn;
        final String addr;
        String name = "";
        String room = "";                                 // tên phòng đang ở ("" = sảnh)
        String role = "";                                 // "X", "O" hoặc "xem"
        Client(Socket conn, String addr) { this.conn = conn; this.addr = addr; }
    }

    static final class Room {
        final String name;
        final Map<String, Client> seats = new HashMap<>(Map.of());   // hai người chơi
        final List<Client> watchers = new ArrayList<>();
        BanCo board = new BanCo();
        String state = ROOM_WAITING, turn = "X";
        int moves, gameNo;
        Long deadline = null;                             // null = không có hạn chót (System.nanoTime)
        Map<String, Boolean> rematch = new HashMap<>();

        Room(String name) { this.name = name; }

        List<Client> members() {
            List<Client> list = new ArrayList<>();
            for (String m : new String[] {"X", "O"}) if (seats.get(m) != null) list.add(seats.get(m));
            list.addAll(watchers);
            return list;
        }

        Map<String, Object> summary() {
            return Json.obj("room", name, "x", nameOrNull(seats.get("X")), "o", nameOrNull(seats.get("O")),
                    "watchers", watchers.size(), "state", state);
        }
    }

    static Object nameOrNull(Client c) { return c == null ? null : c.name; }
    static String other(String mark) { return mark.equals("X") ? "O" : "X"; }

    final String name;
    final int port, timeoutMs;
    final DatagramSocket udp;
    final Map<Socket, Client> clients = new LinkedHashMap<>();
    final Map<String, Room> rooms = new LinkedHashMap<>();   // giữ thứ tự tạo phòng

    ServerCaro(String name, int port, int timeoutMs, DatagramSocket udp) {
        this.name = name; this.port = port; this.timeoutMs = timeoutMs; this.udp = udp;
    }

    static void log(String s) { System.out.println("[server] " + s); }

    // ----- gửi -----
    void send(Client c, Map<String, Object> obj) { GiaoThuc.sendJSON(c.conn, obj); }

    void toRoom(Room r, Map<String, Object> obj) { for (Client c : r.members()) send(c, obj); }

    void fail(Client c, String code, String extra) {
        log("Từ chối " + (c.name.isEmpty() ? c.addr : c.name) + ": " + code + extra);
        send(c, Json.obj("type", "error", "code", code, "msg", ERROR_TEXT.get(code)));
    }

    List<Object> roomList() {
        List<Object> list = new ArrayList<>();
        for (Room r : rooms.values()) list.add(r.summary());
        return list;
    }

    // ----- ván cờ -----
    long newDeadline() { return System.nanoTime() + timeoutMs * 1_000_000L; }

    void startGame(Room r) {
        r.gameNo++;
        r.board = new BanCo();
        r.turn = "X";
        r.moves = 0;
        r.state = ROOM_PLAYING;
        r.rematch = new HashMap<>();
        r.deadline = newDeadline();
        String x = r.seats.get("X").name, o = r.seats.get("O").name;
        log("Phòng " + r.name + ", ván " + r.gameNo + ": X = " + x + ", O = " + o);
        toRoom(r, Json.obj("type", "start", "room", r.name, "game_no", r.gameNo, "x", x, "o", o, "size", BanCo.N, "timeout_ms", timeoutMs));
    }

    void finish(Room r, String winnerMark, String reason, List<int[]> line) {
        r.state = ROOM_FINISHED;
        r.deadline = null;
        Object winner = null, mark = null;
        String result = "hòa";
        if (winnerMark != null && r.seats.get(winnerMark) != null) {
            winner = r.seats.get(winnerMark).name;
            mark = winnerMark;
            result = winner + " (" + winnerMark + ") thắng";
        }
        log("Phòng " + r.name + ", ván " + r.gameNo + " kết thúc sau " + r.moves + " nước: " + result + " – " + REASON_TEXT.get(reason));
        List<Object> cells = new ArrayList<>();
        if (line != null) for (int[] p : line) cells.add(List.of(p[0], p[1]));
        toRoom(r, Json.obj("type", "over", "room", r.name, "game_no", r.gameNo, "winner", winner, "mark", mark,
                "reason", reason, "moves", r.moves, "line", cells));
    }

    void onMove(Client c, Map<String, Object> m) {
        Room r = rooms.get(c.room);
        if (r == null) { fail(c, "NOT_IN_ROOM", ""); return; }
        if (c.role.equals("xem")) { fail(c, "NOT_PLAYER", ""); return; }
        if (!r.state.equals(ROOM_PLAYING)) { fail(c, "GAME_NOT_RUNNING", ""); return; }
        if (!c.role.equals(r.turn)) { fail(c, "NOT_YOUR_TURN", ""); return; }
        if (!GiaoThuc.isInt(m.get("r")) || !GiaoThuc.isInt(m.get("c"))) { fail(c, "BAD_MOVE", ""); return; }
        long rowL = (Long) m.get("r"), colL = (Long) m.get("c");
        if (rowL < 0 || rowL >= BanCo.N || colL < 0 || colL >= BanCo.N) { fail(c, "OUT_OF_BOARD", " (" + rowL + ", " + colL + ")"); return; }
        int row = (int) rowL, col = (int) colL;
        if (r.board.o[row][col] != BanCo.EMPTY) { fail(c, "CELL_TAKEN", " (" + BanCo.cellName(row, col) + ")"); return; }
        r.board.o[row][col] = c.role.charAt(0);               // nước đi hợp lệ: ghi vào bàn cờ của server
        r.moves++;
        List<int[]> line = r.board.fiveLine(row, col);          // phán xử TRƯỚC khi báo nước đi
        boolean ended = line != null || r.moves == BanCo.N * BanCo.N;
        toRoom(r, Json.obj("type", "moved", "room", r.name, "r", row, "c", col, "mark", c.role, "by", c.name,
                "move_no", r.moves, "next", ended ? null : other(c.role)));   // next = null: ván đã hết
        if (line != null) { finish(r, c.role, "five", line); return; }
        if (ended) { finish(r, null, "draw", null); return; }
        r.turn = other(c.role);
        r.deadline = newDeadline();                             // mỗi nước có hạn chót riêng
    }

    void onTimeout(Room r) {
        log("Phòng " + r.name + ": " + r.seats.get(r.turn).name + " (" + r.turn + ") hết giờ");
        finish(r, other(r.turn), "timeout", null);
    }

    // ----- phòng -----
    void enter(Client c, Room r, String role) {
        c.room = r.name;
        c.role = role;
        String what = "quân " + role;
        if (role.equals("xem")) {
            r.watchers.add(c);
            what = "người xem";
        } else {
            r.seats.put(role, c);
        }
        log(c.name + " vào phòng " + r.name + " (" + what + ")");
        send(c, Json.obj("type", "joined", "room", r.name, "role", role, "info", r.summary()));
        if (role.equals("xem") && !r.state.equals(ROOM_WAITING)) {   // người xem đến muộn: gửi ảnh chụp bàn cờ
            List<Object> rows = new ArrayList<>();
            for (char[] row : r.board.o) rows.add(new String(row));
            send(c, Json.obj("type", "board", "room", r.name, "game_no", r.gameNo, "rows", rows, "moves", r.moves,
                    "turn", r.state.equals(ROOM_PLAYING) ? r.turn : null, "state", r.state));
        }
    }

    void leaveRoom(Client c) {
        Room r = rooms.get(c.room);
        if (r == null) return;
        String role = c.role;
        c.room = "";
        c.role = "";
        if (role.equals("xem")) {
            r.watchers.remove(c);
            return;
        }
        if (r.state.equals(ROOM_PLAYING)) finish(r, other(role), "left", null);   // bỏ dở ván: đối thủ thắng
        r.seats.put(role, null);
        log(c.name + " rời phòng " + r.name);
        toRoom(r, Json.obj("type", "left", "room", r.name, "player", c.name, "role", role));
        r.state = ROOM_WAITING;
        r.deadline = null;
        r.rematch = new HashMap<>();
        if (r.seats.get("X") == null && r.seats.get("O") == null) {
            log("Phòng " + r.name + " đóng (không còn người chơi)");
            for (Client w : r.watchers) {
                w.room = "";
                w.role = "";
                send(w, Json.obj("type", "room_closed", "room", r.name));
            }
            rooms.remove(r.name);
        }
    }

    // ----- xử lý thông điệp -----
    static String validName(Object v) {
        if (!(v instanceof String s)) return null;
        String name = s.strip();
        int n = name.codePointCount(0, name.length());
        return n >= 1 && n <= 16 ? name : null;
    }

    void onMessage(Client c, Map<String, Object> m) {
        if (m == null) { fail(c, "BAD_JSON", ""); return; }
        Object kind = m.get("type");
        if (c.name.isEmpty()) {
            if (!"hello".equals(kind)) { fail(c, "NEED_HELLO", ""); return; }
            String name = validName(m.get("name"));
            if (name == null) { fail(c, "BAD_NAME", ""); return; }
            for (Client o : clients.values())
                if (o.name.equals(name)) { fail(c, "NAME_TAKEN", " (" + name + ")"); return; }
            c.name = name;
            log(c.name + " vào sảnh (" + c.addr + ")");
            send(c, Json.obj("type", "welcome", "name", c.name, "rooms", roomList()));
            return;
        }
        if ("list".equals(kind)) send(c, Json.obj("type", "rooms", "rooms", roomList()));
        else if ("create".equals(kind) || "join".equals(kind) || "watch".equals(kind)) onRoomRequest(c, (String) kind, m.get("room"));
        else if ("move".equals(kind)) onMove(c, m);
        else if ("rematch".equals(kind)) onRematch(c);
        else if ("leave".equals(kind)) {
            if (c.room.isEmpty()) fail(c, "NOT_IN_ROOM", "");
            else leaveRoom(c);
        } else if ("bye".equals(kind)) {
            leaveRoom(c);
            log(c.name + " tạm biệt");
            c.name = "";                                     // không còn trong sảnh; "closed" đến sau sẽ im lặng
            try {                                            // đóng hai chiều: luồng đọc sẽ báo "closed"
                c.conn.shutdownInput();
                c.conn.shutdownOutput();
            } catch (IOException ignored) { }
        } else fail(c, "UNKNOWN_TYPE", " (" + GiaoThuc.repr(kind) + ")");
    }

    void onRoomRequest(Client c, String kind, Object v) {
        if (!c.room.isEmpty()) { fail(c, "ALREADY_IN_ROOM", ""); return; }
        String name = validName(v);
        if (name == null) { fail(c, "BAD_ROOM_NAME", ""); return; }
        Room r = rooms.get(name);
        if (kind.equals("create")) {
            if (r != null) { fail(c, "ROOM_EXISTS", " (" + name + ")"); return; }
            r = new Room(name);
            rooms.put(name, r);
            log(c.name + " tạo phòng " + name);
            enter(c, r, "X");
            return;
        }
        if (r == null) { fail(c, "NO_SUCH_ROOM", " (" + name + ")"); return; }
        if (kind.equals("watch")) { enter(c, r, "xem"); return; }
        String free = r.seats.get("X") == null ? "X" : r.seats.get("O") == null ? "O" : null;
        if (free == null) { fail(c, "ROOM_FULL", " (" + name + ")"); return; }
        enter(c, r, free);
        if (r.seats.get("X") != null && r.seats.get("O") != null) startGame(r);
    }

    void onRematch(Client c) {
        Room r = rooms.get(c.room);
        if (r == null || !(c.role.equals("X") || c.role.equals("O"))) { fail(c, "NOT_PLAYER", ""); return; }
        if (!r.state.equals(ROOM_FINISHED)) { fail(c, "GAME_NOT_RUNNING", ""); return; }
        r.rematch.put(c.role, true);
        if (r.rematch.size() < 2) {
            toRoom(r, Json.obj("type", "rematch_wait", "room", r.name, "by", c.name));
            return;
        }
        Client x = r.seats.get("X");                         // đổi màu quân: người kia đi trước
        r.seats.put("X", r.seats.get("O"));
        r.seats.put("O", x);
        for (String m : new String[] {"X", "O"}) r.seats.get(m).role = m;
        startGame(r);
    }

    void onDiscover(SocketAddress addr, Map<String, Object> q) {
        int players = 0;
        for (Client c : clients.values()) if (!c.name.isEmpty()) players++;
        byte[] reply = Json.stringify(Json.obj("id", q.get("id"), "name", name, "port", port, "rooms", rooms.size(), "players", players))
                .getBytes(StandardCharsets.UTF_8);
        InetSocketAddress a = (InetSocketAddress) addr;
        log("Trả lời discover từ " + a.getAddress().getHostAddress() + ":" + a.getPort());
        try {
            udp.send(new DatagramPacket(reply, reply.length, addr));
        } catch (IOException ignored) { }
    }

    void onClosed(Socket conn) {
        Client c = clients.remove(conn);
        if (c == null) return;
        if (!c.name.isEmpty()) {
            leaveRoom(c);
            log(c.name + " ngắt kết nối");
        }
        try { conn.close(); } catch (IOException ignored) { }
    }

    /** Phòng có hạn chót sớm nhất (hoặc null). */
    Room expiredRoom() {
        Room best = null;
        for (Room r : rooms.values()) if (r.deadline != null && (best == null || r.deadline < best.deadline)) best = r;
        return best;
    }

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        int disc = 9999, timeoutMs = 30000;
        String name = "Caro";
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--kham-pha" -> disc = Integer.parseInt(args[++i]);
                case "--ten" -> name = args[++i];
                case "--het-gio-ms" -> timeoutMs = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        String host = pos.size() > 0 ? pos.get(0) : "0.0.0.0";
        int port = pos.size() > 1 ? Integer.parseInt(pos.get(1)) : 9100;
        ServerSocket ln = new ServerSocket();
        ln.setReuseAddress(true);
        ln.bind(new InetSocketAddress(host, port), 16);
        DatagramSocket udp = new DatagramSocket(null);           // tạo chưa bind để kịp đặt SO_REUSEADDR
        udp.setReuseAddress(true);
        udp.bind(new InetSocketAddress("0.0.0.0", disc));         // 0.0.0.0: nhận được gói broadcast
        log(name + ": Caro " + BanCo.N + "×" + BanCo.N + " tại " + host + ":" + port + ", khám phá UDP cổng " + disc
                + ", mỗi nước tối đa " + timeoutMs + " ms");
        BlockingQueue<Event> events = new LinkedBlockingQueue<>();
        Thread.ofPlatform().daemon().start(() -> acceptor(ln, events));
        Thread.ofPlatform().daemon().start(() -> responder(udp, events));
        Runtime.getRuntime().addShutdownHook(new Thread(() -> log("Dừng server")));   // Ctrl+C
        ServerCaro s = new ServerCaro(name, port, timeoutMs, udp);
        while (true) {
            Room r = s.expiredRoom();
            Event ev;
            if (r != null) {
                long left = r.deadline - System.nanoTime();
                if (left <= 0) {                                   // có phòng hết giờ: xử lý trước
                    s.onTimeout(r);
                    continue;
                }
                ev = events.poll(left, TimeUnit.NANOSECONDS);
                if (ev == null) continue;                          // vòng sau sẽ thấy phòng hết giờ
            } else {
                ev = events.take();
            }
            switch (ev.kind()) {
                case "open" -> s.clients.put(ev.conn(), new Client(ev.conn(), ev.addr()));
                case "msg" -> {
                    Client c = s.clients.get(ev.conn());
                    if (c != null) s.onMessage(c, ev.msg());
                }
                case "closed" -> s.onClosed(ev.conn());
                default -> s.onDiscover(ev.udp(), ev.msg());
            }
        }
    }
}
