import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.PrintStream;
import java.net.ConnectException;
import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;
import java.net.http.HttpTimeoutException;
import java.nio.charset.StandardCharsets;
import java.time.Duration;
import java.time.ZonedDateTime;
import java.time.format.DateTimeFormatter;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.function.Consumer;

/**
 * Bài 8.1 – Gọi API LLM có truyền luồng SSE, giới hạn tần suất, thử lại và hạn chót.
 * Mã giả 8.2 CALL_LLM (Mục 8.2.3), kế thừa Mã giả 8.1, 5.8 (RETRY_ASYNC), 8.3 (thùng token).
 * Cấu hình: LLM_BASE_URL (mặc định http://127.0.0.1:9000/v1), LLM_API_KEY (bắt buộc), LLM_MODEL,
 *           LLM_PRICE_IN, LLM_PRICE_OUT (USD/1 triệu token, tùy chọn).
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out LlmClient "So sánh TCP và UDP."
 */
public class LlmClient {
    static final String BASE_URL = env("LLM_BASE_URL", "http://127.0.0.1:9000/v1").replaceAll("/+$", "");
    static final Bucket BUCKET = new Bucket(20000, 2000);      // hạn mức tự đặt: 20 000 token, nạp 2 000/giây
    static final HttpClient HTTP = HttpClient.newBuilder().connectTimeout(Duration.ofMillis(3000)).build();
    static final Set<Integer> RETRYABLE = Set.of(408, 429, 500, 502, 503, 504);
    static PrintStream out;

    static String env(String k, String def) { String v = System.getenv(k); return v == null || v.isEmpty() ? def : v; }

    /** SECRET(name): bí mật chỉ lấy từ biến môi trường, không ghi trong mã nguồn, không in ra. */
    static String secret(String name) {
        String v = System.getenv(name);
        if (v == null || v.isEmpty()) throw new IllegalStateException("Thiếu biến môi trường " + name);
        return v;
    }

    /** ESTIMATE_TOKENS: ước lượng thô ~4 ký tự/token cho đầu vào, cộng max_tokens cho đầu ra. */
    @SuppressWarnings("unchecked")
    static long estimateTokens(Map<String, Object> req) {
        long chars = 0;
        for (Object m : (List<Object>) req.get("messages")) chars += ((String) ((Map<String, Object>) m).get("content")).length();
        return chars / 4 + (Long) req.get("max_tokens");
    }

    /** PARSE_RETRY_AFTER: số giây hoặc ngày giờ HTTP (RFC 9110); trả về ms. */
    static Double parseRetryAfter(HttpResponse<?> resp) {
        String v = resp.headers().firstValue("Retry-After").orElse(null);
        if (v == null) return null;
        try {
            if (v.trim().matches("\\d+")) return Long.parseLong(v.trim()) * 1000.0;
            ZonedDateTime t = ZonedDateTime.parse(v.trim(), DateTimeFormatter.RFC_1123_DATE_TIME);
            return Math.max(0.0, t.toInstant().toEpochMilli() - System.currentTimeMillis());
        } catch (RuntimeException e) { return null; }
    }

    /** HTTPS_POST: HttpClient tự bọc TLS khi url là https://; timeout áp dụng tới khi có tiêu đề. */
    static HttpResponse<InputStream> httpsPost(String url, Map<String, Object> req, String auth, int timeoutMs,
                                               CompletableFuture<Void> ct) throws NetError, InterruptedException {
        if (ct.isDone()) throw new CancellationException("Đã hủy");    // tín hiệu hủy của người dùng
        HttpRequest rq = HttpRequest.newBuilder(URI.create(url)).timeout(Duration.ofMillis(timeoutMs))
                .header("Authorization", auth).header("Content-Type", "application/json")
                .header("Accept", "text/event-stream")
                .POST(HttpRequest.BodyPublishers.ofString(Json.stringify(req), StandardCharsets.UTF_8)).build();
        try {
            return HTTP.send(rq, HttpResponse.BodyHandlers.ofInputStream());
        } catch (HttpTimeoutException e) {                               // Timeout: luôn đáng thử lại
            throw new NetError("Hết thời gian chờ phản hồi", true, null);
        } catch (ConnectException e) {
            throw new NetError("REFUSED", false, null);
        } catch (IOException e) {
            throw new NetError("UNREACHABLE (" + e.getMessage() + ")", true, null);
        }
    }

    interface Op<T> { T run(CompletableFuture<Void> ct) throws NetError, InterruptedException; }

    /** Mã giả 5.8 – RETRY_ASYNC(op, max_tries, base_ms, cap_ms, ct); chạy chặn trên luồng hiện tại. */
    static <T> T retry(Op<T> op, int maxTries, double baseMs, double capMs, CompletableFuture<Void> ct)
            throws NetError, InterruptedException {
        for (int k = 0; ; k++) {
            try {
                return op.run(ct);                                          // dòng 4
            } catch (NetError e) {                                          // dòng 5 (Timeout đã là NetError retry);
                                                                            // CancellationException không bắt
                if (!e.retry || k == maxTries - 1) throw e;
                double delay = ThreadLocalRandom.current().nextDouble() * Math.min(capMs, baseMs * Math.pow(2, k));
                if (e.retryAfter != null) delay = Math.max(delay, e.retryAfter);   // server đề nghị
                out.printf(Locale.ROOT, "[llm] Lần thử %d/%d: %s – chờ %.0f ms rồi thử lại%n", k + 1, maxTries, e.getMessage(), delay);
                try {                                                       // DELAY_ASYNC(delay, ct): hủy được
                    ct.get((long) delay, TimeUnit.MILLISECONDS);
                    throw new CancellationException("Đã hủy");
                } catch (TimeoutException waited) { /* hết thời gian lùi: thử lại */ }
                catch (java.util.concurrent.ExecutionException ignored) { throw new CancellationException("Đã hủy"); }
            }
        }
    }

    /** Mã giả 8.2 – CALL_LLM(req, on_token, ct). */
    static LlmStream.State callLlm(Map<String, Object> req, Consumer<String> onToken, CompletableFuture<Void> ct)
            throws NetError, InterruptedException {
        boolean[] started = {false};
        Op<LlmStream.State> oneAttempt = c -> {                                    // dòng 3: ONE_ATTEMPT(ct)
            BUCKET.acquireTokens(estimateTokens(req));                             // 8.2.4
            String auth = "Bearer " + secret("LLM_API_KEY");
            HttpResponse<InputStream> resp = httpsPost(BASE_URL + "/chat/completions", req, auth, 30000, c);   // dòng 6–7
            try (InputStream body = resp.body()) {
                int status = resp.statusCode();
                if (RETRYABLE.contains(status))
                    throw new NetError("HTTP " + status, !started[0], parseRetryAfter(resp));
                if (status >= 400) throw new NetError("HTTP " + status);
                return LlmStream.streamCompletion(body, d -> { started[0] = true; onToken.accept(d); });
            } catch (IOException e) {
                throw new NetError("Lỗi đóng luồng: " + e.getMessage());
            }
        };
        return retry(oneAttempt, 5, 500, 20000, ct);                               // dòng 16
    }

    public static void main(String[] args) throws InterruptedException {
        out = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        String model = env("LLM_MODEL", "mock-model");
        String question = args.length > 0 ? String.join(" ", args) : "So sánh TCP và UDP.";
        Map<String, Object> req = Json.obj("model", model, "stream", true, "max_tokens", 512L, "messages", List.of(
                Json.obj("role", "system", "content", "Trợ lý mạng, trả lời ngắn."),
                Json.obj("role", "user", "content", question)));
        out.println("[llm] Mô hình " + model + " tại " + BASE_URL);
        out.println("[llm] Câu hỏi: " + question);
        try { secret("LLM_API_KEY"); } catch (IllegalStateException e) { out.println("[llm] " + e.getMessage()); System.exit(2); }
        long t0 = System.nanoTime();
        long[] first = {-1};
        Consumer<String> onToken = d -> {                                          // UI: UI_INVOKE(APPEND, d)
            if (first[0] < 0) { first[0] = System.nanoTime(); out.print("[llm] Trả lời: "); }
            out.print(d);
            out.flush();
        };
        LlmStream.State st;
        try {
            st = callLlm(req, onToken, new CompletableFuture<>());   // ct: chương trình dòng lệnh không hủy
        } catch (NetError e) {
            if (first[0] >= 0) out.println();
            String note = first[0] >= 0 ? " – không thử lại vì đã hiển thị một phần câu trả lời" : "";
            out.println("[llm] Lỗi: " + e.getMessage() + note);
            System.exit(1);
            return;
        }
        long total = System.nanoTime();
        out.println();
        out.printf(Locale.ROOT, "[llm] TTFT: %.0f ms, tổng thời gian: %.0f ms%n", (first[0] - t0) / 1e6, (total - t0) / 1e6);
        long nIn = st.usage == null ? 0 : ((Number) st.usage.getOrDefault("prompt_tokens", 0L)).longValue();
        long nOut = st.usage == null ? 0 : ((Number) st.usage.getOrDefault("completion_tokens", 0L)).longValue();
        out.println("[llm] usage: prompt_tokens = " + nIn + ", completion_tokens = " + nOut);
        String pIn = System.getenv("LLM_PRICE_IN"), pOut = System.getenv("LLM_PRICE_OUT");
        if (pIn != null && !pIn.isEmpty() && pOut != null && !pOut.isEmpty()) {   // Công thức (8.1)
            double cost = (nIn * Double.parseDouble(pIn) + nOut * Double.parseDouble(pOut)) / 1_000_000;
            out.printf(Locale.ROOT, "[llm] Chi phí ước tính: %d × %s + %d × %s (USD/1 triệu token) = %.8f USD%n", nIn, pIn, nOut, pOut, cost);
        }
    }
}
