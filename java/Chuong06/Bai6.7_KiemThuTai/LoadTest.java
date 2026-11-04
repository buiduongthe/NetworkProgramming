import java.io.DataInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.Socket;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Queue;
import java.util.concurrent.ConcurrentLinkedQueue;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Bài 6.7 – Bộ sinh tải vòng kín đo thông lượng và phân vị độ trễ p50, p95, p99 (Mã giả 6.9, Mục 6.5.1).
 * Kế thừa Mã giả 5.2: khởi động N tác vụ rồi WHEN_ALL. Java dùng luồng ảo (mỗi client một luồng ảo, mã chặn);
 * đóng executor tương đương chờ mọi tác vụ hoàn tất.
 * Chạy: java -cp out LoadTest [host] [port] [n_requests] [levels]   (mặc định 127.0.0.1 9000 20 1,10,100)
 */
public class LoadTest {
    /** PERCENTILE theo hạng gần nhất: giá trị mà p% số mẫu không vượt quá. */
    static double percentile(List<Double> values, double p) {
        if (values.isEmpty()) return 0;
        List<Double> s = new ArrayList<>(values);
        Collections.sort(s);
        return s.get(Math.max(0, (int) Math.ceil(p / 100 * s.size()) - 1));
    }

    static double[] loadTest(String host, int port, int nClients, int nRequests) {
        Queue<Double> lat = new ConcurrentLinkedQueue<>();              // dòng 2: CONCURRENT_LIST()
        AtomicInteger errors = new AtomicInteger();                     // ATOMIC_COUNTER(0)
        Map<String, Object> req = Json.obj("op", "lookup", "term", "TCP");   // dòng 3
        Runnable oneClient = () -> {                                    // dòng 4–20
            try (Socket s = NetUtil.safeConnect(host, port, 3000)) {    // SAFE_CONNECT, 3000 ms
                DataInputStream in = new DataInputStream(s.getInputStream());
                OutputStream out = s.getOutputStream();
                for (int j = 1; j <= nRequests; j++) {
                    long t0 = System.nanoTime();                        // NOW_PRECISE()
                    NetUtil.sendJson(out, req);
                    Map<String, Object> r = NetUtil.recvJson(in);
                    if (!Long.valueOf(200).equals(r.get("status"))) { errors.incrementAndGet(); break; }   // dòng 12: vd. 503
                    lat.add((System.nanoTime() - t0) / 1e6);
                }
            } catch (IOException | Json.JsonError e) {                  // dòng 15–16
                errors.incrementAndGet();                               // ghi nhận, không dừng cả bài
            }                                                           // try-with-resources: CLOSE(s) nếu đã mở
        };
        long t = System.nanoTime();
        try (ExecutorService ex = Executors.newVirtualThreadPerTaskExecutor()) {
            for (int i = 0; i < nClients; i++) ex.submit(oneClient);
        }                                                               // close() = WHEN_ALL: chờ mọi tác vụ
        double sec = (System.nanoTime() - t) / 1e9;
        List<Double> list = new ArrayList<>(lat);
        return new double[] {list.size(), errors.get(), list.size() / sec,
                percentile(list, 50), percentile(list, 95), percentile(list, 99)};
    }

    public static void main(String[] args) {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int nRequests = NetUtil.intArg(args, 2, 20);
        String[] levels = (args.length > 3 ? args[3] : "1,10,100").split(",");
        System.out.println("[load] Server " + host + ":" + port + ", " + nRequests + " yêu cầu/client, vòng kín");
        System.out.printf("[load] %6s %6s %5s %8s %7s %7s %7s%n", "client", "OK", "lỗi", "yc/giây", "p50", "p95", "p99");
        for (String lv : levels) {
            int n = Integer.parseInt(lv.trim());
            double[] r = loadTest(host, port, n, nRequests);
            System.out.printf(java.util.Locale.ROOT, "[load] %6d %6d %5d %8.1f %7.1f %7.1f %7.1f%n",
                    n, (int) r[0], (int) r[1], r[2], r[3], r[4], r[5]);
        }
        System.out.println("[load] OK: số yêu cầu thành công; lỗi: số client gặp lỗi (bị từ chối, mất kết nối, mã ≠ 200)");
        System.out.println("[load] p50, p95, p99: phân vị độ trễ, tính bằng ms");
    }
}
