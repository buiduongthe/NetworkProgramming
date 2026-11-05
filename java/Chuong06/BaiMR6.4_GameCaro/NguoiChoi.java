import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Bài mở rộng MR6.4 – Client Caro: người chơi thật (dòng lệnh), bot, hoặc người xem tự động.
 *
 * Kết nối: chỉ định host, port; hoặc --tim CỔNG để tìm server trong LAN bằng UDP broadcast (Bài 6.5).
 *  Người chơi (không có --bot, --xem): gõ lệnh ds | tao <phòng> | vao <phòng> | xem <phòng> | h8 | lai | roi | thoat
 *  Bot (--bot tan_cong|phong_thu|can_bang|tap_su|im_lang, kèm --tao hoặc --vao); --thu-loi: gửi nước sai
 *  Người xem tự động (--xem <phòng>): in tiến trình và bàn cờ cuối mỗi ván, thoát khi phòng đóng.
 *
 * Chạy: java -cp out NguoiChoi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P]
 *                             [--bot KIỂU] [--seed 1] [--thu-loi] [--so-van 1] [--tre-ms 300]
 */
public class NguoiChoi {
    static int tim = 0, games = 1, delayMs = 300;
    static String create = "", join = "", watch = "", bot = "";
    static long seed = 1;
    static boolean tryErrors = false;

    static String overText(Map<String, Object> m) {
        String reason = ServerCaro.REASON_TEXT.getOrDefault(GiaoThuc.str(m, "reason"), "");
        String w = GiaoThuc.str(m, "winner");
        String result = w.isEmpty() ? reason : w + " (" + GiaoThuc.str(m, "mark") + ") thắng – " + reason;
        return "Ván " + GiaoThuc.num(m, "game_no") + " kết thúc sau " + GiaoThuc.num(m, "moves") + " nước: " + result;
    }

    @SuppressWarnings("unchecked")
    static String lineText(Map<String, Object> m) {
        List<String> parts = new ArrayList<>();
        if (m.get("line") instanceof List<?> cells)
            for (Object p : cells)
                if (p instanceof List<?> rc && rc.size() == 2 && rc.get(0) instanceof Long r && rc.get(1) instanceof Long c)
                    parts.add(BanCo.cellName(r.intValue(), c.intValue()));
        return String.join(" ", parts);
    }

    static String orNone(Map<String, Object> m, String key) {
        String v = GiaoThuc.str(m, key);
        return v.isEmpty() ? "chưa có" : v;
    }

    /** Bàn cờ phía client: chỉ là bản sao để hiển thị, được cập nhật từ thông điệp của server. */
    static final class View {
        BanCo b = new BanCo();
        int lastR = -1, lastC = -1;
        String mark = "", turn = "X";                     // turn = "" nghĩa là không ai có lượt (ván đã hết)

        void apply(Map<String, Object> m, String me) {
            switch (GiaoThuc.str(m, "type")) {
                case "start" -> {
                    b = new BanCo();
                    lastR = lastC = -1;
                    turn = "X";
                    mark = GiaoThuc.str(m, "x").equals(me) ? "X" : GiaoThuc.str(m, "o").equals(me) ? "O" : "";
                }
                case "board" -> {                         // ảnh chụp bàn cờ cho người xem đến muộn
                    if (m.get("rows") instanceof List<?> rows)
                        for (int r = 0; r < rows.size() && r < BanCo.N; r++)
                            if (rows.get(r) instanceof String text)
                                for (int c = 0; c < text.length() && c < BanCo.N; c++) b.o[r][c] = text.charAt(c);
                    turn = GiaoThuc.str(m, "turn");
                }
                case "moved" -> {
                    int r = GiaoThuc.num(m, "r"), c = GiaoThuc.num(m, "c");
                    if (BanCo.inside(r, c)) b.o[r][c] = GiaoThuc.str(m, "mark").charAt(0);
                    lastR = r;
                    lastC = c;
                    turn = GiaoThuc.str(m, "next");      // lượt kế do server quyết định ("" = null: ván đã hết)
                }
                case "over" -> turn = "";
                default -> { }
            }
        }
    }

    static Socket connect(String me, String host, int port) throws IOException {
        if (tim > 0) {
            System.out.println("[" + me + "] Tìm server Caro bằng broadcast tới cổng " + tim + "...");
            List<GiaoThuc.Found> list = GiaoThuc.discover(tim, 800, "255.255.255.255");
            if (list.isEmpty()) {
                System.out.println("[" + me + "] Không tìm thấy server nào");
                System.exit(1);
            }
            host = list.get(0).ip();
            port = list.get(0).port();
            System.out.println("[" + me + "] Thấy " + list.get(0).name() + " tại " + host + ":" + port + ", kết nối");
        }
        Socket s = new Socket();
        s.connect(new InetSocketAddress(host, port), 3000);
        s.setTcpNoDelay(true);
        if (tim == 0) System.out.println("[" + me + "] Đã kết nối " + host + ":" + port);
        return s;
    }

    // ---------------------------------------------------------------- bot
    static void quitRoom(Socket conn, String me, int n) {
        System.out.println("[" + me + "] Đã chơi " + n + " ván, rời phòng");
        GiaoThuc.sendJSON(conn, Json.obj("type", "bye"));    // bye = rời phòng + rời sảnh trong một thông điệp
    }

    /**
     * Bot phản ứng theo từng thông điệp của server. Sau mỗi ván, người THUA (hoặc quân O nếu hòa) lên tiếng trước:
     * xin chơi lại hoặc rời phòng; người thắng chờ quyết định đó rồi mới trả lời. Nhờ quy ước này hai bot không
     * "nói cùng lúc", nhật ký có thứ tự cố định.
     */
    static void runBot(Socket conn, String me) throws IOException, InterruptedException {
        View v = new View();
        int played = 0;
        Set<String> tried = new HashSet<>();
        BanCo.Lcg rng = new BanCo.Lcg(seed);
        boolean waiting = false, inRoom = false;
        InputStream in = conn.getInputStream();
        while (true) {
            Map<String, Object> m = GiaoThuc.recvJSON(in);
            if (m == null) continue;
            String t = GiaoThuc.str(m, "type");
            v.apply(m, me);
            boolean wantMore = played < games && !bot.equals("im_lang");
            switch (t) {
                case "welcome" -> {
                    int n = m.get("rooms") instanceof List<?> l ? l.size() : 0;
                    System.out.println("[" + me + "] Vào sảnh; số phòng hiện có: " + n);
                    if (!create.isEmpty()) GiaoThuc.sendJSON(conn, Json.obj("type", "create", "room", create));
                    else GiaoThuc.sendJSON(conn, Json.obj("type", "join", "room", join));
                }
                case "joined" -> {
                    inRoom = true;
                    System.out.println("[" + me + "] Vào phòng " + GiaoThuc.str(m, "room") + ", cầm quân " + GiaoThuc.str(m, "role"));
                }
                case "error" -> {
                    System.out.println("[" + me + "] Server báo lỗi " + GiaoThuc.str(m, "code") + ": " + GiaoThuc.str(m, "msg"));
                    if (!inRoom) {                              // không tạo/vào được phòng: bot dừng
                        GiaoThuc.sendJSON(conn, Json.obj("type", "bye"));
                        return;
                    }
                }
                case "start" -> {
                    System.out.println("[" + me + "] Ván " + GiaoThuc.num(m, "game_no") + " bắt đầu: X = " + GiaoThuc.str(m, "x")
                            + ", O = " + GiaoThuc.str(m, "o") + " (tôi cầm " + v.mark + ")");
                    if (tryErrors && v.mark.equals("O") && tried.add("early")) {
                        System.out.println("[" + me + "] (thử lỗi) đánh h8 khi chưa đến lượt");
                        GiaoThuc.sendJSON(conn, Json.obj("type", "move", "r", 7, "c", 7));
                    }
                }
                case "rematch_wait" -> {
                    if (!GiaoThuc.str(m, "by").equals(me) && waiting) {
                        waiting = false;
                        if (!wantMore) {
                            quitRoom(conn, me, played);
                            return;
                        }
                        System.out.println("[" + me + "] " + GiaoThuc.str(m, "by") + " xin chơi lại – đồng ý");
                        GiaoThuc.sendJSON(conn, Json.obj("type", "rematch"));
                    }
                }
                case "left" -> {
                    System.out.println("[" + me + "] " + GiaoThuc.str(m, "player") + " rời phòng");
                    if (waiting) {
                        quitRoom(conn, me, played);
                        return;
                    }
                }
                case "over" -> {
                    played++;
                    wantMore = played < games && !bot.equals("im_lang");
                    System.out.println("[" + me + "] " + overText(m));
                    String winner = GiaoThuc.str(m, "winner");
                    boolean leader = winner.equals(me) || (winner.isEmpty() && v.mark.equals("X"));
                    if (GiaoThuc.str(m, "reason").equals("left")) {   // đối thủ đã đi: không còn ai để chơi
                        quitRoom(conn, me, played);
                        return;
                    } else if (leader) {
                        waiting = true;
                    } else if (wantMore) {
                        System.out.println("[" + me + "] Xin chơi lại");
                        GiaoThuc.sendJSON(conn, Json.obj("type", "rematch"));
                    } else {
                        quitRoom(conn, me, played);
                        return;
                    }
                }
                default -> { }
            }
            if ((t.equals("start") || t.equals("moved")) && !v.mark.isEmpty() && v.turn.equals(v.mark) && !bot.equals("im_lang")) {
                Thread.sleep(delayMs);                          // "suy nghĩ" để người xem kịp theo dõi
                if (tryErrors && v.lastR >= 0 && tried.add("bad_cells")) {
                    System.out.println("[" + me + "] (thử lỗi) đánh vào ô đã có quân " + BanCo.cellName(v.lastR, v.lastC) + " và ô ngoài bàn (15, 3)");
                    GiaoThuc.sendJSON(conn, Json.obj("type", "move", "r", v.lastR, "c", v.lastC));
                    GiaoThuc.sendJSON(conn, Json.obj("type", "move", "r", BanCo.N, "c", 3));
                }
                int[] rc = v.b.chooseMove(v.mark.charAt(0), bot, rng);
                GiaoThuc.sendJSON(conn, Json.obj("type", "move", "r", rc[0], "c", rc[1]));
            }
        }
    }

    // ---------------------------------------------------------------- người xem tự động
    @SuppressWarnings("unchecked")
    static void runWatcher(Socket conn, String me) throws IOException {
        View v = new View();
        InputStream in = conn.getInputStream();
        while (true) {
            Map<String, Object> m = GiaoThuc.recvJSON(in);
            if (m == null) continue;
            v.apply(m, me);
            switch (GiaoThuc.str(m, "type")) {
                case "welcome" -> GiaoThuc.sendJSON(conn, Json.obj("type", "watch", "room", watch));
                case "joined" -> {
                    Map<String, Object> info = m.get("info") instanceof Map<?, ?> i ? (Map<String, Object>) i : Map.of();
                    System.out.println("[" + me + "] Xem phòng " + GiaoThuc.str(m, "room") + " (X = " + orNone(info, "x") + ", O = " + orNone(info, "o") + ")");
                }
                case "error" -> {
                    System.out.println("[" + me + "] Server báo lỗi " + GiaoThuc.str(m, "code") + ": " + GiaoThuc.str(m, "msg"));
                    return;
                }
                case "board" -> System.out.println("[" + me + "] Nhận ảnh chụp bàn cờ: ván " + GiaoThuc.num(m, "game_no") + ", đã đi " + GiaoThuc.num(m, "moves") + " nước");
                case "start" -> System.out.println("[" + me + "] Ván " + GiaoThuc.num(m, "game_no") + ": X = " + GiaoThuc.str(m, "x") + ", O = " + GiaoThuc.str(m, "o"));
                case "moved" -> {
                    if (GiaoThuc.num(m, "move_no") % 10 == 0)
                        System.out.println("[" + me + "] Nước " + GiaoThuc.num(m, "move_no") + ": " + GiaoThuc.str(m, "by") + " (" + GiaoThuc.str(m, "mark") + ") "
                                + BanCo.cellName(GiaoThuc.num(m, "r"), GiaoThuc.num(m, "c")));
                }
                case "over" -> {
                    System.out.println("[" + me + "] " + overText(m));
                    for (String line : v.b.render(v.lastR, v.lastC)) System.out.println(line);
                    String lt = lineText(m);
                    if (!lt.isEmpty()) System.out.println("[" + me + "] Dãy thắng: " + lt);
                }
                case "left" -> System.out.println("[" + me + "] " + GiaoThuc.str(m, "player") + " rời phòng");
                case "room_closed" -> {
                    System.out.println("[" + me + "] Phòng " + GiaoThuc.str(m, "room") + " đã đóng");
                    GiaoThuc.sendJSON(conn, Json.obj("type", "bye"));
                    return;
                }
                default -> { }
            }
        }
    }

    // ---------------------------------------------------------------- người chơi thật
    static final String HELP = "Lệnh: ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> ví dụ h8 | lai | roi | thoat";

    @SuppressWarnings("unchecked")
    static void runHuman(Socket conn, String me) throws IOException {
        View v = new View();
        Object lock = new Object();                          // luồng nhận và luồng bàn phím cùng dùng View
        Runnable showBoard = () -> {
            for (String line : v.b.render(v.lastR, v.lastC)) System.out.println(line);
            if (!v.mark.isEmpty())
                System.out.println("[" + me + "] Bạn cầm " + v.mark + "; " + (v.turn.equals(v.mark) ? "ĐẾN LƯỢT BẠN" : "chờ đối thủ"));
        };
        boolean[] quitting = {false};                        // người dùng gõ thoat: lỗi đọc sau đó là bình thường
        Thread.ofPlatform().daemon().start(() -> {
            try {
                InputStream in = conn.getInputStream();
                while (true) {
                    Map<String, Object> m = GiaoThuc.recvJSON(in);
                    if (m == null) continue;
                    synchronized (lock) {
                        v.apply(m, me);
                        switch (GiaoThuc.str(m, "type")) {
                            case "welcome", "rooms" -> {
                                List<Object> rooms = m.get("rooms") instanceof List<?> l ? (List<Object>) l : List.of();
                                System.out.println("[" + me + "] Có " + rooms.size() + " phòng" + (rooms.isEmpty() ? "" : ":"));
                                for (Object x : rooms) {
                                    Map<String, Object> r = (Map<String, Object>) x;
                                    System.out.println("    " + GiaoThuc.str(r, "room") + ": X = " + orNone(r, "x") + ", O = " + orNone(r, "o") + ", "
                                            + GiaoThuc.num(r, "watchers") + " người xem, " + GiaoThuc.str(r, "state"));
                                }
                            }
                            case "joined" -> System.out.println("[" + me + "] Vào phòng " + GiaoThuc.str(m, "room") + " với vai trò " + GiaoThuc.str(m, "role"));
                            case "start" -> {
                                System.out.println("[" + me + "] Ván " + GiaoThuc.num(m, "game_no") + ": X = " + GiaoThuc.str(m, "x") + ", O = "
                                        + GiaoThuc.str(m, "o") + "; mỗi nước tối đa " + GiaoThuc.num(m, "timeout_ms") / 1000 + " s");
                                showBoard.run();
                            }
                            case "moved", "board" -> showBoard.run();
                            case "over" -> {
                                String lt = lineText(m);
                                System.out.println("[" + me + "] " + overText(m) + (lt.isEmpty() ? "" : "; dãy " + lt));
                                System.out.println("[" + me + "] Gõ 'lai' để xin chơi lại hoặc 'roi' để rời phòng");
                            }
                            case "rematch_wait" -> System.out.println("[" + me + "] " + GiaoThuc.str(m, "by") + " xin chơi lại");
                            case "left" -> System.out.println("[" + me + "] " + GiaoThuc.str(m, "player") + " rời phòng");
                            case "room_closed" -> System.out.println("[" + me + "] Phòng " + GiaoThuc.str(m, "room") + " đã đóng");
                            case "error" -> System.out.println("[" + me + "] Lỗi " + GiaoThuc.str(m, "code") + ": " + GiaoThuc.str(m, "msg"));
                            default -> { }
                        }
                    }
                }
            } catch (IOException e) {
                if (!quitting[0]) System.out.println("[" + me + "] Mất kết nối tới server");
            }
        });
        System.out.println("[" + me + "] " + HELP);
        if (!create.isEmpty()) GiaoThuc.sendJSON(conn, Json.obj("type", "create", "room", create));
        else if (!join.isEmpty()) GiaoThuc.sendJSON(conn, Json.obj("type", "join", "room", join));
        BufferedReader stdin = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String lineIn;
        while ((lineIn = stdin.readLine()) != null) {
            String[] parts = lineIn.strip().split("\\s+");
            if (parts[0].isEmpty()) continue;
            String cmd = parts[0].toLowerCase(java.util.Locale.ROOT), arg = parts.length > 1 ? parts[1] : "";
            if (cmd.equals("thoat")) break;
            switch (cmd) {
                case "ds" -> GiaoThuc.sendJSON(conn, Json.obj("type", "list"));
                case "tao" -> GiaoThuc.sendJSON(conn, Json.obj("type", "create", "room", arg));
                case "vao" -> GiaoThuc.sendJSON(conn, Json.obj("type", "join", "room", arg));
                case "xem" -> GiaoThuc.sendJSON(conn, Json.obj("type", "watch", "room", arg));
                case "lai" -> GiaoThuc.sendJSON(conn, Json.obj("type", "rematch"));
                case "roi" -> GiaoThuc.sendJSON(conn, Json.obj("type", "leave"));
                default -> {
                    int[] rc = BanCo.parseCell(cmd);
                    if (rc == null) {
                        System.out.println("[" + me + "] " + HELP);
                        continue;
                    }
                    boolean taken;
                    synchronized (lock) { taken = v.b.o[rc[0]][rc[1]] != BanCo.EMPTY; }
                    if (taken) {                                // kiểm tra sớm phía client; server vẫn kiểm tra lại
                        System.out.println("[" + me + "] Ô " + cmd + " đã có quân");
                        continue;
                    }
                    GiaoThuc.sendJSON(conn, Json.obj("type", "move", "r", rc[0], "c", rc[1]));
                }
            }
        }
        quitting[0] = true;
        GiaoThuc.sendJSON(conn, Json.obj("type", "bye"));
    }

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--tim" -> tim = Integer.parseInt(args[++i]);
                case "--tao" -> create = args[++i];
                case "--vao" -> join = args[++i];
                case "--xem" -> watch = args[++i];
                case "--bot" -> bot = args[++i];
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--thu-loi" -> tryErrors = true;
                case "--so-van" -> games = Integer.parseInt(args[++i]);
                case "--tre-ms" -> delayMs = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        if (pos.isEmpty()) {
            System.out.println("Cách dùng: java -cp out NguoiChoi <tên> [host] [port] [--tim CỔNG] [--tao P | --vao P | --xem P] [--bot KIỂU]");
            System.exit(1);
        }
        if (!bot.isEmpty() && !BanCo.STYLES.containsKey(bot) && !bot.equals("im_lang")) {
            System.out.println("Kiểu bot không hợp lệ: " + bot);
            System.exit(2);
        }
        if (!bot.isEmpty() && create.isEmpty() && join.isEmpty()) {
            System.out.println("Bot cần --tao hoặc --vao");
            System.exit(2);
        }
        String me = pos.get(0);
        Socket conn;
        try {
            conn = connect(me, pos.size() > 1 ? pos.get(1) : "127.0.0.1", Integer.parseInt(pos.size() > 2 ? pos.get(2) : "9100"));
        } catch (IOException e) {
            System.out.println("[" + me + "] Không kết nối được: " + e.getMessage());
            System.exit(1);
            return;
        }
        try (conn) {
            GiaoThuc.sendJSON(conn, Json.obj("type", "hello", "name", me));
            if (!watch.isEmpty()) runWatcher(conn, me);
            else if (!bot.isEmpty()) runBot(conn, me);
            else runHuman(conn, me);
        } catch (IOException e) {
            System.out.println("[" + me + "] Server đóng kết nối");
        }
        System.exit(0);
    }
}
