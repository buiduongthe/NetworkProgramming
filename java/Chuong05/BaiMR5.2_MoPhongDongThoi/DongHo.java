import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Ghi lại khoảng thời gian chạy của từng tác vụ và vẽ biểu đồ Gantt bằng văn bản.
 *
 * Mỗi hàng của biểu đồ là một worker (luồng, luồng ảo, luồng trong nhóm); mỗi cột là một khoảng thời gian
 * bằng nhau, ký tự lấy theo thời điểm giữa cột:
 *   chữ cái  = đúng một tác vụ đang chạy trên worker đó (A = tác vụ 1, B = tác vụ 2, ...)
 *   *        = nhiều tác vụ cùng lúc trên một hàng (ví dụ nhiều tác vụ cùng chờ mạng)
 */
final class DongHo {
    private DongHo() {}

    static final int COLS = 48;
    static final String LETTERS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    record Seg(int task, String worker, double a, double b) {}   // a, b: giây kể từ lúc bắt đầu ghi

    /** Bộ ghi các khoảng thời gian; an toàn khi nhiều luồng cùng ghi. */
    static final class Recorder {
        final long t0 = System.nanoTime();
        private final List<Seg> segs = new ArrayList<>();

        synchronized void add(int task, String worker, long start, long end) {
            segs.add(new Seg(task, worker, (start - t0) / 1e9, (end - t0) / 1e9));
        }

        synchronized List<Seg> segs() { return new ArrayList<>(segs); }

        double total() {
            double t = 0;
            for (Seg s : segs()) t = Math.max(t, s.b());
            return t;
        }
    }

    /** Đặt tên dễ đọc cho worker theo thứ tự xuất hiện: luồng-1, luồng-2, ... */
    static final class Names {
        final String prefix;
        final AtomicInteger n = new AtomicInteger();
        final ConcurrentHashMap<Thread, String> byThread = new ConcurrentHashMap<>();
        Names(String prefix) { this.prefix = prefix; }

        /** Tên mới mỗi lần gọi (mỗi tác vụ một worker riêng). */
        String next() { return prefix + "-" + n.incrementAndGet(); }

        /** Tên gắn với luồng đang chạy (luồng của nhóm được dùng lại cho nhiều tác vụ). */
        String ofCurrentThread() { return byThread.computeIfAbsent(Thread.currentThread(), t -> next()); }
    }

    record Result(String name, Recorder rec, String kind) {}

    static void gantt(String title, Recorder r, double scale, String kind) {
        final int maxRows = 8;
        System.out.printf(Locale.ROOT, "%s: %.2f s%n", title, r.total());
        List<Seg> segs = r.segs();
        segs.sort((x, y) -> Double.compare(x.a(), y.a()));     // List.sort ổn định
        List<String> workers = new ArrayList<>();
        for (Seg s : segs) if (!workers.contains(s.worker())) workers.add(s.worker());
        for (int i = 0; i < Math.min(maxRows, workers.size()); i++) {
            String w = workers.get(i);
            StringBuilder b = new StringBuilder();
            for (int c = 0; c < COLS; c++) {
                double lo = scale * c / COLS, hi = scale * (c + 1) / COLS, mid = (lo + hi) / 2;
                List<Seg> now = new ArrayList<>(), on = new ArrayList<>();
                for (Seg s : segs) {
                    if (!s.worker().equals(w)) continue;
                    if (s.a() <= mid && mid < s.b()) now.add(s);   // đang chạy giữa cột
                    if (s.a() < hi && s.b() > lo) on.add(s);
                }
                if (now.size() >= 2) b.append('*');                // nhiều tác vụ cùng lúc
                else if (now.size() == 1) b.append(LETTERS.charAt(now.get(0).task()));
                else if (!on.isEmpty()) b.append(LETTERS.charAt(on.get(0).task()));   // tác vụ ngắn hơn một cột
                else b.append(' ');
            }
            System.out.printf("  %-12s|%s|%n", w, b);
        }
        if (workers.size() > maxRows)
            System.out.printf("  … và %d %s khác, mỗi %s chạy 1 tác vụ%n", workers.size() - maxRows, kind, kind);
    }

    static void axis(double scale) {
        System.out.printf(Locale.ROOT, "  %-12s 0%" + COLS + "s%n", "", String.format(Locale.ROOT, "%.1f s", scale));
    }

    static double scaleOf(List<Result> results) {
        double scale = 0;
        for (Result r : results) scale = Math.max(scale, r.rec().total());
        return scale;
    }

    static void show(List<Result> results) {
        double scale = scaleOf(results);
        System.out.println();
        for (Result r : results) gantt(r.name(), r.rec(), scale, r.kind());
        axis(scale);
        System.out.println();
        double base = results.get(0).rec().total();
        System.out.printf("  %-30s %9s %9s%n", "Mô hình", "Thời gian", "Tăng tốc");
        for (Result r : results) {
            double t = r.rec().total();
            System.out.printf(Locale.ROOT, "  %-30s %7.2f s %8.1f×%n", r.name(), t, base / t);
        }
        System.out.println();
    }
}
