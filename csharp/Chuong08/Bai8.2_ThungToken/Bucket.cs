using System.Diagnostics;

namespace Bai8_2;

// Mã giả 8.3 – STRUCT Bucket { cap, rate, tokens, last, lock ← ASYNC_LOCK() } và ACQUIRE_TOKENS (Mục 8.2.4)
class Bucket(double cap, double rate)
{
    public readonly double Cap = cap, Rate = rate;
    double tokens = cap;                                         // thùng đầy lúc khởi tạo
    double last = NowSeconds();
    readonly SemaphoreSlim lockObj = new(1, 1);                  // ASYNC_LOCK(): khóa dùng được với await

    static double NowSeconds() => Stopwatch.GetTimestamp() / (double)Stopwatch.Frequency;   // NOW_SECONDS()

    public async Task AcquireTokens(double n)
    {
        if (n > Cap) throw new ArgumentException("n vượt dung lượng");          // dòng 3
        while (true)                                                            // dòng 4: LOOP
        {
            double waitMs;
            await lockObj.WaitAsync();                                          // dòng 5: WITH b.lock DO
            try
            {
                double now = NowSeconds();                                      // dòng 6
                double refill = (now - last) * Rate;                            // dòng 7
                tokens = Math.Min(Cap, tokens + refill);                        // dòng 8: không vượt cap
                last = now;                                                     // dòng 9
                if (tokens >= n) { tokens -= n; return; }                       // dòng 10–12
                waitMs = (n - tokens) / Rate * 1000;                            // dòng 13
            }
            finally
            {
                lockObj.Release();                                              // dòng 14: END WITH
            }
            await Task.Delay(TimeSpan.FromMilliseconds(waitMs));                // dòng 15: chờ ngoài khóa
        }
    }
}
