// Bài 6.7 – Bộ sinh tải vòng kín đo thông lượng và phân vị độ trễ p50, p95, p99.
//
// Cài đặt Mã giả 6.9 (Mục 6.5.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 5.2 (khởi động N tác vụ
// rồi WHEN_ALL = sync.WaitGroup) và các hàm khung JSON của Mã giả 3.6/5.1 (netutil.go).
// Chạy lần lượt các mức tải (số client) trong danh sách levels và in bảng kết quả.
//
// Chạy:  out/app load [host] [port] [n_requests] [levels]   (mặc định 127.0.0.1 9000 20 1,10,100)
package main

import (
	"fmt"
	"math"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// percentile – PERCENTILE theo hạng gần nhất: giá trị mà p% số mẫu không vượt quá.
func percentile(values []float64, p float64) float64 {
	if len(values) == 0 {
		return 0
	}
	s := append([]float64(nil), values...)
	sort.Float64s(s)
	k := int(math.Ceil(p/100*float64(len(s)))) - 1
	return s[max(0, k)]
}

type result struct {
	ok, errors         int
	rps, p50, p95, p99 float64
}

// loadTest – LOAD_TEST(host, port, n_clients, n_requests).
func loadTest(host string, port int, nClients int, nRequests int) result {
	var mu sync.Mutex // CONCURRENT_LIST: danh sách lat có khóa (các goroutine chạy song song thật)
	var lat []float64
	var errors atomic.Int64 // errors ← ATOMIC_COUNTER(0)
	req := map[string]any{"op": "lookup", "term": "TCP"}

	oneClient := func() { // ONE_CLIENT()
		s, err := safeConnect(host, port, 3000) // SAFE_CONNECT_ASYNC(host, port, 3000)
		if err != nil {
			errors.Add(1) // CATCH NetError: ghi nhận, không dừng cả bài
			return
		}
		defer s.Close() // FINALLY: IF s ≠ NULL THEN CLOSE(s)
		s.SetDeadline(time.Now().Add(30 * time.Second))
		for j := 1; j <= nRequests; j++ {
			t0 := time.Now() // NOW_PRECISE()
			if err := sendJSON(s, req); err != nil {
				errors.Add(1) // CATCH NetError, ConnectionClosed, ProtocolError
				return
			}
			r, err := recvJSON(s)
			if err != nil {
				errors.Add(1)
				return
			}
			if r["status"] != 200.0 { // ví dụ 503: server bận
				errors.Add(1)
				return // IF r.status ≠ 200 THEN INCREMENT(errors); BREAK (client dừng, đếm một lần)
			}
			ms := float64(time.Since(t0).Microseconds()) / 1000
			mu.Lock()
			lat = append(lat, ms) // APPEND(lat, NOW_PRECISE() − t0)
			mu.Unlock()
		}
	}

	t := time.Now()
	var wg sync.WaitGroup
	for i := 1; i <= nClients; i++ {
		wg.Add(1)
		go func() { defer wg.Done(); oneClient() }()
	}
	wg.Wait() // WHEN_ALL
	sec := time.Since(t).Seconds()
	return result{ok: len(lat), errors: int(errors.Load()), rps: float64(len(lat)) / sec,
		p50: percentile(lat, 50), p95: percentile(lat, 95), p99: percentile(lat, 99)}
}

func loadMain(args []string) {
	host, port := parseEndpoint(args, "127.0.0.1", 9000)
	nRequests := intArg(args, 2, 20)
	levelsText := "1,10,100"
	if len(args) > 3 {
		levelsText = args[3]
	}
	var levels []int
	for _, x := range strings.Split(levelsText, ",") {
		n, err := strconv.Atoi(strings.TrimSpace(x))
		if err != nil {
			fail("Mức tải phải là số nguyên: " + x)
		}
		levels = append(levels, n)
	}
	fmt.Printf("[load] Server %s:%d, %d yêu cầu/client, vòng kín\n", host, port, nRequests)
	fmt.Printf("[load] %6s %6s %5s %8s %7s %7s %7s\n", "client", "OK", "lỗi", "yc/giây", "p50", "p95", "p99")
	for _, n := range levels {
		r := loadTest(host, port, n, nRequests)
		fmt.Printf("[load] %6d %6d %5d %8.1f %7.1f %7.1f %7.1f\n", n, r.ok, r.errors, r.rps, r.p50, r.p95, r.p99)
	}
	fmt.Println("[load] OK: số yêu cầu thành công; lỗi: số lần lỗi (phản hồi khác 200, lỗi kết nối)")
	fmt.Println("[load] p50, p95, p99: phân vị độ trễ, tính bằng ms")
}
