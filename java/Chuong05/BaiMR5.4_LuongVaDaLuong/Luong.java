import java.net.Socket;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Bài mở rộng MR5.4 – Luồng (thread) và đa luồng trong chương trình mạng (Java).
 * Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1–5.6) và Chương 6 (Bài 6.1, 6.2).
 * Khác MR5.2 (so sánh mô hình trên một khối việc), bài này đi từng bước vào chính luồng:
 *   (a) Thread: tạo – start – join, truyền tham số, lấy kết quả (Future)   → luong
 *   (b) server: mỗi client một Thread / ThreadPoolExecutor + hàng đợi giới hạn (503) → server, tai
 *   (c) trạng thái dùng chung: tranh chấp, ReentrantLock, AtomicLong, ConcurrentHashMap → khoa
 *   (d) nhà sản xuất – người tiêu dùng, ArrayBlockingQueue, poison pill   → hang-doi
 *   (e) giới hạn kết nối ra ngoài: semaphore tự làm bằng ReentrantLock + Condition → luong
 *   (f) bế tắc: tryLock có hạn, thứ tự lấy khóa                          → khoa
 *   (g) nhật ký theo luồng: tên luồng + ThreadLocal (NhatKy.java); thêm so sánh luồng ảo (Java 21)
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out Luong dich-vu|server|luong|hang-doi|tai|khoa host port
 */
public class Luong {
    static final int SO_SUAT = 3, N_GOI_RA = 8, MS_GOI_RA = 150;

    // Một lời gọi mạng kiểu chặn: kết nối, gửi một dòng, đọc một dòng trả lời
    static String goiDichVu(String host, int port, String line) throws Exception {
        try (Socket s = new Socket(host, port)) {
            NhatKy.ghi(s).println(line);
            return NhatKy.doc(s).readLine();
        }
    }

    // (a) tạo, chạy, chờ luồng; tham số và kết quả
    static void coBan(String host, int port) throws Exception {
        NhatKy.say("(a) Tạo – chạy – chờ luồng: 3 luồng, mỗi luồng nhận tham số (mã, thời gian xử lý) và trả kết quả");
        int[][] thamSo = {{1, 300}, {2, 100}, {3, 200}};
        String[] traLoi = new String[3];        // mỗi luồng ghi ô RIÊNG → không cần khóa
        double[] thoiGian = new double[3];
        NhatKy.log("tạo 3 luồng tải-1..tải-3 rồi cho chạy (start)");
        long t0 = System.nanoTime();
        List<Thread> ths = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            int o = i, ma = thamSo[i][0], ms = thamSo[i][1];   // lambda "bắt" biến final → truyền tham số
            ths.add(new Thread(() -> {
                try {
                    NhatKy.datNguCanh("yc=" + ma);
                    NhatKy.log("kết nối dịch vụ, gửi VIEC " + ma + " " + ms);
                    long t = System.nanoTime();
                    String tl = goiDichVu(host, port, "VIEC " + ma + " " + ms);
                    double dt = (System.nanoTime() - t) / 1e6;
                    NhatKy.log(NhatKy.f("nhận %s sau %.0f ms", tl, dt));
                    traLoi[o] = tl;                 // Runnable không trả giá trị → ghi vào ô dùng riêng
                    thoiGian[o] = dt;               // (muốn có giá trị trả về: Callable + Future, xem dưới)
                } catch (Exception e) { NhatKy.log("lỗi: " + e); }
            }, "tải-" + ma));
        }
        for (Thread t : ths) t.start();             // luồng bắt đầu chạy song song với luồng chính
        for (Thread t : ths) t.join();              // luồng chính chờ từng luồng kết thúc
        double tong = (System.nanoTime() - t0) / 1e9;
        NhatKy.log(NhatKy.f("chờ (join) xong cả 3 luồng sau %.2f s (tuần tự sẽ ≈ %.2f s)", tong, (thamSo[0][1] + thamSo[1][1] + thamSo[2][1]) / 1000.0));
        NhatKy.say();
        NhatKy.say(NhatKy.f("  %-8s%-16s%-10s%10s", "Luồng", "Tham số", "Kết quả", "Thời gian"));
        for (int i = 0; i < 3; i++)
            NhatKy.say(NhatKy.f("  %-8s%-16s%-10s%7.0f ms", "tải-" + thamSo[i][0],
                    "mã " + thamSo[i][0] + ", " + thamSo[i][1] + " ms", traLoi[i], thoiGian[i]));
        goiDichVu(host, port, "THONGKE");          // đặt lại thống kê của dịch vụ trước phần (e)
    }

    /** Semaphore tự làm bằng ReentrantLock + Condition (biến điều kiện) – java.util.concurrent.Semaphore làm sẵn việc này. */
    static final class GioiHan {
        private final ReentrantLock khoa = new ReentrantLock();
        private final Condition conSuat = khoa.newCondition();
        private int dangDung;
        private final int toiDa;

        GioiHan(int toiDa) { this.toiDa = toiDa; }

        /** Lấy một suất; trả về số suất đang dùng sau khi lấy. */
        int lay() throws InterruptedException {
            khoa.lock();
            try {
                if (dangDung == toiDa) NhatKy.log("hết suất → chờ");
                while (dangDung == toiDa) conSuat.await();   // nhả khóa và ngủ tới khi được signal()
                return ++dangDung;
            } finally { khoa.unlock(); }
        }

        void tra() {
            khoa.lock();
            try { dangDung--; conSuat.signal(); }            // đánh thức một luồng đang chờ
            finally { khoa.unlock(); }
        }
    }

    // (e) giới hạn kết nối ra ngoài
    static void gioiHan(String host, int port) throws Exception {
        NhatKy.say();
        NhatKy.say(NhatKy.f("(e) Giới hạn kết nối ra ngoài: %d luồng gọi dịch vụ, tối đa %d kết nối cùng lúc (semaphore)", N_GOI_RA, SO_SUAT));
        GioiHan suat = new GioiHan(SO_SUAT);
        long t0 = System.nanoTime();
        List<Thread> ths = new ArrayList<>();
        for (int k = 1; k <= N_GOI_RA; k++) {
            int kk = k;
            ths.add(new Thread(() -> {
                try {
                    NhatKy.datNguCanh("yc=" + kk);
                    int n = suat.lay();
                    NhatKy.log(NhatKy.f("được suất (đang dùng %d/%d), gửi VIEC %d %d", n, SO_SUAT, 10 + kk, MS_GOI_RA));
                    try { goiDichVu(host, port, "VIEC " + (10 + kk) + " " + MS_GOI_RA); }
                    finally { suat.tra(); }                   // luôn trả suất, kể cả khi lời gọi lỗi
                    NhatKy.log("xong, trả suất");
                } catch (Exception e) { NhatKy.log("lỗi: " + e); }
            }, "gọi-" + k));
        }
        for (Thread t : ths) t.start();
        for (Thread t : ths) t.join();
        double dt = (System.nanoTime() - t0) / 1e9;
        String[] p = goiDichVu(host, port, "THONGKE").split(" ");
        int dot = (N_GOI_RA + SO_SUAT - 1) / SO_SUAT;
        NhatKy.log(NhatKy.f("dịch vụ xác nhận: %s yêu cầu, đồng thời tối đa %s; tổng %.2f s (≈ ⌈%d/%d⌉ × %d ms = %.2f s)",
                p[1], p[2], dt, N_GOI_RA, SO_SUAT, MS_GOI_RA, dot * MS_GOI_RA / 1000.0));
    }

    // Đặc thù Java 21: luồng ảo so với nhóm luồng nền tảng cố định, cùng 100 lời gọi mạng chờ 100 ms
    static double chay100(String host, int port, ExecutorService ex) throws Exception {
        long t = System.nanoTime();
        try (ex) {                                       // close() = shutdown + chờ mọi việc xong
            List<Future<String>> fs = new ArrayList<>();
            for (int k = 1; k <= 100; k++) {
                int kk = k;
                fs.add(ex.submit(() -> goiDichVu(host, port, "VIEC " + (100 + kk) + " 100")));   // Callable → Future
            }
            for (Future<String> f : fs) f.get();         // get() chờ và lấy KẾT QUẢ của luồng
        }
        return (System.nanoTime() - t) / 1e9;
    }

    static void run(String host, int port) throws Exception {
        coBan(host, port);
        gioiHan(host, port);
        NhatKy.say();
        NhatKy.say("(g) Nhật ký theo luồng: mỗi dòng ghi tên luồng; ngữ cảnh \"yc=\" lấy từ biến cục bộ luồng,");
        NhatKy.say("    nên hàm ghi nhật ký không cần nhận mã yêu cầu qua tham số.");
        NhatKy.say();
        double a = chay100(host, port, Executors.newFixedThreadPool(10));
        double b = chay100(host, port, Executors.newVirtualThreadPerTaskExecutor());
        goiDichVu(host, port, "THONGKE");
        NhatKy.say("Đặc thù Java – luồng ảo (Java 21) so với nhóm luồng nền tảng, 100 lời gọi VIEC 100 ms:");
        NhatKy.say(NhatKy.f("  %-52s%6.2f s", "nhóm 10 luồng nền tảng (newFixedThreadPool)", a));
        NhatKy.say(NhatKy.f("  %-52s%6.2f s", "mỗi lời gọi một luồng ảo (newVirtualThreadPerTask)", b));
        NhatKy.say("  Luồng ảo rất rẻ; khi chờ socket nó nhả luồng HĐH đang mang nó (carrier) cho luồng ảo khác,");
        NhatKy.say("  nên viết mã chặn quen thuộc mà vẫn phục vụ được hàng nghìn kết nối.");
    }

    public static void main(String[] args) throws Exception {
        Thread.currentThread().setName("chính");
        List<String> vai = List.of("dich-vu", "server", "luong", "hang-doi", "tai", "khoa");
        if (args.length < 3 || !vai.contains(args[0])) {
            NhatKy.say("Cách dùng: java -cp out Luong dich-vu|server|luong|hang-doi|tai|khoa <host> <port>");
            System.exit(1);
        }
        String host = args[1];
        int port = Integer.parseInt(args[2]);
        switch (args[0]) {
            case "dich-vu" -> DichVu.run(host, port);
            case "server" -> MayChu.runServer(host, port);
            case "luong" -> run(host, port);
            case "hang-doi" -> HangDoi.run(host, port);
            case "tai" -> MayChu.runTai(host, port);
            default -> Khoa.run(host, port);
        }
    }
}
