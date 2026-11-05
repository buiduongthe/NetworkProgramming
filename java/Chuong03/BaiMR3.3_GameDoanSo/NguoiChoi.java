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
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;

/**
 * Bài mở rộng MR3.3 – Client trò chơi đoán số: người chơi thật (bàn phím) hoặc bot.
 *
 * Chế độ người chơi: một luồng nhận thông điệp của server và in ra; luồng chính chỉ hỏi số khi đến lượt.
 * Client cũng kiểm tra đầu vào (phải là số nguyên) để báo lỗi sớm cho người dùng, nhưng server vẫn
 * kiểm tra lại mọi thứ – server không bao giờ tin client.
 *
 * Các bot (dùng cho demo tất định):
 *   chia_doi   – tìm kiếm nhị phân: đoán điểm giữa khoảng [lo, hi] server gửi kèm lượt.
 *   ngau_nhien – đoán ngẫu nhiên trong [lo, hi] (bộ sinh LCG, hạt giống --seed).
 *   nghich     – cố tình gửi thông điệp sai để xem server kiểm tra: đoán khi ván chưa bắt đầu,
 *                khung không phải JSON, đoán khi chưa đến lượt, giá trị là chuỗi, số ngoài khoảng.
 *   im_lang    – không bao giờ trả lời: bị server tính hết giờ, rồi tự ngắt kết nối.
 *
 * Chạy: java -cp out NguoiChoi <tên> [host] [port] [--bot KIỂU] [--seed N] [--nghi-ms 300]
 */
public class NguoiChoi {
    static final Map<String, String> HINT_TEXT = Map.of("THAP", "thấp quá", "CAO", "cao quá", "DUNG", "ĐÚNG");

    static int num(Map<String, Object> m, String k) { return ((Number) m.get(k)).intValue(); }

    /** Chuyển thông điệp của server thành dòng chữ cho người chơi; trả null nếu không cần in. */
    @SuppressWarnings("unchecked")
    static String describe(String me, Map<String, Object> msg) {
        Object t = msg.get("type");
        if ("welcome".equals(t))
            return "[" + me + "] Vào phòng chờ: " + msg.get("waiting") + "/" + msg.get("need") + " người, đoán số trong "
                    + msg.get("min") + "–" + msg.get("max");
        if ("start".equals(t)) {
            List<String> order = new ArrayList<>();
            for (Object o : (List<Object>) msg.get("order")) order.add(String.valueOf(o));
            return "[" + me + "] Ván " + msg.get("round") + " bắt đầu, thứ tự: " + String.join(", ", order);
        }
        if ("turn".equals(t) && me.equals(msg.get("player")))
            return "[" + me + "] Đến lượt bạn (lượt " + msg.get("turn_no") + "), khoảng " + msg.get("lo") + "–" + msg.get("hi");
        if ("result".equals(t))
            return "[" + me + "] " + msg.get("player") + " đoán " + msg.get("value") + ": "
                    + HINT_TEXT.getOrDefault(String.valueOf(msg.get("hint")), String.valueOf(msg.get("hint")));
        if ("timeout".equals(t)) return "[" + me + "] " + msg.get("player") + " hết giờ, mất lượt";
        if ("left".equals(t)) return "[" + me + "] " + msg.get("player") + " đã rời trò chơi";
        if ("error".equals(t)) return "[" + me + "] Server báo lỗi " + msg.get("code") + ": " + msg.get("msg");
        if ("game_over".equals(t)) {
            Object w = msg.get("winner");
            String who = w == null || "".equals(w) ? "không ai" : w.toString();
            return "[" + me + "] Kết thúc: " + who + " thắng, số bí mật là " + msg.get("secret") + " (" + msg.get("turns") + " lượt)";
        }
        if ("bye".equals(t)) return "[" + me + "] Server kết thúc phiên";
        return null;
    }

    static void say(String line) { if (line != null) System.out.println(line); }

    // ---------------------------------------------------------------- bot
    static void runBot(Socket s, String me, String kind, long seed, int thinkMs) throws Exception {
        GiaoThuc.Lcg rng = new GiaoThuc.Lcg(seed);
        Set<String> tricksDone = new HashSet<>();                  // các "trò nghịch" đã làm (mỗi trò một lần)
        InputStream in = s.getInputStream();
        while (true) {
            Map<String, Object> msg = GiaoThuc.recvJson(in);
            if (msg == null) continue;
            say(describe(me, msg));
            Object t = msg.get("type");
            if ("game_over".equals(t) || "bye".equals(t)) return;
            if (kind.equals("nghich") && "welcome".equals(t)) {
                System.out.println("[" + me + "] (nghịch) đoán khi ván chưa bắt đầu, rồi gửi khung không phải JSON");
                GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", 1));
                GiaoThuc.sendFrame(s, "xin chào".getBytes(StandardCharsets.UTF_8));
            }
            if ("timeout".equals(t) && me.equals(msg.get("player")) && kind.equals("im_lang")) {
                System.out.println("[" + me + "] (im lặng) bị hết giờ, ngắt kết nối");
                return;
            }
            if (!"turn".equals(t)) continue;
            if (!me.equals(msg.get("player"))) {
                if (kind.equals("nghich") && tricksDone.add("out_of_turn")) {
                    System.out.println("[" + me + "] (nghịch) đoán khi đang là lượt của " + msg.get("player"));
                    GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", msg.get("lo")));
                }
                continue;
            }
            if (kind.equals("im_lang")) continue;
            int lo = num(msg, "lo"), hi = num(msg, "hi");
            Thread.sleep(thinkMs);                                 // "suy nghĩ" để người xem kịp theo dõi
            if (kind.equals("nghich") && tricksDone.add("bad_values")) {
                System.out.println("[" + me + "] (nghịch) gửi giá trị chuỗi và số ngoài khoảng");
                GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", "năm mươi"));
                GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", hi + 50));
            }
            int guess = kind.equals("ngau_nhien") ? rng.nextInt(lo, hi) : Math.floorDiv(lo + hi, 2);
            System.out.println("[" + me + "] Đoán " + guess);
            GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", guess));
        }
    }

    // ---------------------------------------------------------------- người chơi thật
    static void runHuman(Socket s, String me) throws Exception {
        BlockingQueue<String> turns = new LinkedBlockingQueue<>();   // luồng nhận báo cho luồng chính: "turn" hoặc "end"
        Thread receiver = new Thread(() -> {
            try {
                InputStream in = s.getInputStream();
                while (true) {
                    Map<String, Object> msg = GiaoThuc.recvJson(in);
                    if (msg == null) continue;
                    say(describe(me, msg));
                    if ("turn".equals(msg.get("type")) && me.equals(msg.get("player"))) turns.add("turn");
                    if ("game_over".equals(msg.get("type")) || "bye".equals(msg.get("type"))) break;
                }
            } catch (IOException ignored) { }
            turns.add("end");
        });
        receiver.setDaemon(true);
        receiver.start();
        boolean echo = System.console() == null;                   // đầu vào từ tệp/ống: in lại để nhật ký dễ đọc
        BufferedReader stdin = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        while (turns.take().equals("turn")) {
            while (true) {
                System.out.print("[" + me + "] Nhập số: ");
                System.out.flush();
                String line = stdin.readLine();
                if (line == null) {                                // hết đầu vào (Ctrl+D / Ctrl+Z): rời trò chơi
                    System.out.println();
                    GiaoThuc.sendJson(s, Json.obj("type", "bye"));
                    return;
                }
                line = line.strip();
                if (echo) System.out.println(line);
                int value;
                try {
                    value = Integer.parseInt(line);                // kiểm tra phía client: chỉ để báo lỗi sớm
                } catch (NumberFormatException e) {
                    System.out.println("[" + me + "] Vui lòng nhập một số nguyên");
                    continue;
                }
                GiaoThuc.sendJson(s, Json.obj("type", "guess", "value", value));
                break;
            }
        }
    }

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        String bot = null;
        long seed = 1;
        int thinkMs = 300;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--bot" -> bot = args[++i];
                case "--seed" -> seed = Long.parseLong(args[++i]);
                case "--nghi-ms" -> thinkMs = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        if (pos.isEmpty() || (bot != null && !List.of("chia_doi", "ngau_nhien", "nghich", "im_lang").contains(bot))) {
            System.err.println("Cách dùng: java -cp out NguoiChoi <tên> [host] [port] [--bot chia_doi|ngau_nhien|nghich|im_lang] [--seed N] [--nghi-ms 300]");
            System.exit(2);
        }
        String me = pos.get(0);
        String host = pos.size() > 1 ? pos.get(1) : "127.0.0.1";
        int port = pos.size() > 2 ? Integer.parseInt(pos.get(2)) : 9000;
        Socket s = new Socket();
        try {
            s.connect(new InetSocketAddress(host, port), 3000);
        } catch (IOException e) {
            System.err.println("[" + me + "] Không kết nối được " + host + ":" + port + ": " + e.getMessage());
            System.exit(1);
        }
        s.setTcpNoDelay(true);                                     // tắt Nagle: gửi thông điệp nhỏ ngay
        System.out.println("[" + me + "] Đã kết nối " + host + ":" + port);
        try (s) {
            GiaoThuc.sendJson(s, Json.obj("type", "hello", "name", me));
            if (bot != null) runBot(s, me, bot, seed, thinkMs);
            else runHuman(s, me);
        } catch (IOException e) {                                  // ConnectionClosed, ProtocolError, lỗi socket
            System.out.println("[" + me + "] Server đóng kết nối");
        }
        System.exit(0);
    }
}
