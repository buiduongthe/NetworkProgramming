import java.io.IOException;
import java.net.ConnectException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.UnknownHostException;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Semaphore;

/**
 * Bài 5.2 – Kiểm tra nhiều cổng đồng thời có giới hạn (Mã giả 5.2, Mục 5.2.5).
 * Kế thừa Mã giả 1.2 (PORT_CHECK). Ánh xạ Java: mỗi CHECK_ONE là một CompletableFuture chạy trên
 * luồng ảo; WHEN_ALL ↔ CompletableFuture.allOf; SEMAPHORE ↔ java.util.concurrent.Semaphore
 * (acquire() chặn luồng ảo, không chặn luồng hệ điều hành).
 * Chỉ chạy trên máy thuộc quyền quản lý hoặc mạng thí nghiệm cô lập (Mục 1.2.5).
 * Chạy: java -cp out ScanAsync [host] [cổng_đầu] [cổng_cuối] [max_parallel]  (mặc định 127.0.0.1 9000 9019 10)
 */
public class ScanAsync {
    record Result(int port, String state) {}

    /** Mã giả 1.2: kết quả OPEN, CLOSED, FILTERED hoặc UNRESOLVED. */
    static String portCheck(String host, int port, int timeoutMs) {
        InetAddress[] addrs;
        try { addrs = InetAddress.getAllByName(host); }               // RESOLVE_HOST
        catch (UnknownHostException e) { return "UNRESOLVED"; }
        String state = "FILTERED";                                     // chưa địa chỉ nào phản hồi
        for (InetAddress a : addrs) {
            try (Socket s = new Socket()) {                            // try-with-resources = FINALLY CLOSE
                s.connect(new InetSocketAddress(a, port), timeoutMs);  // CONNECT có CONNECT_TIMEOUT
                return "OPEN";                                         // bắt tay ba bước thành công
            } catch (ConnectException e) {
                state = "CLOSED";                                      // nhận gói RST
            } catch (IOException e) {
                // hết giờ, mạng không tới được: thử địa chỉ tiếp theo
            }
        }
        return state;
    }

    static Result checkOne(String host, int p, Semaphore gate) {     // Mã giả 5.2, dòng 9–16
        gate.acquireUninterruptibly();                                 // ACQUIRE(gate)
        try {
            return new Result(p, portCheck(host, p, 1000));
        } finally {
            gate.release();                                            // RELEASE(gate)
        }
    }

    static List<Result> scanPorts(String host, List<Integer> ports, int maxParallel) {   // dòng 1–8
        Semaphore gate = new Semaphore(maxParallel);                   // giới hạn số kết nối
        try (ExecutorService vexec = Executors.newVirtualThreadPerTaskExecutor()) {
            List<CompletableFuture<Result>> tasks = new ArrayList<>();
            for (int p : ports)
                tasks.add(CompletableFuture.supplyAsync(() -> checkOne(host, p, gate), vexec));  // chưa chờ
            CompletableFuture.allOf(tasks.toArray(new CompletableFuture[0])).join();  // WHEN_ALL
            return tasks.stream().map(CompletableFuture::join).toList();
        }
    }

    public static void main(String[] a) {
        String host = a.length > 0 ? a[0] : "127.0.0.1";
        int first = a.length > 1 ? Integer.parseInt(a[1]) : 9000;
        int last = a.length > 2 ? Integer.parseInt(a[2]) : 9019;
        int k = a.length > 3 ? Integer.parseInt(a[3]) : 10;
        List<Integer> ports = new ArrayList<>();
        for (int p = first; p <= last; p++) ports.add(p);
        System.out.printf("[scan] Quét %d cổng %d–%d trên %s, thời gian chờ 1000 ms%n", ports.size(), first, last, host);
        long t0 = System.nanoTime();
        List<Result> seq = scanPorts(host, ports, 1);
        double tSeq = (System.nanoTime() - t0) / 1e9;
        t0 = System.nanoTime();
        List<Result> par = scanPorts(host, ports, k);
        double tPar = (System.nanoTime() - t0) / 1e9;
        for (Result r : par)
            if (!r.state().equals("CLOSED")) System.out.println("[scan]   cổng " + r.port() + ": " + r.state());
        long open = par.stream().filter(r -> r.state().equals("OPEN")).count();
        long closed = par.stream().filter(r -> r.state().equals("CLOSED")).count();
        long filtered = par.stream().filter(r -> r.state().equals("FILTERED")).count();
        System.out.printf("[scan] Tổng hợp: OPEN=%d, CLOSED=%d, FILTERED=%d%n", open, closed, filtered);
        System.out.println("[scan] Hai lần quét cho cùng kết quả: " + (seq.equals(par) ? "có" : "không"));
        System.out.printf(java.util.Locale.ROOT, "[scan] Tuần tự   (max_parallel=1):  %.2f s%n", tSeq);
        System.out.printf(java.util.Locale.ROOT, "[scan] Đồng thời (max_parallel=%d): %.2f s%n", k, tPar);
    }
}
