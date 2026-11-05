import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.PriorityQueue;
import java.util.Set;
import java.util.function.BiConsumer;

/**
 * Lõi bộ mô phỏng mạng theo sự kiện rời rạc: topo, hàng đợi sự kiện, định tuyến theo chi phí
 * (Dijkstra) và quy tắc chuyển tiếp của từng loại nút. Không dùng socket thật: mỗi gói tin là một
 * đối tượng Java, "thời gian" là đồng hồ mô phỏng tính bằng micro giây (µs).
 *
 *  H1 ─┐                                   ┌─ H4
 *  H2 ─┼─ SW1 ── R1 ── R2 ── R3 ── SW2 ─┼─ H5      (R1 ── R3: đường dự phòng, chi phí 3)
 *  H3 ─┘   LAN A 192.168.1.0/24   LAN B 192.168.2.0/24  └─ H6
 *
 * Quy tắc chuyển tiếp:
 *  - host: nhận gói nếu gói dành cho mình (unicast đúng tên, quảng bá trong LAN, nhóm đã tham gia,
 *    địa chỉ anycast mình phục vụ); host không chuyển tiếp gói của người khác;
 *  - switch: quảng bá → gửi tràn mọi cổng trừ cổng vào; multicast → chỉ cổng có thành viên
 *    (IGMP snooping) và cổng router; unicast → đúng một cổng theo bảng định tuyến;
 *  - router: chặn quảng bá; unicast/anycast → giảm TTL, chọn chặng kế theo chi phí nhỏ nhất;
 *    multicast → chỉ chuyển tới các LAN có thành viên.
 */
final class Mang {

    /** Định dạng thời gian mô phỏng (µs) thành mili giây, một chữ số thập phân. */
    static String fmtT(long us) { return (us / 1000) + "." + (us % 1000 / 100); }

    /** Bộ sinh số giả ngẫu nhiên xorshift32: cùng seed → cùng dãy số ở mọi ngôn ngữ. */
    static final class Rng {
        private int x;                                     // 32 bit, coi như số không dấu
        Rng(int seed) {
            x = (int) (seed * 2654435761L + 1);
            if (x == 0) x = 1;
        }
        int next() {
            x ^= x << 13;
            x ^= x >>> 17;                                 // >>>: dịch phải không dấu như uint32
            x ^= x << 5;
            return x;
        }
        /** Số nguyên ngẫu nhiên trong [0, n). */
        int below(int n) { return Integer.remainderUnsigned(next(), n); }
    }

    static final class Node {
        final String name, kind, ip, lan;
        final int x, y;                                    // tọa độ để vẽ hoạt hình HTML
        boolean up = true;
        int sent, recv, deliv, fwd, drop;
        Node(String name, String kind, String ip, String lan, int x, int y) {
            this.name = name; this.kind = kind; this.ip = ip; this.lan = lan; this.x = x; this.y = y;
        }
    }

    static final class Link {
        final String a, b;
        final int cost;                                    // chi phí định tuyến
        final long delay, bw;                              // trễ lan truyền (µs); băng thông byte/ms
        boolean up = true;
        int loss;                                          // xác suất mất gói, phần nghìn
        long jitter;                                       // trễ ngẫu nhiên thêm tối đa (µs)
        final Map<String, Long> free = new HashMap<>();    // thời điểm mỗi chiều rảnh (hàng đợi FIFO)
        Link(String a, String b, int cost, long delay, long bw) {
            this.a = a; this.b = b; this.cost = cost; this.delay = delay; this.bw = bw;
            free.put(a, 0L); free.put(b, 0L);
        }
    }

    static final class Packet implements Cloneable {
        String cast;                                       // uni | bc | mc | igmp | any
        String kind;                                       // nhãn hiển thị: TCP, UDP, UNI, BC, MC, IGMP, ANY
        String src, dst, label;
        int size, ttl = 64;
        String dropA = "", dropB = "";                     // kịch bản ép mất gói trên liên kết dropA→dropB
        // dữ liệu của kịch bản
        String flags = "", msg = "";
        int seq, ack, n;
        long t;
        boolean reply, join;

        Packet(String cast, String kind, String src, String dst, String label, int size) {
            this.cast = cast; this.kind = kind; this.src = src; this.dst = dst; this.label = label; this.size = size;
        }
        Packet copy() {
            try { return (Packet) clone(); } catch (CloneNotSupportedException e) { throw new AssertionError(e); }
        }
    }

    static final long LAN_US = 500, WAN_US = 5000;         // trễ lan truyền: trong LAN 0,5 ms; giữa router 5 ms
    static final long LAN_BW = 12500, WAN_BW = 1250;       // 100 Mbit/s và 10 Mbit/s (byte/ms)

    /** {tên, loại, IP, LAN, x, y} */
    static final Object[][] NODE_DEFS = {
        {"H1", "host", "192.168.1.11", "A", 60, 80}, {"H2", "host", "192.168.1.12", "A", 60, 210},
        {"H3", "host", "192.168.1.13", "A", 60, 340}, {"SW1", "switch", "-", "A", 190, 210},
        {"R1", "router", "192.168.1.1", "A", 330, 210}, {"R2", "router", "10.0.0.2", "-", 450, 80},
        {"R3", "router", "192.168.2.1", "B", 570, 210}, {"SW2", "switch", "-", "B", 710, 210},
        {"H4", "host", "192.168.2.14", "B", 840, 80}, {"H5", "host", "192.168.2.15", "B", 840, 210},
        {"H6", "host", "192.168.2.16", "B", 840, 340},
    };

    /** {a, b, chi phí, trễ µs, băng thông byte/ms} */
    static final Object[][] LINK_DEFS = {
        {"H1", "SW1", 1, LAN_US, LAN_BW}, {"H2", "SW1", 1, LAN_US, LAN_BW},
        {"H3", "SW1", 1, LAN_US, LAN_BW}, {"SW1", "R1", 1, LAN_US, LAN_BW},
        {"R1", "R2", 1, WAN_US, WAN_BW}, {"R2", "R3", 1, WAN_US, WAN_BW},
        {"R1", "R3", 3, 8000L, WAN_BW}, {"R3", "SW2", 1, LAN_US, LAN_BW},
        {"SW2", "H4", 1, LAN_US, LAN_BW}, {"SW2", "H5", 1, LAN_US, LAN_BW},
        {"SW2", "H6", 1, LAN_US, LAN_BW},
    };

    record Nbr(String name, Link l) {}

    // ---------- hàng đợi sự kiện (heap theo thời điểm, cùng thời điểm thì theo thứ tự thêm) ----------
    record Event(long t, int seq, Runnable fn) {}

    final Rng rng;
    long now;
    private final PriorityQueue<Event> q = new PriorityQueue<>(
            (a, b) -> a.t() != b.t() ? Long.compare(a.t(), b.t()) : Integer.compare(a.seq(), b.seq()));
    private int seq;
    final List<String> order = new ArrayList<>();
    final Map<String, Node> nodes = new HashMap<>();
    final Map<String, List<Nbr>> adj = new HashMap<>();
    final Map<String, Link> links = new HashMap<>();             // khóa "A|B" (cả hai chiều)
    final Map<String, List<String>> anycast = new HashMap<>();   // địa chỉ anycast -> host phục vụ
    final Map<String, List<String>> snoop = new HashMap<>();     // "switch|nhóm" -> cổng có thành viên
    final Map<String, List<String>> mroute = new HashMap<>();    // nhóm -> các switch (LAN) có thành viên
    final Map<String, List<String>> member = new HashMap<>();    // host -> các nhóm đã tham gia
    Map<String, Map<String, Integer>> dist = new HashMap<>();
    Map<String, Map<String, String>> nh = new HashMap<>();
    boolean hopTrace = true;
    BiConsumer<String, Packet> onDeliver;
    TriConsumer onLoss;
    final List<Object> hops = new ArrayList<>();                 // dữ liệu cho hoạt hình HTML
    final List<Object> notes = new ArrayList<>();
    final List<Object> changes = new ArrayList<>();

    interface TriConsumer { void accept(Packet p, String a, String b); }

    Mang(int seed) {
        rng = new Rng(seed);
        for (Object[] d : NODE_DEFS) {
            String name = (String) d[0];
            order.add(name);
            nodes.put(name, new Node(name, (String) d[1], (String) d[2], (String) d[3], (Integer) d[4], (Integer) d[5]));
        }
        for (Object[] d : LINK_DEFS) {
            String a = (String) d[0], b = (String) d[1];
            Link l = new Link(a, b, (Integer) d[2], (Long) d[3], (Long) d[4]);
            links.put(a + "|" + b, l);
            links.put(b + "|" + a, l);
            adj.computeIfAbsent(a, k -> new ArrayList<>()).add(new Nbr(b, l));
            adj.computeIfAbsent(b, k -> new ArrayList<>()).add(new Nbr(a, l));
        }
        computeRoutes();
    }

    Link link(String a, String b) { return links.get(a + "|" + b); }
    List<String> list(Map<String, List<String>> m, String k) { return m.computeIfAbsent(k, x -> new ArrayList<>()); }

    void at(long t, Runnable fn) { q.add(new Event(t, ++seq, fn)); }

    void run() {
        while (!q.isEmpty()) {
            Event e = q.poll();
            now = e.t();
            e.fn().run();
        }
    }

    void log(String text, String node) {
        System.out.printf("%7s ms  %s%n", fmtT(now), text);
        notes.add(List.of(now, node, text));
    }

    // ---------- định tuyến: Dijkstra từ mọi nút ----------
    void computeRoutes() {
        dist = new HashMap<>();
        nh = new HashMap<>();
        for (String s : order) {
            if (!nodes.get(s).up) continue;
            Map<String, Integer> d = new HashMap<>(Map.of(s, 0));
            Map<String, String> first = new HashMap<>(Map.of(s, ""));
            Set<String> done = new HashSet<>();
            while (true) {
                String u = null;
                for (String x : order)                     // chọn nút chưa xét có khoảng cách nhỏ nhất
                    if (d.containsKey(x) && !done.contains(x) && (u == null || d.get(x) < d.get(u))) u = x;
                if (u == null) break;
                done.add(u);
                if (!u.equals(s) && nodes.get(u).kind.equals("host")) continue;   // host không chuyển tiếp cho nút khác
                for (Nbr e : adj.get(u)) {
                    if (!e.l().up || !nodes.get(e.name()).up) continue;
                    int nd = d.get(u) + e.l().cost;
                    Integer old = d.get(e.name());
                    if (old == null || nd < old) {
                        d.put(e.name(), nd);
                        first.put(e.name(), u.equals(s) ? e.name() : first.get(u));
                    }
                }
            }
            dist.put(s, d);
            nh.put(s, first);
        }
    }

    /** Chặng kế từ a tới b; "" nếu không có đường. */
    String nh(String a, String b) { return nh.getOrDefault(a, Map.of()).getOrDefault(b, ""); }
    /** Chi phí từ a tới b; null nếu không tới được. */
    Integer dist(String a, String b) { return dist.getOrDefault(a, Map.of()).get(b); }
    int cost(String a, String b) { Integer d = dist(a, b); return d == null ? 0 : d; }

    List<String> path(String a, String b) {
        List<String> p = new ArrayList<>(List.of(a));
        while (!p.get(p.size() - 1).equals(b) && !nh(p.get(p.size() - 1), b).isEmpty())
            p.add(nh(p.get(p.size() - 1), b));
        return p;
    }

    /** Đích thật của gói: với anycast là máy phục vụ gần nhất còn hoạt động. */
    String target(String node, Packet p) {
        if (!p.cast.equals("any")) return p.dst;
        String best = "";
        int bestD = 0;
        for (String h : anycast.getOrDefault(p.dst, List.of())) {
            Integer d = dist(node, h);
            if (nodes.get(h).up && d != null && (best.isEmpty() || d < bestD)) { best = h; bestD = d; }
        }
        return best;
    }

    // ---------- gửi, truyền trên liên kết, nhận ----------

    /** Host `node` phát gói vào mạng (host chỉ có một liên kết, tới switch). */
    void send(String node, Packet p) {
        nodes.get(node).sent++;
        transmit(node, adj.get(node).get(0).name(), p);
    }

    void transmit(String a, String b, Packet pkt) {
        Link l = link(a, b);
        if (!l.up || !nodes.get(b).up) {
            nodes.get(a).drop++;
            if (hopTrace) log(a + ": liên kết tới " + b + " không hoạt động, bỏ gói", a);
            return;
        }
        Packet p = pkt.copy();                             // bản sao riêng cho liên kết này
        long start = Math.max(now, l.free.get(a));
        long tx = p.size * 1000L / l.bw;                   // thời gian đẩy gói lên đường truyền
        l.free.put(a, start + tx);
        long arr = start + tx + l.delay;
        if (l.jitter > 0) arr += rng.below((int) l.jitter + 1);
        boolean lost = false;
        if (p.dropA.equals(a) && p.dropB.equals(b)) {
            lost = true;
            p.dropA = ""; p.dropB = "";
        } else if (l.loss > 0 && rng.below(1000) < l.loss) {
            lost = true;
        }
        hops.add(List.of(start, arr, a, b, p.kind, p.label, lost));
        if (lost) at((start + arr) / 2, () -> lost(p, a, b));
        else {
            long t = arr;
            at(t, () -> arrive(b, p, a));
        }
    }

    void lost(Packet p, String a, String b) {
        nodes.get(a).drop++;
        if (onLoss != null) onLoss.accept(p, a, b);
        else log("× " + p.label + ": mất trên " + a + "→" + b, a);
    }

    void arrive(String node, Packet p, String frm) {
        Node n = nodes.get(node);
        n.recv++;
        if (hopTrace) log(String.format("%3s → %-3s %s", frm, node, p.label), node);
        switch (n.kind) {
            case "host" -> hostIn(n, p);
            case "switch" -> switchIn(n, p, frm);
            default -> routerIn(n, p, frm);
        }
    }

    void hostIn(Node n, Packet p) {
        boolean mine = switch (p.cast) {
            case "uni" -> p.dst.equals(n.name);
            case "any" -> anycast.getOrDefault(p.dst, List.of()).contains(n.name);
            case "bc" -> true;                             // khung quảng bá tới được thì thuộc LAN này
            case "mc" -> member.getOrDefault(n.name, List.of()).contains(p.dst);
            default -> false;
        };
        if (mine) {
            n.deliv++;
            if (onDeliver != null) onDeliver.accept(n.name, p);
        } else {
            n.drop++;
            if (hopTrace) log(n.name + " bỏ qua: gói không dành cho mình", n.name);
        }
    }

    void switchIn(Node n, Packet p, String frm) {
        List<String> outs = new ArrayList<>();
        switch (p.cast) {
            case "bc" -> {
                for (Nbr e : adj.get(n.name)) if (!e.name().equals(frm)) outs.add(e.name());   // gửi tràn
            }
            case "igmp" -> {
                List<String> mem = list(snoop, n.name + "|" + p.dst);
                if (p.join && !mem.contains(frm)) mem.add(frm);
                if (!p.join) mem.remove(frm);
                for (Nbr e : adj.get(n.name)) if (nodes.get(e.name()).kind.equals("router")) outs.add(e.name());
            }
            case "mc" -> {
                List<String> mem = list(snoop, n.name + "|" + p.dst);
                for (Nbr e : adj.get(n.name))
                    if (!e.name().equals(frm) && (mem.contains(e.name()) || nodes.get(e.name()).kind.equals("router")))
                        outs.add(e.name());
            }
            default -> {
                String t = target(n.name, p);
                if (!t.isEmpty() && !nh(n.name, t).isEmpty()) outs.add(nh(n.name, t));
            }
        }
        for (String v : outs) {
            n.fwd++;
            transmit(n.name, v, p);
        }
    }

    void routerIn(Node n, Packet p, String frm) {
        if (p.cast.equals("bc")) {
            n.drop++;
            log(n.name + " chặn quảng bá: không chuyển ra ngoài LAN " + nodes.get(frm).lan, n.name);
            return;
        }
        if (p.cast.equals("igmp")) {
            n.deliv++;
            List<String> lans = list(mroute, p.dst);
            if (p.join && !lans.contains(frm)) {
                lans.add(frm);
                log(n.name + " ghi nhận: LAN " + nodes.get(frm).lan + " có thành viên " + p.dst, n.name);
            } else if (!p.join && list(snoop, frm + "|" + p.dst).isEmpty() && lans.contains(frm)) {
                lans.remove(frm);
                log(n.name + " ghi nhận: LAN " + nodes.get(frm).lan + " hết thành viên " + p.dst, n.name);
            }
            return;
        }
        p.ttl--;
        if (p.ttl <= 0) {
            n.drop++;
            log(n.name + " bỏ gói: TTL = 0", n.name);
            return;
        }
        List<String> outs = new ArrayList<>();
        if (p.cast.equals("mc")) {
            for (String sw : list(mroute, p.dst)) {
                String hop = nh(n.name, sw);
                if (!hop.isEmpty() && !hop.equals(frm) && !outs.contains(hop)) outs.add(hop);
            }
            if (outs.isEmpty()) log(n.name + " không chuyển tiếp: phía sau không có thành viên", n.name);
        } else {
            String t = target(n.name, p);
            if (!t.isEmpty() && !nh(n.name, t).isEmpty()) outs.add(nh(n.name, t));
            else {
                n.drop++;
                log(n.name + " bỏ gói: không có đường tới " + p.dst, n.name);
            }
        }
        for (String v : outs) {
            n.fwd++;
            transmit(n.name, v, p);
        }
    }

    // ---------- tiện ích cho kịch bản ----------
    void setLink(String a, String b, boolean up) {
        link(a, b).up = up;
        changes.add(List.of(now, "link", a + "-" + b, up));
        computeRoutes();
    }

    void setNode(String name, boolean up) {
        nodes.get(name).up = up;
        changes.add(List.of(now, "node", name, up));
        computeRoutes();
    }

    void summary() {
        System.out.println("Tổng kết theo nút (Gửi = phát ra, Nhận = gói tới, Giao = đưa lên");
        System.out.println("ứng dụng, Ch.tiếp = số bản sao chuyển đi, Bỏ = bỏ qua/chặn/mất):");
        System.out.printf("  %-4s %-13s %4s %5s %5s %8s %4s%n", "Nút", "Địa chỉ", "Gửi", "Nhận", "Giao", "Ch.tiếp", "Bỏ");
        for (String name : order) {
            Node n = nodes.get(name);
            System.out.printf("  %-4s %-13s %4d %5d %5d %8d %4d%n", name, n.ip, n.sent, n.recv, n.deliv, n.fwd, n.drop);
        }
    }

    /** Dữ liệu topo cho hoạt hình HTML. */
    static Map<String, Object> topoJson() {
        List<Object> ns = new ArrayList<>(), ls = new ArrayList<>();
        for (Object[] d : NODE_DEFS) ns.add(Json.obj("name", d[0], "kind", d[1], "ip", d[2], "x", d[4], "y", d[5]));
        for (Object[] d : LINK_DEFS) ls.add(Json.obj("a", d[0], "b", d[1], "cost", d[2]));
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("nodes", ns);
        m.put("links", ls);
        return m;
    }
}
