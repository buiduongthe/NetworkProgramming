import java.net.Socket;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.function.Function;

/**
 * Bài 5.6 – Mã giả 5.7 REQUEST_WITH_DEADLINE (Mục 5.4.2) và Mã giả 5.8 RETRY_ASYNC (Mục 5.4.3).
 * Kế thừa: Mã giả 5.1 (khung JSON), 2.2 (SAFE_CONNECT, NetError có retry), 4.4 (vòng gửi lại).
 * Ánh xạ Java: tín hiệu hủy là CompletableFuture&lt;Void&gt;; CANCEL_AFTER ↔ completeOnTimeout;
 * LINK ↔ userCt.thenRun(ct.complete); hủy thì đóng socket để lời gọi chặn trên luồng ảo kết thúc.
 * Chạy: java -cp out DeadlineRetry [host] [port] [cổng_không_có_server]  (mặc định 127.0.0.1 9100 9101)
 */
public class DeadlineRetry {
    static long t0;
    static void log(String text) {
        System.out.printf("[client] [t=%5d ms] %s%n", (System.nanoTime() - t0) / 1_000_000, text);
    }

    /** Mã giả 5.7. */
    static Map<?, ?> requestWithDeadline(Socket sock, Map<String, Object> req, int timeoutMs,
                                         CompletableFuture<Void> userCt) throws Exception {
        CompletableFuture<Void> ct = Khung.cancelAfter(timeoutMs);  // CANCEL_AFTER(timeout_ms)
        userCt.thenRun(() -> ct.complete(null));                    // LINK: một trong hai
        Khung.linkClose(ct, sock);
        try {
            Khung.sendJson(sock, req, ct);
            return (Map<?, ?>) Khung.recvJson(sock, ct);
        } catch (CancellationException e) {
            Khung.closeQuietly(sock);                               // trạng thái khung không xác định
            if (userCt.isDone()) throw e;                           // RAISE Cancelled
            throw new TimeoutException("Quá hạn " + timeoutMs + " ms");
        } finally {
            ct.cancel(false);                                       // xong trước hạn: gỡ hẹn giờ
        }
    }

    /** Một lần thử op(ct) của Mã giả 5.8: nhận tín hiệu hủy của nơi gọi. */
    interface Op<T> { T call(CompletableFuture<Void> ct) throws Exception; }

    /** Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct). */
    static <T> T retryAsync(Op<T> op, int maxTries, int baseMs, int capMs, CompletableFuture<Void> ct) throws Exception {
        if (maxTries < 1) throw new IllegalArgumentException("maxTries phải ≥ 1");
        for (int k = 0; ; k++) {
            try {
                return op.call(ct);                                 // dòng 4
            } catch (TimeoutException | Khung.NetError e) {         // dòng 5: CancellationException không bắt
                boolean ok = e instanceof TimeoutException || ((Khung.NetError) e).retry;   // dòng 6
                if (!ok || k == maxTries - 1) throw e;              // dòng 7
                long limit = Math.min(capMs, baseMs * (1L << k));
                double delay = ThreadLocalRandom.current().nextDouble() * limit;   // dòng 8: jitter
                if (e instanceof Khung.NetError ne && ne.retryAfter != null)
                    delay = Math.max(delay, ne.retryAfter);         // dòng 9–11: server đề nghị
                String code = e instanceof Khung.NetError ne ? ne.code : "TIMEOUT";
                log(String.format("  lỗi %s (có thể thử lại); chờ %.0f ms, giới hạn trên %d ms", code, delay, limit));
                try {                                               // dòng 12: DELAY_ASYNC(delay, ct) – hủy được
                    ct.get((long) delay, TimeUnit.MILLISECONDS);
                    throw new CancellationException("Đã hủy");
                } catch (TimeoutException waited) { /* hết thời gian lùi: thử lại */ }
            }
        }
    }

    /** Tạo op(ct): mỗi lần gọi là một lần thử mới (kết nối mới, yêu cầu mới). */
    static Op<Map<?, ?>> makeOp(String host, int port, Map<String, Object> req, int deadlineMs) {
        int[] tries = {0};
        return ct -> {
            log("Lần thử " + (++tries[0]) + ": gửi " + Json.stringify(req));
            try (Socket s = Khung.safeConnect(host, port, 3000)) {
                Map<?, ?> resp = requestWithDeadline(s, req, deadlineMs, ct);   // có thể ném TimeoutException
                if (Long.valueOf(503).equals(resp.get("status")))
                    throw new Khung.NetError("UNAVAILABLE", true, null, (Long) resp.get("retry_after_ms"));
                return resp;
            }
        };
    }

    static void scenario(ExecutorService vexec, String title, Function<CompletableFuture<Void>, Callable<Map<?, ?>>> run, int cancelMs) {
        t0 = System.nanoTime();
        System.out.println("[client] === " + title + " ===");
        CompletableFuture<Void> user = cancelMs > 0 ? Khung.cancelAfter(cancelMs) : Khung.newCancellation();  // nút Hủy
        Callable<Map<?, ?>> task = run.apply(user);
        try {
            Map<?, ?> r = CompletableFuture.supplyAsync(() -> {
                try { return task.call(); }
                catch (CancellationException e) { throw e; }
                catch (Exception e) { throw new CompletionException(e); }
            }, vexec).join();                                       // AWAIT
            log("Thành công: " + Json.stringify(r));
        } catch (CancellationException e) {
            log("Người dùng đã hủy (Cancelled) – không thử lại");
        } catch (CompletionException e) {
            if (e.getCause() instanceof CancellationException)
                log("Người dùng đã hủy (Cancelled) – không thử lại");
            else if (e.getCause() instanceof Khung.NetError ne)
                log("Thất bại, không thử lại: " + ne.code + " (retry=" + (ne.retry ? "True" : "False") + ")");
            else if (e.getCause() instanceof TimeoutException te)
                log("Thất bại sau lần thử cuối: " + te.getMessage());
            else log("Lỗi: " + e.getCause());
        }
    }

    public static void main(String[] args) {
        Object[] ep = Khung.parseEndpoint(java.util.Arrays.copyOf(args, Math.min(args.length, 2)), "127.0.0.1", 9100);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        int bad = args.length > 2 ? Integer.parseInt(args[2]) : 9101;
        try (ExecutorService vexec = Executors.newVirtualThreadPerTaskExecutor()) {
            scenario(vexec, "Kịch bản 1: hạn chót 1000 ms, thử lại tối đa 5 lần (base 200, cap 5000)",
                    ct -> () -> retryAsync(makeOp(host, port, Json.obj("op", "lookup", "term", "TCP"), 1000), 5, 200, 5000, ct), 0);
            scenario(vexec, "Kịch bản 2: server xử lý 3000 ms, hạn chót 5000 ms, người dùng bấm Hủy sau 500 ms",
                    ct -> () -> retryAsync(makeOp(host, port, Json.obj("op", "lookup", "term", "UDP", "delay_ms", 3000), 5000), 5, 200, 5000, ct), 500);
            scenario(vexec, "Kịch bản 3: cổng " + bad + " không có server (REFUSED)",
                    ct -> () -> retryAsync(makeOp(host, bad, Json.obj("op", "lookup", "term", "TCP"), 1000), 5, 200, 5000, ct), 0);
        }
    }
}
