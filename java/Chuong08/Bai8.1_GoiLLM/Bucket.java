import java.util.concurrent.locks.ReentrantLock;

/** Mã giả 8.3 – thùng token (chép gọn từ Bài 8.2): khóa ngắn, chờ ngoài khóa. */
public class Bucket {
    final double cap, rate;
    double tokens, last;
    final ReentrantLock lock = new ReentrantLock();             // ASYNC_LOCK()

    public Bucket(double cap, double rate) {
        this.cap = cap; this.rate = rate; this.tokens = cap; this.last = System.nanoTime() / 1e9;
    }

    public void acquireTokens(double n) throws InterruptedException {
        if (n > cap) throw new IllegalArgumentException("n vượt dung lượng");
        while (true) {
            double waitMs;
            lock.lock();
            try {
                double now = System.nanoTime() / 1e9;
                tokens = Math.min(cap, tokens + (now - last) * rate);
                last = now;
                if (tokens >= n) { tokens -= n; return; }
                waitMs = (n - tokens) / rate * 1000;
            } finally {
                lock.unlock();
            }
            Thread.sleep((long) Math.ceil(waitMs));                // không giữ khóa khi chờ
        }
    }
}
