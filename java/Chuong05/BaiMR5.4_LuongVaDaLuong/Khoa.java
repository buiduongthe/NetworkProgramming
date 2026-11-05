import java.io.BufferedReader;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Bài mở rộng MR5.4 (c), (f) – Trạng thái dùng chung giữa các luồng xử lý kết nối; bế tắc.
 * (c) Server đếm chạy ngay trong chương trình: 4 luồng "gửi-k" mở 4 kết nối TCP, mỗi kết nối gửi 2000 dòng
 *     "DEM GET"/"DEM PUT"; mỗi kết nối một luồng "xử-lý-k" cập nhật bốn bộ đếm dùng chung:
 *     1. không khóa: đọc → Thread.yield() (nhường CPU, giống ghi nhật ký giữa chừng) → ghi → mất cập nhật;
 *     2. ReentrantLock quanh đúng đoạn đọc – nhường – ghi (synchronized cũng được);
 *     3. AtomicLong.incrementAndGet (nguyên tử); 4. ConcurrentHashMap.merge theo loại thông điệp.
 *     Lưu ý: HashMap thường bị nhiều luồng cùng ghi có thể mất phần tử, hỏng cấu trúc – không thử ở đây.
 * (f) "chuyển-AB" khóa phiên A rồi B, "chuyển-BA" khóa B rồi A; tryLock(500 ms) chờ có hạn để PHÁT HIỆN
 *     bế tắc; sửa bằng thứ tự lấy khóa cố định (A trước B).
 */
final class Khoa {
    static final int N_KET_NOI = 4, N_TIN = 2000, GIU_MS = 50, HAN_CHO_MS = 500;

    static long khongKhoa, coKhoa;
    static final ReentrantLock KHOA_DEM = new ReentrantLock();
    static final AtomicLong NGUYEN_TU = new AtomicLong();
    static final Map<String, Long> BANG = new ConcurrentHashMap<>();

    static void capNhat(String loai) {
        long v = khongKhoa;              // 1. đọc
        Thread.yield();                  //    nhường CPU: luồng khác có thể chen vào đây
        khongKhoa = v + 1;               //    ghi – có thể đè lên kết quả của luồng khác
        KHOA_DEM.lock();                 // 2. đoạn găng có khóa
        try {
            v = coKhoa;
            Thread.yield();
            coKhoa = v + 1;
        } finally {
            KHOA_DEM.unlock();
        }
        NGUYEN_TU.incrementAndGet();     // 3. đọc – cộng – ghi nguyên tử
        BANG.merge(loai, 1L, Long::sum); // 4. bảng đồng thời
    }

    static void tranhChap(String host, int port) throws Exception {
        NhatKy.say(NhatKy.f("(c) Trạng thái dùng chung: %d kết nối × %d thông điệp DEM, mỗi kết nối một luồng xử lý", N_KET_NOI, N_TIN));
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < N_TIN; i++) sb.append(i % 2 == 0 ? "DEM GET\n" : "DEM PUT\n");
        byte[] data = sb.toString().getBytes(StandardCharsets.UTF_8);
        List<Thread> ths = new ArrayList<>();
        try (ServerSocket ss = new ServerSocket(port, 16, InetAddress.getByName(host))) {
            NhatKy.log(NhatKy.f("server đếm lắng nghe %s:%d; %d luồng gửi bắt đầu", host, port, N_KET_NOI));
            for (int k = 1; k <= N_KET_NOI; k++)
                ths.add(new Thread(() -> {
                    try (Socket s = new Socket(host, port)) {
                        OutputStream o = s.getOutputStream();
                        o.write(data);
                        o.flush();
                    } catch (Exception e) { NhatKy.log("lỗi: " + e); }
                }, "gửi-" + k));
            for (Thread t : ths) t.start();
            for (int k = 1; k <= N_KET_NOI; k++) {
                Socket s = ss.accept();
                Thread t = new Thread(() -> {
                    int soTin = 0;
                    try (s) {
                        BufferedReader r = NhatKy.doc(s);
                        for (String line; (line = r.readLine()) != null; ) {
                            String[] p = line.split(" ");
                            if (p.length == 2 && p[0].equals("DEM")) { capNhat(p[1]); soTin++; }
                        }
                    } catch (Exception e) { NhatKy.log("lỗi: " + e); }
                    NhatKy.log("kết nối đóng: đã xử lý " + soTin + " thông điệp");
                }, "xử-lý-" + k);
                t.start();
                ths.add(t);
            }
            for (Thread t : ths) t.join();
        }
        long dung = (long) N_KET_NOI * N_TIN;
        long get = BANG.getOrDefault("GET", 0L), put = BANG.getOrDefault("PUT", 0L);
        NhatKy.say();
        NhatKy.say(NhatKy.f("  %-32s%9s%6s%13s", "Cách cập nhật biến chung", "Giá trị", "Đúng", "Mất cập nhật"));
        Object[][] hang = {
            {"không khóa (đọc – nhường – ghi)", String.valueOf(khongKhoa), khongKhoa},
            {"khóa (mutex/lock)", String.valueOf(coKhoa), coKhoa},
            {"nguyên tử (atomic)", String.valueOf(NGUYEN_TU.get()), NGUYEN_TU.get()},
            {"bảng đồng thời: GET + PUT", get + "+" + put, get + put}};
        for (Object[] h : hang)
            NhatKy.say(NhatKy.f("  %-32s%9s%6d%13d", h[0], h[1], dung, dung - (long) h[2]));
        NhatKy.say("Nhận xét: \"đọc – làm việc khác – ghi\" không khóa làm mất cập nhật; khóa, thao tác nguyên tử");
        NhatKy.say("  hoặc cấu trúc dữ liệu đồng thời đều cho kết quả đúng.");
    }

    static void beTac() throws InterruptedException {
        NhatKy.say();
        NhatKy.say("(f) Bế tắc: hai luồng chuyển tin giữa phiên A và phiên B, mỗi luồng cần khóa của cả hai phiên");
        Map<String, ReentrantLock> khoa = Map.of("A", new ReentrantLock(), "B", new ReentrantLock());
        List<Object[]> bang = new ArrayList<>();
        for (int lan = 1; lan <= 2; lan++) {
            boolean coThuTu = lan == 2;
            NhatKy.say(coThuTu ? "Lần 2 – mọi luồng lấy khóa theo cùng thứ tự (A trước B):"
                               : "Lần 1 – lấy khóa theo thứ tự ngược nhau:");
            AtomicInteger xong = new AtomicInteger(), bo = new AtomicInteger();
            Thread t1 = new Thread(() -> chuyen(khoa, "A", "B", coThuTu, xong, bo), "chuyển-AB");
            Thread t2 = new Thread(() -> chuyen(khoa, "B", "A", coThuTu, xong, bo), "chuyển-BA");
            t1.start();
            Thread.sleep(10);
            t2.start();
            t1.join();
            t2.join();
            bang.add(new Object[] {coThuTu ? "cùng thứ tự" : "ngược thứ tự", xong.get(), bo.get()});
        }
        NhatKy.say();
        NhatKy.say(NhatKy.f("  %-20s%10s%24s", "Cách lấy khóa", "Hoàn tất", "Bỏ cuộc (hết hạn chờ)"));
        for (Object[] h : bang) NhatKy.say(NhatKy.f("  %-20s%10d%24d", h[0], h[1], h[2]));
        NhatKy.say("Nhận xét: cùng một thứ tự lấy khóa cho mọi luồng loại bỏ vòng chờ – điều kiện cần của bế tắc.");
    }

    static void chuyen(Map<String, ReentrantLock> khoa, String nguon, String dich, boolean coThuTu,
                       AtomicInteger xong, AtomicInteger bo) {
        String dau = nguon, sau = dich;
        if (coThuTu && nguon.compareTo(dich) > 0) { dau = dich; sau = nguon; }
        khoa.get(dau).lock();
        try {
            NhatKy.log("giữ khóa phiên " + dau + ", chờ khóa phiên " + sau);
            Thread.sleep(GIU_MS);                                         // giả lập ghi vào socket của phiên
            if (!khoa.get(sau).tryLock(HAN_CHO_MS, TimeUnit.MILLISECONDS)) { // chờ có hạn
                NhatKy.log(NhatKy.f("chờ khóa phiên %s quá %d ms → nghi bế tắc, nhả khóa phiên %s, bỏ cuộc", sau, HAN_CHO_MS, dau));
                bo.incrementAndGet();
                return;
            }
            try {
                NhatKy.log("đã chuyển tin từ phiên " + nguon + " sang phiên " + dich);
                xong.incrementAndGet();
            } finally {
                khoa.get(sau).unlock();
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } finally {
            khoa.get(dau).unlock();
        }
    }

    static void run(String host, int port) throws Exception {
        tranhChap(host, port);
        beTac();
    }
}
