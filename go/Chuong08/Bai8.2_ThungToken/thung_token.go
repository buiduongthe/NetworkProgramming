// Bài 8.2 – Thùng token giới hạn tần suất.
//
// Cài đặt Mã giả 8.3 – ACQUIRE_TOKENS (Mục 8.2.4, Giáo trình Lập trình mạng).
// Thùng dung lượng C, nạp lại R đơn vị/giây; mỗi yêu cầu lấy n đơn vị, thiếu thì chờ.
// Chương trình cho nhiều tác vụ (goroutine) đồng thời gửi một chuỗi yêu cầu qua cùng một thùng và
// in thời điểm mỗi yêu cầu được phép, so sánh với lý thuyết: C yêu cầu đầu đi ngay,
// yêu cầu thứ m > C được phép sau (m − C)/R giây.
//
// Chạy:  out/app [C] [R] [số_yêu_cầu] [số_tác_vụ]      (mặc định 10 2 14 2)
package main

import (
	"errors"
	"fmt"
	"os"
	"runtime"
	"sort"
	"strconv"
	"sync"
	"time"
)

// Bucket – STRUCT Bucket { cap, rate, tokens, last, lock ← ASYNC_LOCK() }  (Mã giả 8.3, dòng 1).
type Bucket struct {
	cap    float64
	rate   float64
	tokens float64
	last   time.Time
	lock   sync.Mutex // LOCK: chỉ một goroutine cập nhật thùng tại một thời điểm
}

func newBucket(cap, rate float64) *Bucket {
	return &Bucket{cap: cap, rate: rate, tokens: cap, last: time.Now()} // thùng đầy lúc khởi tạo
}

// ArgumentError của mã giả.
var ArgumentError = errors.New("n vượt dung lượng")

// acquireTokens – Mã giả 8.3, ACQUIRE_TOKENS(b, n).
func acquireTokens(b *Bucket, n float64) error {
	if n > b.cap { // dòng 3
		return ArgumentError
	}
	for { // dòng 4: LOOP
		b.lock.Lock()                                // dòng 5: WITH b.lock DO
		now := time.Now()                            // dòng 6: NOW_SECONDS()
		refill := now.Sub(b.last).Seconds() * b.rate // dòng 7
		b.tokens = min(b.cap, b.tokens+refill)       // dòng 8: không vượt cap
		b.last = now                                 // dòng 9
		if b.tokens >= n {                           // dòng 10–12
			b.tokens -= n
			b.lock.Unlock()
			return nil
		}
		waitMs := (n - b.tokens) / b.rate * 1000                      // dòng 13
		b.lock.Unlock()                                               // END WITH
		time.Sleep(time.Duration(waitMs * float64(time.Millisecond))) // dòng 15: chờ ngoài khóa
	}
}

type grant struct {
	t   float64 // thời điểm được phép (giây kể từ lúc bắt đầu)
	wid int     // tác vụ nào
}

// worker: một tác vụ gửi count yêu cầu, mỗi yêu cầu cần 1 đơn vị.
func worker(b *Bucket, wid, count int, t0 time.Time, mu *sync.Mutex, granted *[]grant) {
	for i := 0; i < count; i++ {
		acquireTokens(b, 1)
		mu.Lock()
		*granted = append(*granted, grant{time.Since(t0).Seconds(), wid}) // ghi lại thời điểm được phép
		mu.Unlock()
		runtime.Gosched() // nhường lượt: giả lập gửi yêu cầu
	}
}

func numArg(args []string, i int, def float64) float64 {
	if len(args) <= i {
		return def
	}
	v, err := strconv.ParseFloat(args[i], 64)
	if err != nil || v <= 0 {
		fmt.Fprintln(os.Stderr, "Tham số phải là số dương: "+args[i])
		os.Exit(1)
	}
	return v
}

func main() {
	args := os.Args[1:]
	capacity := numArg(args, 0, 10)
	rate := numArg(args, 1, 2)
	total := int(numArg(args, 2, 14))
	tasks := int(numArg(args, 3, 2))
	b := newBucket(capacity, rate)
	fmt.Printf("[bucket] Thùng: dung lượng C = %g, tốc độ nạp R = %g đơn vị/giây, mỗi yêu cầu n = 1\n", capacity, rate)
	fmt.Printf("[bucket] Gửi %d yêu cầu từ %d tác vụ đồng thời (thùng đầy lúc bắt đầu)\n", total, tasks)
	t0 := time.Now()
	var mu sync.Mutex
	var granted []grant
	var wg sync.WaitGroup
	for w := 0; w < tasks; w++ {
		count := total / tasks
		if w < total%tasks {
			count++
		}
		wg.Add(1)
		go func(wid, count int) {
			defer wg.Done()
			worker(b, wid, count, t0, &mu, &granted)
		}(w+1, count)
	}
	wg.Wait() // chờ mọi tác vụ kết thúc
	sort.Slice(granted, func(i, j int) bool { return granted[i].t < granted[j].t })
	for m, g := range granted {
		theory := max(0, (float64(m+1)-capacity)/rate)
		fmt.Printf("[bucket] Yêu cầu %02d (tác vụ %d) được phép tại t = %6.3f s   lý thuyết %6.3f s\n", m+1, g.wid, g.t, theory)
	}
	fmt.Printf("[bucket] Thử lấy n = %g > C = %g đơn vị\n", capacity+1, capacity)
	if err := acquireTokens(b, capacity+1); err != nil { // dòng 3: báo lỗi ngay, không chờ mãi
		fmt.Printf("[bucket] Lỗi ArgumentError: %v\n", err)
	}
}
