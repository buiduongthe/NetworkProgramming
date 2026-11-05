import java.io.IOException;
import java.io.InputStream;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR3.3 – Server trò chơi đoán số nhiều người chơi trên TCP.
 *
 * Server chọn một số bí mật; người chơi lần lượt đoán, server trả lời "thấp quá", "cao quá" hoặc "đúng".
 * Thiết kế:
 *  - Mỗi kết nối có một luồng đọc (reader) chỉ làm một việc: nhận khung JSON và đẩy sự kiện vào hàng đợi.
 *  - MỘT luồng trò chơi (vòng lặp sự kiện) giữ toàn bộ trạng thái, xử lý tuần tự từng sự kiện,
 *    nên không cần khóa (lock). Hết giờ mỗi lượt = thời gian chờ của BlockingQueue.poll().
 *  - Máy trạng thái của server: CHO (chờ đủ người) -> DANG_CHOI -> (kết thúc ván) -> CHO.
 *  - Server là bên quyết định: kiểm tra mọi thông điệp (đúng lượt, kiểu số, khoảng hợp lệ), không tin client.
 *
 * Chạy: java -cp out ServerDoanSo [host] [port] [--so-nguoi 2] [--het-gio-ms 30000]
 *                                 [--seed 0] [--min 1] [--max 100] [--so-van 0]
 *       (--seed 0: lấy hạt giống theo thời gian; --so-van 0: chơi mãi)
 */
public class ServerDoanSo {
    static final String CHO = "CHO", DANG_CHOI = "DANG_CHOI";       // hai trạng thái của server
    static final Map<String, String> HINT_TEXT = Map.of("THAP", "thấp quá", "CAO", "cao quá", "DUNG", "ĐÚNG");
    static final Map<String, String> ERRORS = new HashMap<>();
    static {
        ERRORS.put("BAD_JSON", "Khung không phải đối tượng JSON");
        ERRORS.put("NEED_HELLO", "Phải gửi hello trước");
        ERRORS.put("BAD_NAME", "Tên phải là chuỗi 1–16 ký tự");
        ERRORS.put("NAME_TAKEN", "Tên đã có người dùng");
        ERRORS.put("GAME_IN_PROGRESS", "Ván đang diễn ra, hãy vào lại sau");
        ERRORS.put("GAME_NOT_STARTED", "Ván chưa bắt đầu");
        ERRORS.put("NOT_YOUR_TURN", "Chưa đến lượt bạn");
        ERRORS.put("BAD_VALUE", "Giá trị đoán phải là số nguyên");
        ERRORS.put("OUT_OF_RANGE", "Số đoán nằm ngoài khoảng cho phép");
        ERRORS.put("UNKNOWN_TYPE", "Loại thông điệp không hỗ trợ");
    }

    /** Sự kiện gửi cho vòng trò chơi: kind = open | msg | closed. */
    record Event(String kind, Socket conn, Object data) {}

    static int soNguoi = 2, hetGioMs = 30000, min = 1, max = 100, soVan = 0;
    static long seed = 0;
    static volatile boolean normalExit = false;

    static void log(String s) { System.out.println(s); }

    /** Luồng đọc của một kết nối: chuyển mỗi khung thành sự kiện ("msg", conn, obj) cho vòng trò chơi. */
    static void reader(Socket conn, BlockingQueue<Event> events) {
        try {
            InputStream in = conn.getInputStream();
            while (true) {
                Map<String, Object> obj = GiaoThuc.recvJson(in);
                events.add(new Event("msg", conn, obj));     // obj = null nghĩa là khung hỏng
            }
        } catch (IOException e) {                            // ConnectionClosed, ProtocolError, lỗi socket
            events.add(new Event("closed", conn, e.getMessage()));
        }
    }

    /** Luồng chấp nhận kết nối: mỗi client mới có một luồng đọc riêng. */
    static void acceptor(ServerSocket srv, BlockingQueue<Event> events) {
        while (true) {
            Socket conn;
            try {
                conn = srv.accept();
                conn.setTcpNoDelay(true);                    // tắt Nagle: phản hồi nhỏ đi ngay
            } catch (IOException e) {
                return;                                      // socket nghe đã đóng
            }
            events.add(new Event("open", conn, conn.getInetAddress().getHostAddress() + ":" + conn.getPort()));
            Thread t = new Thread(() -> reader(conn, events));
            t.setDaemon(true);
            t.start();
        }
    }

    /** Toàn bộ trạng thái trò chơi; chỉ luồng trò chơi được đọc/ghi. */
    static final class Game {
        final GiaoThuc.Lcg rng = new GiaoThuc.Lcg(seed != 0 ? seed : System.currentTimeMillis() / 1000);
        String state = CHO;
        final Map<Socket, String> names = new LinkedHashMap<>();   // conn -> tên (giữ thứ tự vào phòng)
        List<Socket> order = new ArrayList<>();                    // thứ tự lượt
        int turn = 0, turnNo = 0;                                  // chỉ số người đang có lượt; số lượt
        Long deadline = null;                                      // hạn chót của lượt hiện tại (System.nanoTime)
        int secret = 0, lo = min, hi = max, round = 0;

        // ----- gửi -----
        void send(Socket conn, Map<String, Object> obj) {
            try {
                GiaoThuc.sendJson(conn, obj);
            } catch (IOException ignored) { }                     // client đã mất: luồng đọc sẽ báo "closed"
        }

        void broadcast(Map<String, Object> obj) {
            for (Socket c : new ArrayList<>(names.keySet())) send(c, obj);
        }

        void error(Socket conn, String code, String who, String extra) {
            log("[server] Từ chối " + who + ": " + code + extra);
            send(conn, Json.obj("type", "error", "code", code, "msg", ERRORS.get(code)));
        }

        /** Đóng hẳn kết nối (luồng đọc sẽ thoát). */
        void drop(Socket conn) {
            names.remove(conn);
            try { conn.close(); } catch (IOException ignored) { }
        }

        // ----- máy trạng thái -----
        void startRound() {
            round++;
            state = DANG_CHOI;
            secret = rng.nextInt(min, max);
            lo = min;
            hi = max;
            order = new ArrayList<>(names.keySet());
            turn = 0;
            turnNo = 0;
            List<Object> orderNames = new ArrayList<>();
            for (Socket c : order) orderNames.add(names.get(c));
            log("[server] Bắt đầu ván " + round + ": số bí mật = " + secret + " (chỉ server biết), thứ tự: "
                    + String.join(", ", orderNames.stream().map(Object::toString).toList()));
            broadcast(Json.obj("type", "start", "round", round, "order", orderNames, "min", min, "max", max));
            nextTurn();
        }

        /** Giao lượt cho người kế tiếp và đặt hạn chót. */
        void nextTurn() {
            turnNo++;
            String name = names.get(order.get(turn));
            deadline = System.nanoTime() + hetGioMs * 1_000_000L;
            broadcast(Json.obj("type", "turn", "player", name, "lo", lo, "hi", hi, "turn_no", turnNo, "timeout_ms", hetGioMs));
        }

        void advance() {
            turn = (turn + 1) % order.size();
            nextTurn();
        }

        void endRound(String winner) {
            if (winner != null) log("[server] Kết thúc ván " + round + ": " + winner + " thắng sau " + turnNo + " lượt");
            else log("[server] Kết thúc ván " + round + ": không còn người chơi");
            broadcast(Json.obj("type", "game_over", "winner", winner, "secret", secret, "turns", turnNo));
            for (Socket c : new ArrayList<>(names.keySet())) {      // mỗi ván một phiên: đóng mọi kết nối
                send(c, Json.obj("type", "bye"));
                drop(c);
            }
            state = CHO;
            order = new ArrayList<>();
            deadline = null;
        }

        void removePlayer(Socket conn) {
            String name = names.remove(conn);
            log("[server] " + name + " rời trò chơi (còn " + names.size() + " người)");
            broadcast(Json.obj("type", "left", "player", name));
            if (!state.equals(DANG_CHOI)) return;
            int i = order.indexOf(conn);
            boolean wasCurrent = i == turn;
            order.remove(i);
            if (order.isEmpty()) {
                endRound(null);
                return;
            }
            if (i < turn) turn--;                                 // giữ nguyên người đang có lượt
            if (wasCurrent) {
                turn %= order.size();
                nextTurn();
            }
        }

        void onTimeout() {
            String name = names.get(order.get(turn));
            log("[server] " + name + " hết giờ ở lượt " + turnNo);
            broadcast(Json.obj("type", "timeout", "player", name));
            advance();
        }

        // ----- xử lý thông điệp -----
        void onMessage(Socket conn, String addr, Map<String, Object> msg) {
            String who = names.getOrDefault(conn, addr);
            if (msg == null) {
                error(conn, "BAD_JSON", who, "");
                return;
            }
            Object kind = msg.get("type");
            if (!names.containsKey(conn)) {                       // chưa vào phòng: chỉ chấp nhận hello
                if (!"hello".equals(kind)) error(conn, "NEED_HELLO", who, "");
                else onHello(conn, addr, msg);
                return;
            }
            if ("guess".equals(kind)) {
                onGuess(conn, who, msg.get("value"));
                return;
            }
            if ("bye".equals(kind)) {
                removePlayer(conn);
                drop(conn);
                return;
            }
            error(conn, "UNKNOWN_TYPE", who, " (" + GiaoThuc.repr(kind) + ")");
        }

        void onHello(Socket conn, String addr, Map<String, Object> msg) {
            Object n = msg.get("name");
            if (!(n instanceof String raw) || raw.strip().codePointCount(0, raw.strip().length()) < 1
                    || raw.strip().codePointCount(0, raw.strip().length()) > 16) {
                error(conn, "BAD_NAME", addr, "");
                return;
            }
            String name = raw.strip();
            if (names.containsValue(name)) {
                error(conn, "NAME_TAKEN", addr, " (" + name + ")");
                return;
            }
            if (state.equals(DANG_CHOI)) {
                error(conn, "GAME_IN_PROGRESS", name, "");
                send(conn, Json.obj("type", "bye"));
                drop(conn);
                return;
            }
            names.put(conn, name);
            log("[server] " + name + " vào phòng chờ (" + names.size() + "/" + soNguoi + ")");
            send(conn, Json.obj("type", "welcome", "name", name, "waiting", names.size(), "need", soNguoi, "min", min, "max", max));
            if (names.size() >= soNguoi) startRound();
        }

        void onGuess(Socket conn, String who, Object value) {
            if (!state.equals(DANG_CHOI)) {
                error(conn, "GAME_NOT_STARTED", who, "");
                return;
            }
            if (order.get(turn) != conn) {
                error(conn, "NOT_YOUR_TURN", who, "");
                return;
            }
            if (!(value instanceof Long lv)) {                    // Json: số nguyên -> Long; true/false, 1.5, "50" không phải
                error(conn, "BAD_VALUE", who, " (" + GiaoThuc.repr(value) + ")");
                return;
            }
            if (lv < lo || lv > hi) {
                error(conn, "OUT_OF_RANGE", who, " (" + lv + " ngoài " + lo + "–" + hi + ")");
                return;
            }
            int v = lv.intValue();
            String hint = v == secret ? "DUNG" : (v < secret ? "THAP" : "CAO");
            log("[server] Lượt " + turnNo + ": " + who + " đoán " + v + " -> " + HINT_TEXT.get(hint));
            broadcast(Json.obj("type", "result", "player", who, "value", v, "hint", hint));
            if (hint.equals("DUNG")) {
                endRound(who);
                return;
            }
            if (hint.equals("THAP")) lo = v + 1;
            else hi = v - 1;
            advance();
        }
    }

    public static void main(String[] args) throws Exception {
        String host = "0.0.0.0";
        int port = 9000;
        List<String> pos = new ArrayList<>();
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--so-nguoi" -> soNguoi = Integer.parseInt(args[++i]);
                case "--het-gio-ms" -> hetGioMs = Integer.parseInt(args[++i]);
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--min" -> min = Integer.parseInt(args[++i]);
                case "--max" -> max = Integer.parseInt(args[++i]);
                case "--so-van" -> soVan = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        if (pos.size() > 0) host = pos.get(0);
        if (pos.size() > 1) port = Integer.parseInt(pos.get(1));

        ServerSocket srv = new ServerSocket();
        srv.setReuseAddress(true);
        srv.bind(new InetSocketAddress(host, port), 16);
        log("[server] Trò chơi đoán số tại " + host + ":" + port + "; cần " + soNguoi + " người; mỗi lượt tối đa "
                + hetGioMs + " ms");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> { if (!normalExit) log("[server] Dừng server"); }));
        BlockingQueue<Event> events = new LinkedBlockingQueue<>();
        Thread acc = new Thread(() -> acceptor(srv, events));
        acc.setDaemon(true);
        acc.start();
        Game game = new Game();
        Map<Socket, String> addrs = new HashMap<>();               // conn -> "ip:port"
        while (true) {
            if (game.deadline != null && System.nanoTime() >= game.deadline)
                game.onTimeout();                                  // kiểm tra trước: thông điệp đến liên tục không "hoãn" được hết giờ
            Event ev;
            if (game.deadline != null) {
                long waitNs = Math.max(0, game.deadline - System.nanoTime());
                ev = events.poll(waitNs, TimeUnit.NANOSECONDS);
                if (ev == null) {                                  // hết giờ của lượt hiện tại
                    game.onTimeout();
                    continue;
                }
            } else {
                ev = events.take();
            }
            switch (ev.kind()) {
                case "open" -> {
                    addrs.put(ev.conn(), (String) ev.data());
                    log("[server] " + ev.data() + " kết nối");
                }
                case "msg" -> {
                    @SuppressWarnings("unchecked")
                    Map<String, Object> m = (Map<String, Object>) ev.data();
                    game.onMessage(ev.conn(), addrs.getOrDefault(ev.conn(), "?"), m);
                }
                default -> {                                       // closed
                    addrs.remove(ev.conn());
                    if (game.names.containsKey(ev.conn())) game.removePlayer(ev.conn());   // mất kết nối giữa chừng
                    try { ev.conn().close(); } catch (IOException ignored) { }
                }
            }
            if (soVan > 0 && game.round >= soVan && game.state.equals(CHO)) {
                log("[server] Đã chơi " + game.round + " ván, dừng server");
                break;
            }
        }
        normalExit = true;
        srv.close();
        System.exit(0);
    }
}
