// Bài 8.1 – Mã giả 8.3 ACQUIRE_TOKENS (chép gọn từ Bài 8.2).
//
// Thùng token để client tự giới hạn tần suất gọi LLM (Mục 8.2.4). Được llm_client.go dùng.
package main

import (
	"errors"
	"sync"
	"time"
)

// Bucket – STRUCT Bucket { cap, rate, tokens, last, lock }.
type Bucket struct {
	cap, rate, tokens float64
	last              time.Time
	lock              sync.Mutex
}

func newBucket(cap, rate float64) *Bucket {
	return &Bucket{cap: cap, rate: rate, tokens: cap, last: time.Now()} // thùng đầy lúc khởi tạo
}

// acquireTokens – ACQUIRE_TOKENS(b, n).
func acquireTokens(b *Bucket, n float64) error {
	if n > b.cap {
		return errors.New("n vượt dung lượng") // ArgumentError
	}
	for {
		b.lock.Lock()
		now := time.Now()
		b.tokens = min(b.cap, b.tokens+now.Sub(b.last).Seconds()*b.rate)
		b.last = now
		if b.tokens >= n {
			b.tokens -= n
			b.lock.Unlock()
			return nil
		}
		waitMs := (n - b.tokens) / b.rate * 1000
		b.lock.Unlock()
		time.Sleep(time.Duration(waitMs * float64(time.Millisecond))) // không giữ khóa khi chờ
	}
}
