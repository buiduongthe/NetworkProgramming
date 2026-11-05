import java.io.BufferedReader;
import java.io.IOException;
import java.io.PrintWriter;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài mở rộng MR5.4 (b) – Server TCP đa luồng: mỗi client một luồng và nhóm luồng cố định.
 *   cổng P   – máy chủ A: luồng "nhận-A" accept(), mỗi kết nối một Thread MỚI "kết-nối-k";
 *   cổng P+1 – máy chủ B: ThreadPoolExecutor 2 luồng cố định "nhóm-1", "nhóm-2", hàng đợi ArrayBlockingQueue(2),
 *              chính sách AbortPolicy: execute() ném RejectedExecutionException khi đầy → trả "503 BAN mã".
 * Vai trò tai: 6 luồng "khách-k" (cách nhau 20 ms) gửi "VIEC k 300" tới từng máy chủ.
 */
final class MayChu {
    static final int N_KHACH = 6, MS_VIEC = 300, CACH_MS = 20, NHOM = 2, HANG_DOI = 2;

    // Phần việc giống nhau ở hai máy chủ: chờ ms rồi trả lời kèm tên luồng đang chạy
    static void xuLy(PrintWriter w, String[] p) throws InterruptedException {
        NhatKy.log("xử lý VIEC " + p[1] + " " + p[2]);
        Thread.sleep(Integer.parseInt(p[2]));
        w.println("XONG " + p[1] + " " + Thread.currentThread().getName());
    }

    static String[] docYeuCau(BufferedReader r) throws IOException {
        String line = r.readLine();
        if (line == null) return null;
        String[] p = line.trim().split(" ");
        return p.length == 3 && p[0].equals("VIEC") ? p : null;
    }

    static void mayChuA(ServerSocket ss) {
        try {
            for (int dem = 1; ; dem++) {
                Socket s = ss.accept();
                NhatKy.log("kết nối mới → tạo luồng kết-nối-" + dem);
                Thread t = new Thread(() -> {
                    try (s) {
                        String[] p = docYeuCau(NhatKy.doc(s));
                        if (p != null) xuLy(NhatKy.ghi(s), p);
                    } catch (IOException | InterruptedException e) { NhatKy.log("lỗi: " + e.getMessage()); }
                }, "kết-nối-" + dem);
                t.setDaemon(true);
                t.start();
            }
        } catch (IOException e) { NhatKy.log("dừng: " + e.getMessage()); }
    }

    static void mayChuB(ServerSocket ss) throws IOException {
        AtomicInteger soLuong = new AtomicInteger();
        ThreadPoolExecutor nhom = new ThreadPoolExecutor(NHOM, NHOM, 0, TimeUnit.SECONDS,
                new ArrayBlockingQueue<>(HANG_DOI),                         // hàng đợi có giới hạn
                r -> { Thread t = new Thread(r, "nhóm-" + soLuong.incrementAndGet()); t.setDaemon(true); return t; },
                new ThreadPoolExecutor.AbortPolicy());                      // đầy → ném ngoại lệ, không chờ
        while (true) {
            Socket s = ss.accept();
            String[] p = docYeuCau(NhatKy.doc(s));
            if (p == null) { s.close(); continue; }
            PrintWriter w = NhatKy.ghi(s);
            try {
                nhom.execute(() -> {
                    try (s) { xuLy(w, p); } catch (IOException | InterruptedException e) { NhatKy.log("lỗi: " + e); }
                });
                NhatKy.log("VIEC " + p[1] + " vào hàng đợi");
            } catch (RejectedExecutionException e) {
                NhatKy.log(NhatKy.f("hàng đợi đầy (%d/%d) → trả 503 cho VIEC %s", HANG_DOI, HANG_DOI, p[1]));
                w.println("503 BAN " + p[1]);
                s.close();
            }
        }
    }

    static void runServer(String host, int pa) throws Exception {
        ServerSocket a = new ServerSocket(pa, 64, InetAddress.getByName(host));
        ServerSocket b = new ServerSocket(pa + 1, 64, InetAddress.getByName(host));
        NhatKy.say(NhatKy.f("[server] cổng %d: máy chủ A – mỗi client một luồng", pa));
        NhatKy.say(NhatKy.f("[server] cổng %d: máy chủ B – nhóm %d luồng cố định, hàng đợi tối đa %d (đầy → 503)", pa + 1, NHOM, HANG_DOI));
        Thread ta = new Thread(() -> mayChuA(a), "nhận-A");
        ta.setDaemon(true);
        ta.start();
        Thread tb = new Thread(() -> {
            try { mayChuB(b); } catch (IOException e) { NhatKy.log("dừng: " + e.getMessage()); }
        }, "nhận-B");
        tb.start();
        tb.join();
    }

    // ---------------- Máy tạo tải ----------------
    static void khach(String host, int port, int k, String[] kq) {
        try {
            Thread.sleep((long) (k - 1) * CACH_MS);   // khách k xuất phát muộn hơn khách 1 là (k-1)×20 ms
            long t = System.nanoTime();
            try (Socket s = new Socket(host, port)) {
                NhatKy.ghi(s).println("VIEC " + k + " " + MS_VIEC);
                String line = NhatKy.doc(s).readLine();
                double dt = (System.nanoTime() - t) / 1e9;
                kq[k - 1] = line;
                String[] p = line.split(" ");
                if (p[0].equals("XONG")) NhatKy.log(NhatKy.f("XONG %d bởi %s sau %.2f s", k, p[2], dt));
                else NhatKy.log(NhatKy.f("%s (bị từ chối) sau %.2f s", line, dt));
            }
        } catch (Exception e) { NhatKy.log("lỗi: " + e.getMessage()); }
    }

    static double[] motDot(String host, int port) throws InterruptedException {
        String[] kq = new String[N_KHACH];
        long t = System.nanoTime();
        List<Thread> ths = new ArrayList<>();
        for (int k = 1; k <= N_KHACH; k++) {
            int kk = k;
            ths.add(new Thread(() -> khach(host, port, kk, kq), "khách-" + k));
        }
        for (Thread th : ths) th.start();
        for (Thread th : ths) th.join();
        int ok = 0;
        for (String x : kq) if (x != null && x.startsWith("XONG")) ok++;
        return new double[] {ok, N_KHACH - ok, (System.nanoTime() - t) / 1e9};
    }

    static void runTai(String host, int pa) throws InterruptedException {
        NhatKy.say(NhatKy.f("(b) Máy chủ đa luồng: %d client đồng thời (cách nhau %d ms), mỗi yêu cầu xử lý %d ms", N_KHACH, CACH_MS, MS_VIEC));
        NhatKy.say(NhatKy.f("Máy chủ A – mỗi client một luồng (cổng %d):", pa));
        double[] a = motDot(host, pa);
        NhatKy.say(NhatKy.f("Máy chủ B – nhóm %d luồng, hàng đợi %d (cổng %d):", NHOM, HANG_DOI, pa + 1));
        double[] b = motDot(host, pa + 1);
        NhatKy.say();
        NhatKy.say(NhatKy.f("  %-29s%11s%9s%10s", "Máy chủ", "Thành công", "Từ chối", "Thời gian"));
        NhatKy.say(NhatKy.f("  %-29s%11d%9d%8.2f s", "A. mỗi client một luồng", (int) a[0], (int) a[1], a[2]));
        NhatKy.say(NhatKy.f("  %-29s%11d%9d%8.2f s", NhatKy.f("B. nhóm %d luồng + hàng đợi %d", NHOM, HANG_DOI), (int) b[0], (int) b[1], b[2]));
        NhatKy.say("Nhận xét: A tạo luồng không giới hạn – nhanh khi ít client nhưng quá tải sẽ cạn bộ nhớ;");
        NhatKy.say(NhatKy.f("  B giữ số luồng cố định, chỉ nhận %d việc cùng lúc, phần vượt bị từ chối ngay (503)", NHOM + HANG_DOI));
        NhatKy.say("  thay vì xếp hàng vô hạn – client biết để thử lại sau.");
    }
}
