package main

import (
	"bufio"
	"context"
	"fmt"
	"net"
	"runtime"
	"sync"
	"time"
)

const (
	nNet, nCPU = 20, 8
	pause      = 500 * time.Millisecond // nghỉ giữa hai mô hình để máy chủ thử báo từng "đợt" riêng
)

// ---------------- việc mạng và việc CPU ----------------

// fetch – một yêu cầu mạng: kết nối, gửi GET, chờ dòng trả lời. Trong Go lời gọi này "chặn"
// goroutine, nhưng không chặn luồng hệ điều hành: bộ lập lịch đặt goroutine vào netpoller và
// chạy goroutine khác trên luồng đó.
func fetch(ctx context.Context, addr string, k int) (string, error) {
	var d net.Dialer
	c, err := d.DialContext(ctx, "tcp", addr)
	if err != nil {
		return "", err
	}
	defer c.Close()
	if dl, ok := ctx.Deadline(); ok {
		c.SetDeadline(dl)
	}
	fmt.Fprintf(c, "GET %d\n", k)
	return bufio.NewReader(c).ReadString('\n')
}

// cpuWork – việc tính toán (bộ sinh đồng dư tuyến tính) giống bản Python.
func cpuWork(n int) uint32 {
	x := uint32(1)
	for range n {
		x = (x*1103515245 + 12345) & 0x7FFFFFFF
	}
	return x
}

var sink uint32 // giữ kết quả để trình biên dịch không bỏ vòng lặp

// calibrate – số vòng lặp để cpuWork chạy ≈ ms mili giây trên máy này.
func calibrate(ms int) int {
	n, dt := 20_000_000, 1.0
	for range 3 { // đo 3 lần, lấy lần nhanh nhất
		t := time.Now()
		sink += cpuWork(n)
		dt = min(dt, time.Since(t).Seconds())
	}
	return int(float64(n) * float64(ms) / 1000 / dt)
}

// ---------------- khối việc 1: mạng ----------------
func runNet(host, port string) {
	addr := net.JoinHostPort(host, port)
	ctx := context.Background()
	fmt.Printf("Khối việc: %d yêu cầu mạng, mỗi yêu cầu máy chủ trễ 200 ms\n", nNet)
	var results []result
	timed := func(rec *Recorder, worker string, k int) {
		t := time.Now()
		fetch(ctx, addr, k)
		rec.Add(k, worker, t, time.Now())
	}

	// 1. Tuần tự
	rec := NewRecorder()
	for k := range nNet {
		timed(rec, "chính", k)
	}
	results = append(results, result{"1. tuần tự", rec, "goroutine"})
	time.Sleep(pause)

	// 2. Mỗi tác vụ một goroutine, chờ tất cả bằng WaitGroup
	rec, names := NewRecorder(), &Names{prefix: "goroutine"}
	var wg sync.WaitGroup
	for k := range nNet {
		wg.Add(1)
		go func() {
			defer wg.Done()
			timed(rec, names.Next(), k)
		}()
	}
	wg.Wait()
	results = append(results, result{"2. mỗi tác vụ một goroutine", rec, "goroutine"})
	time.Sleep(pause)

	// 3. Nhóm worker: 5 goroutine lấy việc từ một kênh
	rec = NewRecorder()
	jobs := make(chan int)
	for w := 1; w <= 5; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for k := range jobs {
				timed(rec, fmt.Sprintf("worker-%d", w), k)
			}
		}()
	}
	for k := range nNet {
		jobs <- k
	}
	close(jobs)
	wg.Wait()
	results = append(results, result{"3. nhóm worker (5 goroutine)", rec, "worker"})
	time.Sleep(pause)

	// 4. Goroutine + kênh kết quả (fan-out/fan-in), hạn chót chung 2 s bằng context
	rec, names = NewRecorder(), &Names{prefix: "goroutine"}
	ctx2, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	out := make(chan error, nNet)
	for k := range nNet {
		go func() {
			w := names.Next()
			t := time.Now()
			_, err := fetch(ctx2, addr, k)
			rec.Add(k, w, t, time.Now())
			out <- err
		}()
	}
	ok := 0
	for range nNet {
		select {
		case err := <-out:
			if err == nil {
				ok++
			}
		case <-ctx2.Done():
		}
	}
	results = append(results, result{"4. goroutine + kênh + context", rec, "goroutine"})
	show(results)
	fmt.Printf("Mô hình 4 nhận đủ %d/%d kết quả qua kênh trước hạn chót.\n", ok, nNet)
	fmt.Println("Nhận xét: việc chờ I/O không tốn CPU; goroutine rất nhẹ (vài KB ngăn")
	fmt.Println("  xếp) nên mỗi yêu cầu một goroutine là cách viết tự nhiên trong Go –")
	fmt.Println("  bộ lập lịch tự ghép hàng nghìn goroutine lên vài luồng hệ điều hành.")
}

// ---------------- khối việc 2: CPU ----------------
func runCPU() {
	n := calibrate(100)
	ncpu := runtime.NumCPU()
	fmt.Printf("Khối việc: %d tác vụ CPU, mỗi tác vụ ≈ 100 ms\n", nCPU)
	fmt.Printf("Máy có %d CPU; GOMAXPROCS = %d; Go không có GIL\n", ncpu, runtime.GOMAXPROCS(0))
	var results []result
	rec := NewRecorder()
	for k := range nCPU {
		t := time.Now()
		sink += cpuWork(n)
		rec.Add(k, "chính", t, time.Now())
	}
	results = append(results, result{"1. tuần tự", rec, "goroutine"})

	rec, names := NewRecorder(), &Names{prefix: "goroutine"}
	var wg sync.WaitGroup
	var mu sync.Mutex
	for k := range nCPU {
		wg.Add(1)
		go func() {
			defer wg.Done()
			w := names.Next()
			t := time.Now()
			x := cpuWork(n)
			rec.Add(k, w, t, time.Now())
			mu.Lock()
			sink += x
			mu.Unlock()
		}()
	}
	wg.Wait()
	results = append(results, result{"2. mỗi tác vụ một goroutine", rec, "goroutine"})

	rec = NewRecorder()
	jobs := make(chan int)
	for w := 1; w <= ncpu; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for k := range jobs {
				t := time.Now()
				x := cpuWork(n)
				rec.Add(k, fmt.Sprintf("worker-%d", w), t, time.Now())
				mu.Lock()
				sink += x
				mu.Unlock()
			}
		}()
	}
	for k := range nCPU {
		jobs <- k
	}
	close(jobs)
	wg.Wait()
	results = append(results, result{fmt.Sprintf("3. nhóm worker (%d = số CPU)", ncpu), rec, "worker"})
	show(results)
	fmt.Println("Nhận xét: không có GIL, các goroutine chạy song song thật trên mọi CPU;")
	fmt.Println("  mô hình 2 tạo 8 goroutine nhưng chỉ GOMAXPROCS goroutine chạy cùng")
	fmt.Println("  lúc (mỗi tác vụ bị kéo dài vì chia lượt); nhóm worker bằng số CPU")
	fmt.Println("  cho cùng tổng thời gian mà mỗi tác vụ chạy liền một mạch.")
}

// ---------------- khối việc 3: mạng + CPU ----------------
func runMixed(host, port string) {
	addr := net.JoinHostPort(host, port)
	n := calibrate(30)
	ncpu := runtime.NumCPU()
	fmt.Printf("Khối việc: %d tác vụ = yêu cầu mạng 200 ms + xử lý CPU ≈ 30 ms\n", nNet)
	fmt.Println("Đo thêm độ trễ lập lịch: goroutine \"nhịp tim\" ngủ 10 ms và đo xem")
	fmt.Println("  thực tế bị đánh thức muộn bao nhiêu (trễ lớn = ứng dụng đứng).")

	scenario := func(mode string, rec *Recorder) time.Duration {
		var lag time.Duration
		stop := make(chan struct{})
		hbDone := make(chan struct{})
		go func() { // nhịp tim
			defer close(hbDone)
			for {
				select {
				case <-stop:
					return
				default:
				}
				t := time.Now()
				time.Sleep(10 * time.Millisecond)
				lag = max(lag, time.Since(t)-10*time.Millisecond)
			}
		}()
		var mu sync.Mutex
		var wg sync.WaitGroup
		names := &Names{prefix: "goroutine"}
		cpuJobs := make(chan func(worker string))
		if mode == "pool" { // nhóm worker CPU: số worker = số CPU
			for w := 1; w <= ncpu; w++ {
				go func() {
					for job := range cpuJobs {
						job(fmt.Sprintf("worker-%d", w))
					}
				}()
			}
		}
		for k := range nNet {
			wg.Add(1)
			go func() {
				defer wg.Done()
				t := time.Now()
				fetch(context.Background(), addr, k)
				rec.Add(k, "chờ mạng", t, time.Now())
				work := func(worker string) {
					t := time.Now()
					x := cpuWork(n)
					rec.Add(k, worker, t, time.Now())
					mu.Lock()
					sink += x
					mu.Unlock()
				}
				if mode == "pool" {
					done := make(chan struct{})
					cpuJobs <- func(worker string) { work(worker); close(done) }
					<-done
				} else {
					work(names.Next()) // xử lý CPU ngay trong goroutine của tác vụ
				}
			}()
		}
		wg.Wait()
		close(cpuJobs)
		close(stop)
		<-hbDone
		return lag
	}

	var results []result
	var lags []time.Duration
	rec := NewRecorder()
	lags = append(lags, scenario("inline", rec))
	results = append(results, result{"A. CPU ngay trong goroutine", rec, "goroutine"})
	time.Sleep(pause)
	rec = NewRecorder()
	lags = append(lags, scenario("pool", rec))
	results = append(results, result{fmt.Sprintf("B. + nhóm worker CPU (%d)", ncpu), rec, "worker"})
	time.Sleep(pause)
	old := runtime.GOMAXPROCS(1) // chỉ một luồng chạy mã Go: giống asyncio một luồng
	rec = NewRecorder()
	lags = append(lags, scenario("inline", rec))
	runtime.GOMAXPROCS(old)
	results = append(results, result{"C. như A, GOMAXPROCS = 1", rec, "goroutine"})

	scale := 0.0
	for _, r := range results {
		scale = max(scale, r.rec.Total())
	}
	fmt.Println()
	for _, r := range results {
		gantt(r.name, r.rec, scale, r.kind)
	}
	axis(scale)
	fmt.Println()
	fmt.Printf("  %-30s %9s %13s\n", "Mô hình", "Thời gian", "Trễ lập lịch")
	for i, r := range results {
		fmt.Printf("  %-30s %7.2f s %10d ms\n", r.name, r.rec.Total(), lags[i].Milliseconds())
	}
	fmt.Println()
	fmt.Println("Nhận xét: \"chờ mạng\" là hàng chung của mọi goroutine đang đợi trong")
	fmt.Println("  netpoller (*). A, B – CPU chạy song song trên mọi CPU, nhịp tim gần")
	fmt.Println("  như không trễ; B giới hạn số việc CPU đồng thời. C – chỉ một luồng")
	fmt.Println("  nên CPU tuần tự như Python, nhưng bộ lập lịch Go tự ngắt (preempt)")
	fmt.Println("  goroutine chạy lâu nên nhịp tim chỉ trễ vài chục ms, không đứng hẳn.")
}
