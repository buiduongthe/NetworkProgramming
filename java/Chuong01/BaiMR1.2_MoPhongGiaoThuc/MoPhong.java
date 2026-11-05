import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR1.2 – Mô phỏng giao thức, kiểu truyền và mô hình máy chủ: TCP, UDP, unicast, broadcast,
 * multicast, anycast; đồng bộ, đa luồng, nhóm luồng, bất đồng bộ, phối hợp (MayChu.java).
 *
 * Bài mở rộng, không có trong giáo trình; minh họa trực quan Chương 1 (mô hình TCP/IP, tầng giao vận,
 * các kiểu địa chỉ) trước khi lập trình socket thật ở các chương sau. Toàn bộ chạy trong bộ mô phỏng
 * sự kiện rời rạc (Mang.java): không mở socket, không cần quyền quản trị, kết quả tất định theo seed.
 *
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out MoPhong <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]
 *   kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast            (giao thức, kiểu truyền)
 *             dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop   (bên trong máy chủ)
 *             so_sanh (bảng so sánh 5 mô hình máy chủ) | tat_ca
 *   --seed  hạt giống bộ sinh ngẫu nhiên (mặc định 7)
 *   --loss  xác suất mất gói trên liên kết R2–R3, 0..1 (mặc định: 0; UDP: 0.2)
 *   --doan  số đoạn dữ liệu TCP (mặc định 40)
 *   --mat   các đoạn TCP bị ép mất; lặp số đoạn để mất nhiều lần (mặc định 20,20)
 *   --html  ghi tệp HTML hoạt hình tự chứa (JavaScript thuần, không cần mạng)
 */
public class MoPhong {
    static final Map<String, String> TITLES = new LinkedHashMap<>();
    static {
        TITLES.put("tcp", "TCP – bắt tay, cửa sổ trượt, cwnd, mất gói, đóng FIN");
        TITLES.put("udp", "UDP – datagram, mất gói, sai thứ tự, không gửi lại");
        TITLES.put("unicast", "Unicast – một nguồn tới một đích");
        TITLES.put("broadcast", "Broadcast – gửi tràn trong LAN, dừng ở router");
        TITLES.put("multicast", "Multicast – IGMP, chỉ giao cho thành viên nhóm");
        TITLES.put("anycast", "Anycast – máy phục vụ gần nhất, dự phòng khi hỏng");
        TITLES.putAll(MayChu.TITLES);
    }

    static int seed = 7, doan = 40;
    static double loss = -1;
    static String mat = "20,20", html = "";

    static Map<String, Object> runOne(String name) {
        System.out.printf("=== Kịch bản %s (seed %d) ===%n", name, seed);
        System.out.println(TITLES.get(name));
        Mang n = new Mang(seed);
        boolean lossSet = loss >= 0;
        if (lossSet) n.link("R2", "R3").loss = (int) (loss * 1000 + 0.5);
        Map<String, Object> rd = Json.obj("id", name, "title", TITLES.get(name));
        if (MayChu.TITLES.containsKey(name)) {             // kịch bản bên trong máy chủ: không in bảng theo nút
            Map<String, Object> extra = MayChu.chay(n, name);
            System.out.println();
            rd.put("hops", n.hops);
            rd.put("notes", n.notes);
            rd.put("changes", n.changes);
            rd.putAll(extra);
            return rd;
        }
        List<Object> cwnd = null;
        switch (name) {
            case "tcp" -> {
                List<Integer> drops = new ArrayList<>();
                List<String> shown = new ArrayList<>();
                for (String x : mat.split(",")) {
                    if (x.strip().isEmpty()) continue;
                    int k = Integer.parseInt(x.strip());
                    drops.add(k);
                    shown.add("#" + k);
                }
                System.out.printf("Truyền %d đoạn × %d byte từ H1 tới H5; ép mất đoạn: %s%n", doan, TcpKichBan.MSS,
                        shown.isEmpty() ? "không" : String.join(", ", shown));
                TcpKichBan sc = new TcpKichBan(n, doan, drops);
                sc.start();
                n.run();
                sc.report();
                cwnd = new ArrayList<>();
                for (long[] p : sc.cwndLog) cwnd.add(List.of(p[0], p[1]));
            }
            case "udp" -> KichBan.udp(n, lossSet);
            case "unicast" -> KichBan.unicast(n);
            case "broadcast" -> KichBan.broadcast(n);
            case "multicast" -> KichBan.multicast(n);
            default -> KichBan.anycast(n);
        }
        System.out.println();
        n.summary();
        System.out.println();
        rd.put("hops", n.hops);
        rd.put("notes", n.notes);
        rd.put("changes", n.changes);
        if (cwnd != null) rd.put("cwnd", cwnd);
        return rd;
    }

    /** Chèn dữ liệu JSON vào khung hoạt hình; thay "</" để chuỗi không đóng nhầm thẻ <script>. */
    static void writeHtml(String path, List<Object> runs) throws Exception {
        Map<String, Object> data = Mang.topoJson();
        data.put("runs", runs);
        String tpl = Files.readString(Path.of("khung_hoat_hinh.html"), StandardCharsets.UTF_8);
        String js = Json.stringify(data).replace("</", "<\\/");
        Files.writeString(Path.of(path), tpl.replace("{{.Data}}", js), StandardCharsets.UTF_8);
        System.out.println("Đã ghi hoạt hình HTML: " + path);
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1 || (!TITLES.containsKey(args[0]) && !args[0].equals("so_sanh") && !args[0].equals("tat_ca"))) {
            System.out.println("Cách dùng: java -cp out MoPhong <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]");
            System.out.println("  kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast");
            System.out.println("            dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop | so_sanh | tat_ca");
            System.exit(1);
        }
        for (int i = 1; i + 1 < args.length; i += 2) {
            String v = args[i + 1];
            switch (args[i]) {
                case "--seed" -> seed = Integer.parseInt(v);
                case "--loss" -> loss = Double.parseDouble(v);
                case "--doan" -> doan = Integer.parseInt(v);
                case "--mat" -> mat = v;
                case "--html" -> html = v;
                default -> { System.out.println("Tham số không hợp lệ: " + args[i]); System.exit(1); }
            }
        }
        if (args[0].equals("so_sanh")) {
            Map<String, MayChu.ChiSo> kq = new LinkedHashMap<>();
            for (String m : MayChu.TITLES.keySet()) kq.put(m, MayChu.chiSo(new Mang(seed), m));
            MayChu.soSanh(kq);
            return;
        }
        List<String> names = args[0].equals("tat_ca") ? new ArrayList<>(TITLES.keySet()) : List.of(args[0]);
        List<Object> runs = new ArrayList<>();
        for (String name : names) runs.add(runOne(name));
        if (!html.isEmpty()) writeHtml(html, runs);
    }
}
