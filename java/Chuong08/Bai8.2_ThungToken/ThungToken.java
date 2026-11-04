import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;

/**
 * Bài 8.2 – Thùng token giới hạn tần suất (Mã giả 8.3; Mục 8.2.4 Giáo trình Lập trình mạng).
 * Nhiều tác vụ (luồng ảo) đồng thời gửi một chuỗi yêu cầu qua cùng một thùng; chương trình in thời điểm
 * mỗi yêu cầu được phép, so với lý thuyết: C yêu cầu đầu đi ngay, yêu cầu thứ m > C sau (m − C)/R giây.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out ThungToken [C] [R] [số_yêu_cầu] [số_tác_vụ]      (mặc định 10 2 14 2)
 */
public class ThungToken {
    public static void main(String[] args) throws InterruptedException {
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        double cap = args.length > 0 ? Double.parseDouble(args[0]) : 10;
        double rate = args.length > 1 ? Double.parseDouble(args[1]) : 2;
        int total = args.length > 2 ? Integer.parseInt(args[2]) : 14;
        int tasks = args.length > 3 ? Integer.parseInt(args[3]) : 2;
        Bucket b = new Bucket(cap, rate);
        out.printf(Locale.ROOT, "[bucket] Thùng: dung lượng C = %s, tốc độ nạp R = %s đơn vị/giây, mỗi yêu cầu n = 1%n", g(cap), g(rate));
        out.printf("[bucket] Gửi %d yêu cầu từ %d tác vụ đồng thời (thùng đầy lúc bắt đầu)%n", total, tasks);
        long t0 = System.nanoTime();
        List<double[]> granted = new ArrayList<>();               // (thời điểm, mã tác vụ)
        List<Thread> threads = new ArrayList<>();
        for (int w = 0; w < tasks; w++) {
            int wid = w + 1, count = total / tasks + (w < total % tasks ? 1 : 0);
            threads.add(Thread.ofVirtual().start(() -> {           // một tác vụ gửi count yêu cầu, mỗi yêu cầu 1 đơn vị
                try {
                    for (int i = 0; i < count; i++) {
                        b.acquireTokens(1);
                        synchronized (granted) { granted.add(new double[]{(System.nanoTime() - t0) / 1e9, wid}); }
                        Thread.yield();                            // nhường lượt: giả lập gửi yêu cầu
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }));
        }
        for (Thread t : threads) t.join();
        granted.sort(Comparator.comparingDouble(x -> x[0]));
        for (int m = 1; m <= granted.size(); m++) {
            double[] gr = granted.get(m - 1);
            double theory = Math.max(0, (m - cap) / rate);
            out.printf(Locale.ROOT, "[bucket] Yêu cầu %02d (tác vụ %d) được phép tại t = %6.3f s   lý thuyết %6.3f s%n",
                    m, (int) gr[1], gr[0], theory);
        }
        try {
            out.printf("[bucket] Thử lấy n = %s > C = %s đơn vị%n", g(cap + 1), g(cap));
            b.acquireTokens(cap + 1);
        } catch (IllegalArgumentException e) {                     // dòng 3: báo lỗi ngay, không chờ mãi
            out.println("[bucket] Lỗi ArgumentError: " + e.getMessage());
        }
    }

    /** In số như %g của Python: 10.0 -> "10", 2.5 -> "2.5". */
    static String g(double v) { return v == Math.rint(v) ? String.valueOf((long) v) : String.valueOf(v); }
}
