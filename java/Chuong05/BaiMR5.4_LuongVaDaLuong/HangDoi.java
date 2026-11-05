import java.io.BufferedReader;
import java.io.PrintWriter;
import java.net.Socket;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài mở rộng MR5.4 (d) – Nhà sản xuất – người tiêu dùng trên một kết nối TCP.
 *   luồng "nhận"     : gửi "PHAT 9", đọc từng dòng "GOI k dữ-liệu", offer()/put() vào ArrayBlockingQueue(4) –
 *                      đầy thì put() CHẶN: áp lực ngược, không đọc thêm từ socket;
 *   3 luồng "xử-lý-k": take() một gói, xử lý 80 ms + tính tổng byte, put() kết quả vào hàng đợi kết quả;
 *   luồng "gửi"      : ghi "KQ k tổng" vào CÙNG socket (một luồng đọc, một luồng ghi: an toàn).
 * Kết thúc êm bằng "viên thuốc độc" (BlockingQueue không nhận null nên dùng một đối tượng đánh dấu).
 * (Cách khác: Thread.interrupt() – take() đang chờ ném InterruptedException – khi cần dừng NGAY.)
 */
final class HangDoi {
    static final int N_GOI = 9, N_THO = 3, SUC_CHUA = 4, MS_XU_LY = 80;

    record Goi(int k, String text, int v) { }

    static final Goi DUNG = new Goi(0, "", 0);   // viên thuốc độc

    static void run(String host, int port) throws Exception {
        NhatKy.say(NhatKy.f("(d) Nhà sản xuất – người tiêu dùng: luồng nhận đọc socket → hàng đợi (tối đa %d) → %d luồng xử lý → luồng gửi", SUC_CHUA, N_THO));
        BlockingQueue<Goi> viec = new ArrayBlockingQueue<>(SUC_CHUA);
        BlockingQueue<Goi> ketQua = new LinkedBlockingQueue<>();
        int[] dem = new int[N_THO + 1];
        AtomicInteger soLanCho = new AtomicInteger();
        try (Socket s = new Socket(host, port)) {
            BufferedReader r = NhatKy.doc(s);
            PrintWriter w = NhatKy.ghi(s);
            List<Thread> ths = new ArrayList<>();
            ths.add(new Thread(() -> {
                try {
                    NhatKy.log("kết nối dịch vụ, gửi PHAT " + N_GOI);
                    w.println("PHAT " + N_GOI);
                    for (int i = 0; i < N_GOI; i++) {
                        String[] p = r.readLine().split(" ");
                        Goi g = new Goi(Integer.parseInt(p[1]), p[2], 0);
                        if (!viec.offer(g)) {                       // thử thêm không chờ
                            soLanCho.incrementAndGet();
                            NhatKy.log(NhatKy.f("hàng đợi đầy (%d/%d) → chờ chỗ trống (áp lực ngược)", SUC_CHUA, SUC_CHUA));
                            viec.put(g);                            // chặn tới khi một luồng xử lý lấy bớt
                        }
                        NhatKy.log("GOI " + g.k() + " → hàng đợi");
                    }
                    NhatKy.log(NhatKy.f("hết dữ liệu → đặt %d viên thuốc độc (poison pill) vào hàng đợi", N_THO));
                    for (int i = 0; i < N_THO; i++) viec.put(DUNG);
                } catch (Exception e) { NhatKy.log("lỗi: " + e); }
            }, "nhận"));
            for (int i = 1; i <= N_THO; i++) {
                int so = i;
                ths.add(new Thread(() -> {
                    try {
                        while (true) {
                            Goi g = viec.take();
                            if (g == DUNG) {
                                NhatKy.datNguCanh("");
                                NhatKy.log("gặp viên thuốc độc → dừng");
                                ketQua.put(DUNG);
                                return;
                            }
                            NhatKy.datNguCanh("yc=" + g.k());        // ngữ cảnh cục bộ luồng: log() tự ghi "yc=k"
                            Thread.sleep(MS_XU_LY);                  // giả lập xử lý tốn thời gian
                            int v = DichVu.tongByte(g.text());
                            NhatKy.log(NhatKy.f("xử lý GOI %d: tổng byte = %d", g.k(), v));
                            dem[so]++;                               // mỗi luồng chỉ ghi ô của mình
                            ketQua.put(new Goi(g.k(), g.text(), v));
                        }
                    } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
                }, "xử-lý-" + i));
            }
            ths.add(new Thread(() -> {
                try {
                    for (int conLai = N_THO; conLai > 0; ) {
                        Goi g = ketQua.take();
                        if (g == DUNG) { conLai--; continue; }
                        w.println("KQ " + g.k() + " " + g.v());
                        NhatKy.log("gửi KQ " + g.k() + " " + g.v());
                    }
                    NhatKy.log(NhatKy.f("cả %d luồng xử lý đã dừng → hết việc gửi", N_THO));
                } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
            }, "gửi"));
            long t = System.nanoTime();
            for (Thread th : ths) th.start();
            for (Thread th : ths) th.join();
            String traLoi = r.readLine();
            double dt = (System.nanoTime() - t) / 1e9;
            NhatKy.log("dịch vụ trả lời: " + traLoi);
            NhatKy.say();
            NhatKy.say(NhatKy.f("  %-12s%16s", "Luồng", "Số gói đã xử lý"));
            for (int i = 1; i <= N_THO; i++) NhatKy.say(NhatKy.f("  %-12s%16d", "xử-lý-" + i, dem[i]));
            NhatKy.say(NhatKy.f("  Luồng nhận phải chờ vì hàng đợi đầy: %d lần; tổng %.2f s (1 luồng xử lý ≈ %.2f s)",
                    soLanCho.get(), dt, N_GOI * MS_XU_LY / 1000.0));
        }
    }
}
