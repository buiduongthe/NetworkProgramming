import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.locks.ReentrantLock;

/**
 * Tranh chấp dữ liệu (race condition), khóa, thao tác nguyên tử và tránh bế tắc (deadlock).
 *
 *  1. Tranh chấp: 4 luồng cùng "đọc – nhường CPU – ghi" lên một biến đếm chung; Thread.yield()
 *     (giống gọi mạng hay ghi nhật ký giữa chừng) cho luồng khác chen vào → mất lần cộng.
 *     Sửa bằng ReentrantLock (hoặc synchronized), hoặc bằng AtomicLong (cộng nguyên tử, không cần khóa).
 *  2. Bế tắc: T1 giữ khóa X rồi chờ Y, T2 giữ Y rồi chờ X. tryLock(thời gian) cho phép chờ có hạn để phát
 *     hiện bế tắc; cách tránh là luôn lấy khóa theo cùng một thứ tự.
 */
final class Khoa {
    private Khoa() {}

    static final int N_T = 4, N_ITER = 5000;
    static long plain = 0, locked = 0;             // biến đếm chung (cố ý không volatile, không khóa)

    static void run(Runnable body) {
        List<Thread> ts = new ArrayList<>();
        for (int i = 0; i < N_T; i++) ts.add(Thread.ofPlatform().start(body));
        DongThoi.joinAll(ts);
    }

    static void report(String name, long v, long t0) {
        long total = (long) N_T * N_ITER;
        String verdict = v == total ? "đúng" : "mất " + (total - v) + " lần cộng";
        System.out.printf(Locale.ROOT, "  %-16s: %6d / %d → %s (%.2f s)%n", name, v, total, verdict, (System.nanoTime() - t0) / 1e9);
    }

    static void raceDemo() {
        System.out.println("Tranh chấp dữ liệu: " + N_T + " luồng × " + N_ITER + " lần \"đọc – nhường CPU – ghi\"");
        System.out.println("  lên một biến đếm chung (kết quả đúng phải là " + (N_T * N_ITER) + ").");

        long t = System.nanoTime();                 // 1. không khóa
        run(() -> {
            for (int i = 0; i < N_ITER; i++) {
                long v = plain;                     // đọc
                Thread.yield();                     // nhường CPU: luồng khác có thể chen vào đây
                plain = v + 1;                      // ghi – có thể đè lên kết quả của luồng khác
            }
        });
        report("không khóa", plain, t);

        ReentrantLock mu = new ReentrantLock();     // 2. khóa
        t = System.nanoTime();
        run(() -> {
            for (int i = 0; i < N_ITER; i++) {
                mu.lock();                          // đoạn găng: chỉ một luồng được vào
                try {
                    long v = locked;
                    Thread.yield();
                    locked = v + 1;
                } finally {
                    mu.unlock();
                }
            }
        });
        report("có khóa (Lock)", locked, t);

        AtomicLong at = new AtomicLong();           // 3. thao tác nguyên tử
        t = System.nanoTime();
        run(() -> {
            for (int i = 0; i < N_ITER; i++) {
                at.incrementAndGet();               // đọc – cộng – ghi trong một lệnh của CPU
                Thread.yield();
            }
        });
        report("AtomicLong", at.get(), t);
    }

    static synchronized void say(String s) { System.out.println("  " + s); }

    static void deadlockDemo() throws InterruptedException {
        System.out.println();
        System.out.println("Bế tắc (deadlock): T1 chuyển X→Y, T2 chuyển Y→X, mỗi luồng cần cả");
        System.out.println("  hai khóa; số dư ban đầu X = 100, Y = 100.");
        Map<String, Integer> bal = new HashMap<>(Map.of("X", 100, "Y", 100));
        Map<String, ReentrantLock> locks = Map.of("X", new ReentrantLock(), "Y", new ReentrantLock());
        for (boolean ordered : new boolean[] {false, true}) {
            if (ordered) System.out.println("Cách tránh: mọi luồng lấy khóa theo cùng thứ tự (X trước, Y sau):");
            CountDownLatch t1Started = new CountDownLatch(1);
            Thread t1 = Thread.ofPlatform().start(() -> transfer(bal, locks, "T1", "X", "Y", 10, ordered, t1Started));
            t1Started.await();                         // chờ T1 thật sự giữ khóa đầu tiên rồi mới cho T2 chạy
            Thread.sleep(10);
            Thread t2 = Thread.ofPlatform().start(() -> transfer(bal, locks, "T2", "Y", "X", 5, ordered, new CountDownLatch(1)));
            t1.join();
            t2.join();
            System.out.println("  Số dư: X = " + bal.get("X") + ", Y = " + bal.get("Y") + " (tổng " + (bal.get("X") + bal.get("Y")) + ")");
        }
    }

    static void transfer(Map<String, Integer> bal, Map<String, ReentrantLock> locks, String name, String src, String dst,
                         int amount, boolean ordered, CountDownLatch started) {
        String first = src, second = dst;
        if (ordered && first.compareTo(second) > 0) { first = dst; second = src; }
        locks.get(first).lock();
        started.countDown();
        try {
            say(name + " giữ khóa " + first + ", chờ khóa " + second);
            DongThoi.sleep(50);                      // để luồng kia kịp giữ khóa còn lại
            boolean got;
            try {
                got = locks.get(second).tryLock(500, TimeUnit.MILLISECONDS);
            } catch (InterruptedException e) {
                got = false;
            }
            if (!got) {
                say(name + ": chờ " + second + " quá 0,5 s → nghi bế tắc, nhả " + first + ", bỏ cuộc");
                return;
            }
            try {
                synchronized (bal) {
                    bal.put(src, bal.get(src) - amount);
                    bal.put(dst, bal.get(dst) + amount);
                }
                say(name + ": đã chuyển " + amount + " từ " + src + " sang " + dst);
            } finally {
                locks.get(second).unlock();
            }
        } finally {
            locks.get(first).unlock();
        }
    }
}
