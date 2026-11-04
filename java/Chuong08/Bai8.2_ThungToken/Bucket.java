import java.util.concurrent.locks.ReentrantLock;

/**
 * Bài 8.2 – Mã giả 8.3: STRUCT Bucket { cap, rate, tokens, last, lock } và ACQUIRE_TOKENS (Mục 8.2.4).
 * Java dùng luồng ảo: mã chặn (lock, Thread.sleep) thay cho ASYNC_LOCK, DELAY_ASYNC (Mục 5.2.3).
 */
public class Bucket {
    final double cap, rate;
    double tokens, last;
    final ReentrantLock lock = new ReentrantLock();             // khóa dùng với luồng ảo

    public Bucket(double cap, double rate) {
        this.cap = cap;
        this.rate = rate;
        this.tokens = cap;                                       // thùng đầy lúc khởi tạo
        this.last = nowSeconds();
    }

    static double nowSeconds() { return System.nanoTime() / 1e9; }   // NOW_SECONDS()

    public void acquireTokens(double n) throws InterruptedException {
        if (n > cap) throw new IllegalArgumentException("n vượt dung lượng");   // dòng 3
        while (true) {                                                          // dòng 4: LOOP
            double waitMs;
            lock.lock();                                                        // dòng 5: WITH b.lock DO
            try {
                double now = nowSeconds();                                      // dòng 6
                double refill = (now - last) * rate;                            // dòng 7
                tokens = Math.min(cap, tokens + refill);                        // dòng 8: không vượt cap
                last = now;                                                     // dòng 9
                if (tokens >= n) { tokens -= n; return; }                       // dòng 10–12
                waitMs = (n - tokens) / rate * 1000;                            // dòng 13
            } finally {
                lock.unlock();                                                  // dòng 14: END WITH
            }
            Thread.sleep((long) Math.ceil(waitMs));                             // dòng 15: chờ ngoài khóa
        }
    }
}
