import java.util.ArrayList;
import java.util.List;

/** Các kịch bản UDP, unicast, broadcast, multicast, anycast trên bộ mô phỏng. */
final class KichBan {
    static final int MS = 1000;                            // 1 ms = 1000 µs

    static void showPath(Mang n, String a, String b) {
        System.out.printf("  Đường %s→%s: %s (chi phí %d)%n", a, b, String.join(" → ", n.path(a, b)), n.cost(a, b));
    }

    static Mang.Packet msgPacket(String cast, String kind, String src, String dst, String label, int size, String msg) {
        Mang.Packet p = new Mang.Packet(cast, kind, src, dst, label, size);
        p.msg = msg;
        return p;
    }

    // ---------------- UDP ----------------

    /**
     * H2 gửi 12 datagram tới H6, cách nhau 10 ms. Liên kết R2–R3 mất gói ngẫu nhiên và có trễ
     * dao động (jitter) nên datagram có thể mất hoặc đến sai thứ tự; UDP không gửi lại.
     */
    static void udp(Mang n, boolean lossSet) {
        Mang.Link l = n.link("R2", "R3");
        if (!lossSet) l.loss = 200;                        // mặc định 20 %
        l.jitter = 15 * MS;                                // trễ thêm ngẫu nhiên 0..15 ms
        n.hopTrace = false;
        System.out.printf("Liên kết R2–R3: mất gói %d.%d %%, trễ dao động 0–15 ms%n", l.loss / 10, l.loss % 10);
        showPath(n, "H2", "H6");
        List<Integer> got = new ArrayList<>();
        int[] expect = {1}, late = {0};                    // mảng 1 phần tử: biến thay đổi được trong lambda
        n.onDeliver = (node, p) -> {
            int k = p.n;
            got.add(k);
            String note;
            if (k == expect[0]) note = "đúng thứ tự";
            else if (k > expect[0]) {
                List<String> miss = new ArrayList<>();
                for (int i = expect[0]; i < k; i++) miss.add("#" + i);
                note = "đến sớm, đang thiếu " + String.join(", ", miss);
            } else {
                note = "ĐẾN MUỘN – sai thứ tự";
                late[0]++;
            }
            expect[0] = Math.max(expect[0], k + 1);
            n.log(String.format("H6 nhận datagram #%-2d %s", k, note), node);
        };
        for (int i = 1; i <= 12; i++) {
            final int k = i;
            n.at((long) (i - 1) * 10 * MS, () -> {
                n.log("H2 gửi datagram #" + k + " (200 byte)", "H2");
                Mang.Packet p = new Mang.Packet("uni", "UDP", "H2", "H6", "UDP #" + k, 200);
                p.n = k;
                n.send("H2", p);
            });
        }
        n.run();
        List<String> lost = new ArrayList<>();
        for (int i = 1; i <= 12; i++) if (!got.contains(i)) lost.add("#" + i);
        String ls = lost.isEmpty() ? "không" : String.join(", ", lost);
        System.out.println();
        System.out.printf("Tóm tắt UDP: gửi 12, nhận %d, mất %d (%s), sai thứ tự %d%n", got.size(), lost.size(), ls, late[0]);
        System.out.println("  UDP không xác nhận, không gửi lại, không sắp xếp lại:");
        System.out.println("  ứng dụng tự xử lý (đánh số thứ tự, bỏ qua hoặc yêu cầu lại).");
    }

    // ---------------- Unicast ----------------
    static void unicast(Mang n) {
        System.out.println("Bảng định tuyến của R1 (đích → chặng kế, chi phí):");
        for (String h : List.of("H2", "H5"))
            System.out.printf("  %-3s → %-3s chi phí %d%n", h, n.nh("R1", h), n.cost("R1", h));
        showPath(n, "H1", "H2");
        showPath(n, "H1", "H5");
        n.onDeliver = (node, p) -> {
            n.log(node + " nhận \"" + p.msg + "\" từ " + p.src + " (TTL còn " + p.ttl + ")", node);
            if (p.reply) n.send(node, msgPacket("uni", "UNI", node, p.src, "UNI " + node + "→" + p.src, 100, "chào " + p.src));
        };
        n.at(0, () -> n.send("H1", msgPacket("uni", "UNI", "H1", "H2", "UNI H1→H2", 100, "xin chào H2")));
        n.at(5 * MS, () -> {
            Mang.Packet p = msgPacket("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, "xin chào H5");
            p.reply = true;
            n.send("H1", p);
        });
        n.at(50 * MS, () -> {
            n.log("Liên kết R1–R2 hỏng → các router tính lại đường", "R1");
            n.setLink("R1", "R2", false);
            showPath(n, "H1", "H5");
            n.send("H1", msgPacket("uni", "UNI", "H1", "H5", "UNI H1→H5", 100, "lần 2"));
        });
        n.run();
        System.out.println();
        System.out.println("Tóm tắt unicast: mỗi gói chỉ tới đúng một đích; switch chỉ");
        System.out.println("  chuyển ra đúng một cổng; router giảm TTL, chọn chặng kế theo");
        System.out.println("  chi phí nhỏ nhất; đường đổi khi liên kết hỏng.");
    }

    // ---------------- Broadcast ----------------
    static void broadcast(Mang n) {
        n.onDeliver = (node, p) -> n.log(node + " nhận quảng bá \"" + p.msg + "\" từ " + p.src, node);
        n.at(0, () -> n.send("H1", msgPacket("bc", "BC", "H1", "192.168.1.255", "BC 192.168.1.255", 60, "Ai có 192.168.1.13?")));
        n.at(10 * MS, () -> n.send("H4", msgPacket("bc", "BC", "H4", "255.255.255.255", "BC 255.255.255.255", 60, "DHCP DISCOVER")));
        n.run();
        System.out.println();
        System.out.println("Tóm tắt broadcast: switch gửi tràn mọi cổng trong LAN; router");
        System.out.println("  chặn nên H4, H5, H6 không nhận quảng bá của H1 và LAN A không");
        System.out.println("  nhận quảng bá của H4 (miền quảng bá = một LAN).");
    }

    // ---------------- Multicast ----------------
    static final String G = "239.1.1.1";

    static Runnable join(Mang n, String h, boolean yes) {
        return () -> {
            List<String> groups = n.list(n.member, h);
            if (yes) groups.add(G); else groups.remove(G);
            n.log(h + " " + (yes ? "tham gia" : "rời") + " nhóm " + G + ": gửi IGMP " + (yes ? "Report" : "Leave"), h);
            Mang.Packet p = new Mang.Packet("igmp", "IGMP", h, G, "IGMP " + (yes ? "Report" : "Leave") + " " + G, 60);
            p.join = yes;
            n.send(h, p);
        };
    }

    static Runnable stream(Mang n, int k) {
        return () -> {
            n.log("H1 gửi luồng #" + k + " tới nhóm " + G, "H1");
            n.send("H1", msgPacket("mc", "MC", "H1", G, "MC " + G + " #" + k, 500, "luồng #" + k));
        };
    }

    static void multicast(Mang n) {
        n.onDeliver = (node, p) -> n.log(node + " nhận " + p.msg + " của nhóm " + G, node);
        n.at(0, join(n, "H2", true));
        n.at(5 * MS, stream(n, 1));
        n.at(20 * MS, join(n, "H5", true));
        n.at(40 * MS, stream(n, 2));
        n.at(70 * MS, join(n, "H6", true));
        n.at(75 * MS, join(n, "H2", false));
        n.at(90 * MS, stream(n, 3));
        n.run();
        System.out.println();
        System.out.println("Tóm tắt multicast: chỉ thành viên nhóm nhận dữ liệu; switch");
        System.out.println("  (IGMP snooping) chỉ chuyển ra cổng có thành viên; router chỉ");
        System.out.println("  chuyển sang LAN có thành viên; mỗi liên kết chỉ mang một bản");
        System.out.println("  sao dù phía sau có nhiều thành viên.");
    }

    // ---------------- Anycast ----------------
    static final String A = "10.53.0.53";

    static Runnable query(Mang n, String c, int k) {
        return () -> {
            n.log(c + " gửi truy vấn #" + k + " tới " + A, c);
            n.send(c, msgPacket("any", "ANY", c, A, "ANY " + c + "→" + A, 80, "truy vấn #" + k));
        };
    }

    static void anycast(Mang n) {
        n.anycast.put(A, new ArrayList<>(List.of("H3", "H6")));
        System.out.printf("Địa chỉ anycast %s (DNS) do H3 và H6 cùng quảng bá.%n", A);
        for (String c : List.of("H1", "H5")) {
            List<String> costs = new ArrayList<>();
            for (String s : n.anycast.get(A)) costs.add(s + " chi phí " + n.cost(c, s));
            System.out.printf("  %s: %s → chọn %s%n", c, String.join(", ", costs),
                    n.target(c, new Mang.Packet("any", "ANY", c, A, "", 0)));
        }
        n.onDeliver = (node, p) -> {
            if (p.cast.equals("any")) {
                n.log(node + " (phục vụ " + A + ") nhận \"" + p.msg + "\" từ " + p.src, node);
                n.send(node, msgPacket("uni", "UNI", node, p.src, "UNI " + node + "→" + p.src, 120, "trả lời từ " + node));
            } else {
                n.log(node + " nhận \"" + p.msg + "\"", node);
            }
        };
        n.at(0, query(n, "H1", 1));
        n.at(2 * MS, query(n, "H5", 1));
        n.at(40 * MS, () -> {
            n.log("H3 ngừng hoạt động → rút quảng bá " + A, "H3");
            n.setNode("H3", false);
            String best = n.target("H1", new Mang.Packet("any", "ANY", "H1", A, "", 0));
            System.out.printf("  H1: còn H6 chi phí %d → chọn %s%n", n.cost("H1", "H6"), best);
        });
        n.at(41 * MS, query(n, "H1", 2));
        n.run();
        System.out.println();
        System.out.println("Tóm tắt anycast: cùng một địa chỉ, gói tới máy phục vụ gần nhất");
        System.out.println("  theo chi phí định tuyến; khi H3 hỏng, truy vấn của H1 được");
        System.out.println("  chuyển sang H6 (dự phòng tự động).");
    }
}
