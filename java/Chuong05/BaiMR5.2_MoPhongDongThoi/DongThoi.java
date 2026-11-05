import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Bài mở rộng MR5.2 – So sánh các mô hình đồng thời trên cùng một khối việc (Java 21).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Chương 5 (Bài 5.1, 5.2 – bất đồng bộ, giới hạn
 * đồng thời) và Chương 6 (Bài 6.1, 6.2 – server đa luồng, nhóm luồng, bất đồng bộ).
 *
 * Ba khối việc:
 *   mang  – 20 yêu cầu mạng tới máy chủ thử (mỗi yêu cầu trễ 200 ms): việc chờ I/O;
 *   cpu   – 8 tác vụ tính toán (mỗi tác vụ ≈ 100 ms): việc nặng CPU;
 *   tron  – 20 tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms: luồng ảo và nhóm luồng CPU.
 * Thêm: khoa – tranh chấp dữ liệu và cách sửa (Lock, AtomicLong), bế tắc và cách tránh.
 *
 * Chạy: java -cp out DongThoi server <host> <port> [trễ_ms]
 *       java -cp out DongThoi mang <host> <port> | cpu | tron <host> <port> | khoa
 */
public class DongThoi {
    static final int N_NET = 20, N_CPU = 8;
    static final int PAUSE_MS = 500;              // nghỉ giữa hai mô hình để máy chủ thử báo từng "đợt" riêng
    static final AtomicLong sink = new AtomicLong();   // giữ kết quả để JIT không bỏ vòng lặp

    // ---------------- việc mạng và việc CPU ----------------

    /** Một yêu cầu mạng: kết nối, gửi GET, chờ dòng trả lời (lời gọi chặn luồng đang chạy nó). */
    static String fetch(InetSocketAddress addr, int k) {
        try (Socket c = new Socket()) {
            c.connect(addr, 3000);
            c.setSoTimeout(3000);
            c.getOutputStream().write(("GET " + k + "\n").getBytes(StandardCharsets.UTF_8));
            return new BufferedReader(new InputStreamReader(c.getInputStream(), StandardCharsets.UTF_8)).readLine();
        } catch (IOException e) {
            return null;
        }
    }

    /** Việc tính toán (bộ sinh đồng dư tuyến tính) giống bản Python, Go. */
    static int cpuWork(int n) {
        int x = 1;
        for (int i = 0; i < n; i++) x = (x * 1103515245 + 12345) & 0x7FFFFFFF;   // int tràn số = phép tính mod 2^32
        return x;
    }

    /** Số vòng lặp để cpuWork chạy ≈ ms mili giây trên máy này (đo vài lần, lấy lần nhanh nhất – đã qua JIT). */
    static int calibrate(int ms) {
        int n = 20_000_000;
        double dt = 1.0;
        for (int i = 0; i < 5; i++) {
            long t = System.nanoTime();
            sink.addAndGet(cpuWork(n));
            dt = Math.min(dt, (System.nanoTime() - t) / 1e9);
        }
        return (int) (n * (ms / 1000.0) / dt);
    }

    static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }

    static void joinAll(List<Thread> ts) {
        for (Thread t : ts) {
            try { t.join(); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
        }
    }

    // ---------------- khối việc 1: mạng ----------------
    static void runNet(String host, int port) throws Exception {
        InetSocketAddress addr = new InetSocketAddress(host, port);
        System.out.println("Khối việc: " + N_NET + " yêu cầu mạng, mỗi yêu cầu máy chủ trễ 200 ms");
        List<DongHo.Result> results = new ArrayList<>();

        // 1. Tuần tự
        DongHo.Recorder rec = new DongHo.Recorder();
        for (int k = 0; k < N_NET; k++) {
            long t = System.nanoTime();
            fetch(addr, k);
            rec.add(k, "chính", t, System.nanoTime());
        }
        results.add(new DongHo.Result("1. tuần tự", rec, "luồng"));
        sleep(PAUSE_MS);

        // 2. Mỗi tác vụ một luồng nền tảng (platform thread = luồng của hệ điều hành)
        DongHo.Recorder rec2 = new DongHo.Recorder();
        DongHo.Names names2 = new DongHo.Names("luồng");
        List<Thread> ts = new ArrayList<>();
        for (int k = 0; k < N_NET; k++) {
            final int kk = k;
            String w = names2.next();                      // đặt tên theo thứ tự tạo luồng
            ts.add(Thread.ofPlatform().start(() -> {
                long t = System.nanoTime();
                fetch(addr, kk);
                rec2.add(kk, w, t, System.nanoTime());
            }));
        }
        joinAll(ts);
        results.add(new DongHo.Result("2. mỗi tác vụ một luồng", rec2, "luồng"));
        sleep(PAUSE_MS);

        // 3. Nhóm luồng cố định 5 luồng (ExecutorService)
        DongHo.Recorder rec3 = new DongHo.Recorder();
        DongHo.Names names3 = new DongHo.Names("luồng");
        try (ExecutorService pool = Executors.newFixedThreadPool(5)) {   // close() chờ mọi việc xong
            for (int k = 0; k < N_NET; k++) {
                final int kk = k;
                pool.submit(() -> {
                    String w = names3.ofCurrentThread();
                    long t = System.nanoTime();
                    fetch(addr, kk);
                    rec3.add(kk, w, t, System.nanoTime());
                });
            }
        }
        results.add(new DongHo.Result("3. nhóm luồng (5 luồng)", rec3, "luồng"));
        sleep(PAUSE_MS);

        // 4. Luồng ảo (JDK 21): mỗi tác vụ một luồng ảo, gắn lên vài luồng mang (carrier)
        DongHo.Recorder rec4 = new DongHo.Recorder();
        DongHo.Names names4 = new DongHo.Names("ảo");
        try (ExecutorService vt = Executors.newVirtualThreadPerTaskExecutor()) {
            for (int k = 0; k < N_NET; k++) {
                final int kk = k;
                String w = names4.next();
                vt.submit(() -> {
                    long t = System.nanoTime();
                    fetch(addr, kk);
                    rec4.add(kk, w, t, System.nanoTime());
                });
            }
        }
        results.add(new DongHo.Result("4. luồng ảo", rec4, "luồng ảo"));
        sleep(PAUSE_MS);

        // 5. CompletableFuture: ghép kết quả bằng allOf, hạn chót chung 2 s (orTimeout)
        DongHo.Recorder rec5 = new DongHo.Recorder();
        DongHo.Names names5 = new DongHo.Names("ảo");
        int ok = 0;
        try (ExecutorService vt = Executors.newVirtualThreadPerTaskExecutor()) {
            List<CompletableFuture<String>> fs = new ArrayList<>();
            for (int k = 0; k < N_NET; k++) {
                final int kk = k;
                String w = names5.next();
                fs.add(CompletableFuture.supplyAsync(() -> {
                    long t = System.nanoTime();
                    String r = fetch(addr, kk);
                    rec5.add(kk, w, t, System.nanoTime());
                    return r;
                }, vt));
            }
            try {
                CompletableFuture.allOf(fs.toArray(new CompletableFuture[0])).orTimeout(2, TimeUnit.SECONDS).join();
            } catch (Exception e) {
                // quá hạn chót: đếm những kết quả đã có
            }
            for (CompletableFuture<String> f : fs) if (f.isDone() && !f.isCompletedExceptionally() && f.getNow(null) != null) ok++;
        }
        results.add(new DongHo.Result("5. CompletableFuture + allOf", rec5, "luồng ảo"));
        DongHo.show(results);
        System.out.println("Mô hình 5 nhận đủ " + ok + "/" + N_NET + " kết quả trước hạn chót 2 s.");
        System.out.println("Nhận xét: việc chờ I/O không tốn CPU; luồng nền tảng, luồng ảo hay");
        System.out.println("  CompletableFuture đều chồng được thời gian chờ. Luồng ảo rẻ (vài KB,");
        System.out.println("  không chiếm luồng hệ điều hành khi chờ mạng) nên viết kiểu chặn,");
        System.out.println("  mỗi yêu cầu một luồng ảo, mà vẫn mở rộng tới hàng nghìn kết nối.");
    }

    // ---------------- khối việc 2: CPU ----------------
    static void runCpu() throws Exception {
        int n = calibrate(100);
        int ncpu = Runtime.getRuntime().availableProcessors();
        System.out.println("Khối việc: " + N_CPU + " tác vụ CPU, mỗi tác vụ ≈ 100 ms");
        System.out.println("Máy có " + ncpu + " CPU; Java không có GIL; luồng ảo chạy trên " + ncpu + " luồng mang");
        List<DongHo.Result> results = new ArrayList<>();

        DongHo.Recorder rec = new DongHo.Recorder();
        for (int k = 0; k < N_CPU; k++) {
            long t = System.nanoTime();
            sink.addAndGet(cpuWork(n));
            rec.add(k, "chính", t, System.nanoTime());
        }
        results.add(new DongHo.Result("1. tuần tự", rec, "luồng"));

        DongHo.Recorder rec2 = new DongHo.Recorder();
        DongHo.Names names2 = new DongHo.Names("luồng");
        List<Thread> ts = new ArrayList<>();
        for (int k = 0; k < N_CPU; k++) {
            final int kk = k;
            String w = names2.next();
            ts.add(Thread.ofPlatform().start(() -> {
                long t = System.nanoTime();
                int x = cpuWork(n);
                rec2.add(kk, w, t, System.nanoTime());
                sink.addAndGet(x);
            }));
        }
        joinAll(ts);
        results.add(new DongHo.Result("2. mỗi tác vụ một luồng", rec2, "luồng"));

        DongHo.Recorder rec3 = new DongHo.Recorder();
        DongHo.Names names3 = new DongHo.Names("luồng");
        try (ExecutorService pool = Executors.newFixedThreadPool(ncpu)) {
            for (int k = 0; k < N_CPU; k++) {
                final int kk = k;
                pool.submit(() -> {
                    String w = names3.ofCurrentThread();
                    long t = System.nanoTime();
                    int x = cpuWork(n);
                    rec3.add(kk, w, t, System.nanoTime());
                    sink.addAndGet(x);
                });
            }
        }
        results.add(new DongHo.Result("3. nhóm luồng (" + ncpu + " = số CPU)", rec3, "luồng"));

        DongHo.Recorder rec4 = new DongHo.Recorder();
        DongHo.Names names4 = new DongHo.Names("ảo");
        try (ExecutorService vt = Executors.newVirtualThreadPerTaskExecutor()) {
            for (int k = 0; k < N_CPU; k++) {
                final int kk = k;
                String w = names4.next();
                vt.submit(() -> {
                    long t = System.nanoTime();
                    int x = cpuWork(n);
                    rec4.add(kk, w, t, System.nanoTime());
                    sink.addAndGet(x);
                });
            }
        }
        results.add(new DongHo.Result("4. luồng ảo", rec4, "luồng ảo"));
        DongHo.show(results);
        System.out.println("Nhận xét: không có GIL, các luồng chạy song song thật trên mọi CPU.");
        System.out.println("  Mô hình 2: 8 luồng hệ điều hành chia lượt trên " + ncpu + " CPU nên mỗi tác vụ");
        System.out.println("  bị kéo dài; nhóm luồng bằng số CPU cho cùng tổng thời gian mà mỗi tác");
        System.out.println("  vụ chạy liền một mạch. Luồng ảo KHÔNG bị chia lượt: mỗi luồng ảo giữ");
        System.out.println("  luồng mang tới khi xong việc CPU, nên chạy giống nhóm " + ncpu + " luồng.");
    }

    // ---------------- khối việc 3: mạng + CPU ----------------
    /** Một kịch bản của khối "tron"; trả về độ trễ lớn nhất của luồng nhịp tim (ms). */
    static long scenario(String mode, DongHo.Recorder rec, InetSocketAddress addr, int n, int ncpu) throws Exception {
        long[] lag = {0};
        boolean[] stop = {false};
        Runnable heartbeat = () -> {                       // nhịp tim: ngủ 10 ms, đo bị đánh thức muộn bao nhiêu
            while (!stop[0]) {
                long t = System.nanoTime();
                sleep(10);
                lag[0] = Math.max(lag[0], (System.nanoTime() - t) / 1_000_000 - 10);
            }
        };
        boolean platform = mode.equals("platform");
        Thread hb = platform ? Thread.ofPlatform().start(heartbeat) : Thread.ofVirtual().start(heartbeat);
        DongHo.Names names = new DongHo.Names(platform ? "luồng" : "ảo");
        DongHo.Names poolNames = new DongHo.Names("cpu");
        ExecutorService cpuPool = mode.equals("pool") ? Executors.newFixedThreadPool(ncpu) : null;
        List<Thread> ts = new ArrayList<>();
        for (int k = 0; k < N_NET; k++) {
            final int kk = k;
            Runnable task = () -> {
                long t = System.nanoTime();
                fetch(addr, kk);
                rec.add(kk, "chờ mạng", t, System.nanoTime());
                if (cpuPool != null) {                     // gửi việc CPU sang nhóm luồng nền tảng, chờ kết quả
                    try {
                        sink.addAndGet(cpuPool.submit(() -> {
                            long t2 = System.nanoTime();
                            int x = cpuWork(n);
                            rec.add(kk, poolNames.ofCurrentThread(), t2, System.nanoTime());
                            return x;
                        }).get());
                    } catch (Exception e) {
                        throw new IllegalStateException(e);
                    }
                } else {                                   // xử lý CPU ngay trong luồng của tác vụ
                    String w = names.next();
                    long t2 = System.nanoTime();
                    int x = cpuWork(n);
                    rec.add(kk, w, t2, System.nanoTime());
                    sink.addAndGet(x);
                }
            };
            ts.add(platform ? Thread.ofPlatform().start(task) : Thread.ofVirtual().start(task));
        }
        joinAll(ts);
        if (cpuPool != null) cpuPool.close();
        stop[0] = true;
        hb.join();
        return lag[0];
    }

    static void runMixed(String host, int port) throws Exception {
        InetSocketAddress addr = new InetSocketAddress(host, port);
        int n = calibrate(30);
        int ncpu = Runtime.getRuntime().availableProcessors();
        System.out.println("Khối việc: " + N_NET + " tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms");
        System.out.println("Đo thêm độ trễ lập lịch: luồng \"nhịp tim\" ngủ 10 ms và đo xem");
        System.out.println("  thực tế bị đánh thức muộn bao nhiêu (trễ lớn = ứng dụng đứng).");
        List<DongHo.Result> results = new ArrayList<>();
        List<Long> lags = new ArrayList<>();
        DongHo.Recorder rec = new DongHo.Recorder();
        lags.add(scenario("inline", rec, addr, n, ncpu));
        results.add(new DongHo.Result("A. luồng ảo, CPU ngay trong đó", rec, "luồng ảo"));
        sleep(PAUSE_MS);
        rec = new DongHo.Recorder();
        lags.add(scenario("pool", rec, addr, n, ncpu));
        results.add(new DongHo.Result("B. luồng ảo + nhóm CPU (" + ncpu + ")", rec, "luồng"));
        sleep(PAUSE_MS);
        rec = new DongHo.Recorder();
        lags.add(scenario("platform", rec, addr, n, ncpu));
        results.add(new DongHo.Result("C. mỗi tác vụ một luồng", rec, "luồng"));

        double scale = DongHo.scaleOf(results);
        System.out.println();
        for (DongHo.Result r : results) DongHo.gantt(r.name(), r.rec(), scale, r.kind());
        DongHo.axis(scale);
        System.out.println();
        System.out.printf("  %-30s %9s %13s%n", "Mô hình", "Thời gian", "Trễ lập lịch");
        for (int i = 0; i < results.size(); i++)
            System.out.printf(Locale.ROOT, "  %-30s %7.2f s %10d ms%n", results.get(i).name(), results.get(i).rec().total(), lags.get(i));
        System.out.println();
        System.out.println("Nhận xét: \"chờ mạng\" là hàng chung của mọi tác vụ đang đợi mạng (*).");
        System.out.println("  A – luồng ảo không bị chia lượt: việc CPU giữ luồng mang tới khi xong,");
        System.out.println("  luồng ảo nhịp tim phải xếp hàng chờ nên bị trễ (giống chặn vòng lặp sự");
        System.out.println("  kiện của asyncio). B – đẩy việc CPU sang nhóm luồng nền tảng: luồng mang");
        System.out.println("  rảnh, nhịp tim đúng giờ. C – luồng hệ điều hành được chia lượt nên");
        System.out.println("  nhịp tim cũng ít trễ, nhưng mỗi luồng tốn bộ nhớ hơn luồng ảo nhiều.");
    }

    public static void main(String[] a) throws Exception {
        if (a.length >= 3 && a[0].equals("server")) MayChuTre.run(a[1], Integer.parseInt(a[2]), a.length > 3 ? Integer.parseInt(a[3]) : 200);
        else if (a.length >= 3 && a[0].equals("mang")) runNet(a[1], Integer.parseInt(a[2]));
        else if (a.length >= 1 && a[0].equals("cpu")) runCpu();
        else if (a.length >= 3 && a[0].equals("tron")) runMixed(a[1], Integer.parseInt(a[2]));
        else if (a.length >= 1 && a[0].equals("khoa")) {
            Khoa.raceDemo();
            Khoa.deadlockDemo();
        } else {
            System.out.println("Cách dùng:");
            System.out.println("  java -cp out DongThoi server <host> <port> [trễ_ms]");
            System.out.println("  java -cp out DongThoi mang <host> <port> | cpu | tron <host> <port> | khoa");
            System.exit(1);
        }
    }
}
