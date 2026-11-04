/** NetError của Mã giả 2.2: mang cờ có thể thử lại và thời gian chờ server đề nghị (ms). */
public class NetError extends Exception {
    public final boolean retry;
    public final Double retryAfter;

    public NetError(String message) { this(message, false, null); }

    public NetError(String message, boolean retry, Double retryAfter) {
        super(message);
        this.retry = retry;
        this.retryAfter = retryAfter;
    }
}
