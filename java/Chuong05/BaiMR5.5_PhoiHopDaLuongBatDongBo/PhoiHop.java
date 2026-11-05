import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

/**
 * Bài mở rộng MR5.5 – Phối hợp đa luồng và bất đồng bộ trong server mạng (Java).
 * Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6), Chương 6 (Bài 6.1, 6.2), MR5.4.
 * Server bất đồng bộ (vòng lặp Selector một luồng, VongLap.java) phải:
 *   (a) đẩy việc CPU (băm SHA-256) sang nhóm luồng riêng bằng CompletableFuture.supplyAsync;
 *   (b) gọi hàm thư viện cũ kiểu CHẶN an toàn (luồng ảo);
 *   (c) nhận kết quả từ luồng nền (cảm biến): vongLap.execute(...), CompletableFuture.complete() từ luồng khác;
 *   (d) truyền hủy, hạn chót qua ranh giới (orTimeout + cờ AtomicBoolean);
 *   (e) áp lực ngược giữa vòng lặp và nhóm luồng (đếm suất ở cổng vào + hàng đợi chờ);
 *   (f) cách làm SAI – việc CPU ngay trên vòng lặp – đo bằng nhịp tim 10 ms.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out PhoiHop server|tai host port
 */
public class PhoiHop {
    static void tai(String host, int port) throws Exception {
        byte[] data = Chung.duLieu(Chung.SO_BYTE);
        Chung.say(Chung.f("Máy tạo tải: %d yêu cầu đồng thời mỗi đợt, mỗi yêu cầu %d KB, băm SHA-256 %d vòng (≈ %.0f MB dữ liệu băm)",
                Chung.N_YC, Chung.SO_BYTE / 1024, Chung.SO_VONG, (double) Chung.SO_BYTE * Chung.SO_VONG / 1e6));
        String[][] cheDo = {{"chan", "A. CPU ngay trên vòng lặp"}, {"tach", "B. đẩy sang nhóm luồng"},
                {"gioihan", "C. nhóm luồng + áp lực ngược"}};
        List<Object[]> bang = new ArrayList<>();
        String mauHex = null;
        for (String[] cd : cheDo) {
            Chung.gui(host, port, "DATLAI", null);
            long t = System.nanoTime();
            List<Future<String>> fs = new ArrayList<>();
            try (ExecutorService ex = Executors.newVirtualThreadPerTaskExecutor()) {   // mỗi yêu cầu một luồng ảo
                for (int k = 1; k <= Chung.N_YC; k++) {
                    String line = Chung.f("BAM %d %s %d %d 10000", k, cd[0], Chung.SO_BYTE, Chung.SO_VONG);
                    fs.add(ex.submit(() -> Chung.gui(host, port, line, data)));
                }
            }
            double dt = (System.nanoTime() - t) / 1e9;
            int ok = 0;
            Set<String> hex = new HashSet<>();
            for (Future<String> f : fs) {
                String[] p = f.get().split(" ");
                if (p[0].equals("OK")) { ok++; hex.add(p[2]); if (mauHex == null) mauHex = p[2]; }
            }
            String[] tk = Chung.gui(host, port, "THONGKE " + cd[1], null).split(" ");
            Chung.say(Chung.f("Đợt %s: %d/%d OK, %.2f s, %.1f yc/s, %d giá trị băm khác nhau", cd[1], ok, Chung.N_YC, dt, Chung.N_YC / dt, hex.size()));
            bang.add(new Object[] {cd[1], dt, tk});
        }
        Chung.say("");
        Chung.say(Chung.f("  %-18s%10s%7s%10s%10s%6s", "Chế độ", "Thời gian", "yc/s", "Trễ nhịp", "Cảm biến", "Nhóm"));
        String[] ngan = {"A. chặn vòng lặp", "B. nhóm luồng", "C. + áp lực ngược"};   // nhãn ngắn trong bảng
        for (int i = 0; i < bang.size(); i++) {
            String[] tk = (String[]) bang.get(i)[2];
            double dt = (double) bang.get(i)[1];
            Chung.say(Chung.f("  %-18s%8.2f s%7.1f%7.1f ms%7.1f ms%6d", ngan[i], dt, Chung.N_YC / dt,
                    Double.parseDouble(tk[1]), Double.parseDouble(tk[3]), Integer.parseInt(tk[4])));
        }
        Chung.say("  (Trễ nhịp, Cảm biến: độ trễ tối đa của nhịp tim 10 ms và của");
        Chung.say("   mẫu cảm biến; Nhóm: số việc ở nhóm tính cùng lúc, tối đa)");
        Chung.say("Băm kiểm tra (16 hex đầu): " + mauHex + " – giống nhau ở mọi bản ngôn ngữ");
        Chung.say("Nhận xét: A chặn vòng lặp → nhịp tim, mẫu cảm biến và mọi kết nối khác đều đứng chờ; B, C giữ");
        Chung.say(Chung.f("  vòng lặp thông suốt; C chỉ cho %d việc vào nhóm %d luồng, phần còn lại chờ", Chung.GIOI_HAN, Chung.NHOM_TINH));
        Chung.say("  bất đồng bộ ở cổng vào – hàng đợi của nhóm luồng không phình theo tải.");
        Chung.say("");
        Chung.say("(d) Hạn chót và hủy xuyên ranh giới async → luồng (chế độ tach):");
        int[][] hc = {{101, Chung.SO_VONG, 1000}, {102, Chung.SO_VONG * 8, 30}};
        for (int[] x : hc) {
            long t = System.nanoTime();
            String tl = Chung.gui(host, port, Chung.f("BAM %d tach %d %d %d", x[0], Chung.SO_BYTE, x[1], x[2]), data);
            Chung.say(Chung.f("  yêu cầu %d (%d vòng, hạn %d ms): %s sau %.0f ms", x[0], x[1], x[2], tl.split(" ")[0], (System.nanoTime() - t) / 1e6));
        }
        Chung.say("");
        Chung.say("(c) Luồng nền giao kết quả vào vòng lặp sự kiện:");
        for (int ma : new int[] {201, 202}) {
            String[] tl = Chung.gui(host, port, "CAMBIEN " + ma, null).split(" ");
            Chung.say(Chung.f("  CAMBIEN %d → giá trị %s, future do luồng %s hoàn tất", ma, tl[2], tl[3]));
        }
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3 || !(args[0].equals("server") || args[0].equals("tai"))) {
            Chung.say("Cách dùng: java -cp out PhoiHop server|tai <host> <port>");
            System.exit(1);
        }
        if (args[0].equals("server")) MayChu.run(args[1], Integer.parseInt(args[2]));
        else tai(args[1], Integer.parseInt(args[2]));
    }
}
