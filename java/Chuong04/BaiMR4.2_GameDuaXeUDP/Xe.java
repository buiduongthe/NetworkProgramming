import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Bài mở rộng MR4.2 – Client đua xe UDP: người lái (bàn phím) hoặc bot.
 *
 * Mỗi khi nhận ảnh chụp tick k (chỉ nhận ảnh MỚI HƠN ảnh đang có), client:
 *  1. đối chiếu: so vị trí xe mình mà client đã dự đoán với vị trí server gửi (lệch khi đầu vào bị mất);
 *  2. dự đoán phía client: từ ảnh chụp k, chạy lại vật lý với các đầu vào của chính mình để biết xe
 *     sẽ ở đâu ở tick k+1 (xe khác thì ngoại suy theo vận tốc cuối);
 *  3. chọn đầu vào cho tick k+2 và gửi kèm 2 đầu vào trước đó (dư thừa).
 * Khi một ảnh chụp bị mất, khung hình của tick đó được vẽ bằng dự đoán – người chơi không thấy xe "đứng hình".
 *
 * Chạy: java -cp out Xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]
 *       Người lái: gõ g (ga), n (nhả), p (phanh) rồi Enter; đầu vào giữ nguyên tới khi đổi.
 */
public class Xe {
    /** Trạng thái một xe trong ảnh chụp: [id, x, v, done]. */
    record CarState(int id, int x, int v, int done) {}

    /** Ảnh chụp server gửi. */
    record Snap(int race, int tick, String phase, List<String> names, List<CarState> cars, List<Object[]> rank) {}

    static int num(Object o) { return ((Number) o).intValue(); }

    @SuppressWarnings("unchecked")
    static Snap parseSnap(Map<String, Object> m) {
        List<String> names = new ArrayList<>();
        for (Object n : (List<Object>) m.get("names")) names.add((String) n);
        List<CarState> cars = new ArrayList<>();
        for (Object c : (List<Object>) m.get("cars")) {
            List<Object> f = (List<Object>) c;
            cars.add(new CarState(num(f.get(0)), num(f.get(1)), num(f.get(2)), num(f.get(3))));
        }
        List<Object[]> rank = new ArrayList<>();
        if (m.get("rank") instanceof List<?> r)
            for (Object o : r) {
                List<Object> f = (List<Object>) o;
                rank.add(new Object[] {f.get(0), num(f.get(1))});
            }
        return new Snap(num(m.get("race")), num(m.get("tick")), (String) m.get("phase"), names, cars, rank);
    }

    /** Những gì client biết về cuộc đua: ảnh chụp mới nhất, đầu vào của mình, thống kê. */
    static final class RaceView {
        final String me;
        Integer myId = null;
        int lastTick = -1;
        Snap snap = null;
        final Map<Integer, Integer> myInputs = new HashMap<>();   // tick -> đầu vào mình đã chọn
        int got, stale, missed, maxGap;
        int errN, errMax;                    // dự đoán xe mình sai (server đã dùng đầu vào khác)
        int oerrN, oerrMax;                  // ngoại suy xe khác sai sau một quãng hụt ảnh chụp

        RaceView(String me) { this.me = me; }

        /** Đầu vào mình dùng ở tick t (nếu chưa chọn thì giữ đầu vào gần nhất – giống server). */
        int inputAt(int t) {
            while (t >= 0) {
                Integer v = myInputs.get(t);
                if (v != null) return v;
                t--;
            }
            return 0;
        }

        CarState car(Snap s, int cid) {
            for (CarState c : s.cars()) if (c.id() == cid) return c;
            throw new IllegalStateException("không có xe " + cid);
        }

        /** Dự đoán phía client: chạy lại vật lý từ ảnh chụp với chính các đầu vào của mình. */
        CarState predictOwn(Snap s, int T) {
            CarState c = car(s, myId);
            int x = c.x(), v = c.v(), done = c.done();
            for (int t = s.tick() + 1; t <= T; t++) {
                if (t < DuaXe.START || done >= 0) continue;
                DuaXe.Step st = DuaXe.step(x, v, inputAt(t));
                x = st.x();
                v = st.v();
                if (x >= DuaXe.TRACK) {
                    x = DuaXe.TRACK;
                    v = 0;
                    done = t - DuaXe.START + 1;
                }
            }
            return new CarState(myId, x, v, done);
        }

        /** Xe khác: ngoại suy tuyến tính (dead reckoning) theo vận tốc trong ảnh chụp. */
        CarState predictOther(Snap s, int cid, int T) {
            CarState c = car(s, cid);
            int ticks = T - Math.max(s.tick(), DuaXe.START - 1);
            if (c.done() >= 0 || ticks <= 0) return c;
            return new CarState(cid, Math.min(DuaXe.TRACK, c.x() + c.v() * ticks), c.v(), c.done());
        }

        /** Dòng khung hình của tick T (ảnh chụp thật nếu có, không thì dự đoán). */
        List<String> frame(int T, boolean predicted) {
            List<String> lines = new ArrayList<>();
            lines.add("[" + me + "] Khung t=" + (T - DuaXe.START) + (predicted ? " (dự đoán: thiếu ảnh chụp)" : ""));
            Snap s = snap;
            for (int cid = 0; cid < s.names().size(); cid++) {
                String name = s.names().get(cid);
                CarState c = s.tick() == T ? car(s, cid) : cid == myId ? predictOwn(s, T) : predictOther(s, cid, T);
                String tail = c.done() >= 0 ? " về đích (" + c.done() + " tick)" : " x=" + DuaXe.fmt2(c.x()) + " v=" + DuaXe.fmt2(c.v());
                char letter = Character.toUpperCase(name.charAt(0));
                lines.add("  " + letter + "|" + DuaXe.trackLine(letter, c.x()) + tail);
            }
            return lines;
        }

        /** Nhận một ảnh chụp; trả false nếu là ảnh cũ (đến muộn) – bị bỏ, ảnh mới nhất thắng. */
        boolean accept(Snap s) {
            int k = s.tick();
            if (k <= lastTick) {
                stale++;
                return false;
            }
            if (myId == null) myId = s.names().indexOf(me);
            if (lastTick >= 0 && k - lastTick > 1) {
                int gap = k - lastTick - 1;
                missed += gap;
                maxGap = Math.max(maxGap, gap);
                if (k >= DuaXe.START)                            // xe khác: ngoại suy trong quãng hụt lệch bao nhiêu?
                    for (int cid = 0; cid < s.names().size(); cid++)
                        if (cid != myId) {
                            int err = Math.abs(predictOther(snap, cid, k).x() - car(s, cid).x());
                            if (err != 0) {
                                oerrN++;
                                oerrMax = Math.max(oerrMax, err);
                            }
                        }
            }
            if (snap != null && k >= DuaXe.START) {              // đối chiếu dự đoán với sự thật của server
                int err = Math.abs(predictOwn(snap, k).x() - car(s, myId).x());
                if (err != 0) {
                    errN++;
                    errMax = Math.max(errMax, err);
                }
            }
            got++;
            return true;
        }

        interface Chooser { int choose(int x, int v, int done); }

        /** Chọn đầu vào cho tick k+LEAD, trả gói đầu vào cần gửi (mang kèm WINDOW-1 đầu vào trước đó). */
        Map<String, Object> decide(Chooser choose) {
            int k = snap.tick();
            int T = k + DuaXe.LEAD;
            CarState c = predictOwn(snap, T - 1);
            int inp = choose.choose(c.x(), c.v(), c.done());
            int start = myInputs.isEmpty() ? T : Collections.max(myInputs.keySet()) + 1;
            for (int t = start; t < T; t++) myInputs.put(t, inputAt(t));   // tick bị bỏ qua (do mất ảnh chụp): giữ đầu vào cũ
            myInputs.put(T, inp);
            List<Object> window = new ArrayList<>();
            for (int j = 0; j < DuaXe.WINDOW; j++) window.add(inputAt(T - j));
            return Json.obj("t", "in", "seq", T, "in", window);
        }
    }

    static void send(DatagramSocket sock, InetSocketAddress addr, Map<String, Object> obj) throws IOException {
        byte[] d = Json.stringify(obj).getBytes(StandardCharsets.UTF_8);
        sock.send(new DatagramPacket(d, d.length, addr));
    }

    /** Nhận một datagram JSON; null nếu hết thời gian chờ hoặc gói hỏng. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> recv(DatagramSocket sock, byte[] buf) throws IOException {
        DatagramPacket pk = new DatagramPacket(buf, buf.length);
        sock.receive(pk);
        Object o = Json.parse(new String(pk.getData(), 0, pk.getLength(), StandardCharsets.UTF_8));
        return o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
    }

    /** Gửi join (lặp lại mỗi 250 ms vì UDP có thể mất) tới khi nhận welcome. */
    static Map<String, Object> join(DatagramSocket sock, InetSocketAddress addr, String me, byte[] buf) throws IOException {
        sock.setSoTimeout(250);
        while (true) {
            send(sock, addr, Json.obj("t", "join", "name", me));
            try {
                Map<String, Object> p = recv(sock, buf);
                if (p != null && "welcome".equals(p.get("t"))) return p;
            } catch (SocketTimeoutException e) {
                // gửi lại join
            }
        }
    }

    /** Trạng thái người lái: đầu vào hiện tại (do luồng bàn phím đổi) và lúc nhận ảnh chụp gần nhất. */
    static final class Human {
        volatile int inp = 0;
        volatile long tLast = System.nanoTime();
    }

    static void raceOnce(DatagramSocket sock, InetSocketAddress addr, String me, String bot, int khung, Human human) throws IOException {
        byte[] buf = new byte[4096];
        Map<String, Object> w = join(sock, addr, me, buf);
        System.out.println("[" + me + "] Vào cuộc đua " + w.get("race") + " (cần " + w.get("need") + " xe; mất gói giả lập "
                + w.get("mat") + " %, đảo thứ tự " + w.get("dao") + " %)");
        int race = num(w.get("race"));
        RaceView view = new RaceView(me);
        int nextFrame = DuaXe.START;
        sock.setSoTimeout(human != null ? 50 : 3000);
        long lastDraw = 0;
        while (true) {
            Map<String, Object> p;
            try {
                p = recv(sock, buf);
            } catch (SocketTimeoutException e) {
                if (human == null) {
                    System.out.println("[" + me + "] Mất liên lạc với server");
                    return;
                }
                p = null;
            }
            if (p != null && "snap".equals(p.get("t")) && p.get("race") instanceof Long r && r == race) {
                Snap s = parseSnap(p);
                if (view.accept(s)) {
                    int k = s.tick();
                    if (human != null) human.tLast = System.nanoTime();
                    if (human == null && khung > 0 && s.phase().equals("DUA") && view.snap != null) {
                        while (nextFrame <= k) {                     // khung cho mọi tick cần vẽ đến hết tick k
                            List<String> out;
                            if (nextFrame == k) {
                                view.snap = s;
                                out = view.frame(k, false);
                            } else {
                                out = view.frame(nextFrame, true);   // dùng ảnh chụp cũ + dự đoán
                            }
                            System.out.println(String.join("\n", out));
                            nextFrame += khung;
                        }
                    }
                    view.snap = s;
                    view.lastTick = k;
                    if (s.phase().equals("XONG")) {
                        List<String> rank = new ArrayList<>();
                        for (int i = 0; i < s.rank().size(); i++) {
                            int d = (Integer) s.rank().get(i)[1];
                            rank.add((i + 1) + ". " + s.rank().get(i)[0] + " " + (d >= 0 ? String.valueOf(d) : "-") + " tick");
                        }
                        System.out.println("[" + me + "] Kết quả: " + String.join(", ", rank));
                        System.out.println("[" + me + "] Ảnh chụp: nhận " + view.got + ", hụt " + view.missed + " tick (dài nhất "
                                + view.maxGap + "), bỏ " + view.stale + " ảnh cũ");
                        System.out.println("[" + me + "] Dự đoán xe mình: lệch " + view.errN + " lần (lớn nhất " + DuaXe.fmt2(view.errMax)
                                + " ô); ngoại suy xe khác: lệch " + view.oerrN + " lần (lớn nhất " + DuaXe.fmt2(view.oerrMax) + " ô)");
                        return;
                    }
                    RaceView.Chooser choose = human != null ? (x, v, done) -> human.inp
                            : (x, v, done) -> done < 0 ? DuaXe.botInput(x, v, bot) : 0;
                    send(sock, addr, view.decide(choose));
                }
            }
            if (human != null && view.snap != null && System.nanoTime() - lastDraw >= 100_000_000L) {   // vẽ lại 10 lần/giây
                lastDraw = System.nanoTime();
                int est = view.lastTick + (int) ((System.nanoTime() - human.tLast) / 1_000_000L / DuaXe.TICK_MS);
                if (est >= DuaXe.START) {
                    System.out.println("\033[H\033[2J" + String.join("\n", view.frame(est, est != view.lastTick)));
                    System.out.println("Đầu vào: " + DuaXe.INPUT_TEXT.get(human.inp) + "  (g = ga, n = nhả, p = phanh, Enter)");
                } else {
                    System.out.println("\033[H\033[2JXuất phát sau " + String.format(Locale.ROOT, "%.1f",
                            (DuaXe.START - est) * DuaXe.TICK_MS / 1000.0) + " s");
                }
            }
        }
    }

    /** Luồng đọc bàn phím: g/n/p + Enter đổi đầu vào hiện tại. */
    static void keyboard(Human h) {
        try {
            BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            String line;
            while ((line = in.readLine()) != null) {
                String c = line.strip().toLowerCase(Locale.ROOT);
                if (c.startsWith("g")) h.inp = 1;
                else if (c.startsWith("n")) h.inp = 0;
                else if (c.startsWith("p")) h.inp = -1;
            }
        } catch (IOException ignored) { }
    }

    public static void main(String[] args) throws Exception {
        List<String> pos = new ArrayList<>();
        String bot = null;
        int khung = 10, soVan = 1;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--bot" -> bot = args[++i];
                case "--khung" -> khung = Integer.parseInt(args[++i]);
                case "--so-van" -> soVan = Integer.parseInt(args[++i]);
                default -> pos.add(args[i]);
            }
        }
        if (pos.isEmpty() || (bot != null && !DuaXe.STYLES.containsKey(bot))) {
            System.err.println("Cách dùng: java -cp out Xe <tên> [host] [port] [--bot lieu|can_than] [--khung 10] [--so-van 1]");
            System.exit(2);
        }
        String me = pos.get(0);
        InetSocketAddress addr = new InetSocketAddress(pos.size() > 1 ? pos.get(1) : "127.0.0.1",
                pos.size() > 2 ? Integer.parseInt(pos.get(2)) : 9200);
        Human human = null;
        if (bot == null) {
            Human h = new Human();
            human = h;
            Thread t = new Thread(() -> keyboard(h));
            t.setDaemon(true);
            t.start();
        }
        try (DatagramSocket sock = new DatagramSocket()) {
            for (int i = 0; i < soVan; i++) raceOnce(sock, addr, me, bot, khung, human);
        }
    }
}
