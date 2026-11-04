package main

import (
	"context"
	"fmt"
	"net"
	"strconv"
	"sync"
	"time"
)

// Kiểm tra nhiều cổng đồng thời có giới hạn – Mã giả 5.2 (Mục 5.2.5): SCAN_PORTS, CHECK_ONE.
// Kế thừa Mã giả 1.2 (PORT_CHECK): PortCheckAsync có cùng thân hàm, CONNECT được thay bằng
// lời gọi kết nối trong goroutine riêng. Chương trình quét cùng một dải cổng hai lần: tuần tự
// (max_parallel = 1) và đồng thời (max_parallel = k), rồi in thời gian của mỗi lần.

// PortResult – cặp (p, trạng thái) mà CHECK_ONE trả về.
type PortResult struct {
	Port  int
	State string
}

// PortCheckAsync – Mã giả 1.2 chạy trong goroutine: OPEN, CLOSED, FILTERED hoặc UNRESOLVED.
func PortCheckAsync(ctx context.Context, host string, port int, timeoutMs int) string {
	addrs, err := net.DefaultResolver.LookupIPAddr(ctx, host) // RESOLVE_HOST
	if err != nil {
		return "UNRESOLVED"
	}
	state := "FILTERED" // chưa địa chỉ nào phản hồi
	for _, a := range addrs {
		network := "tcp4"
		if a.IP.To4() == nil {
			network = "tcp6"
		}
		d := net.Dialer{Timeout: time.Duration(timeoutMs) * time.Millisecond}                         // CONNECT_TIMEOUT
		conn, err := d.DialContext(ctx, network, net.JoinHostPort(a.IP.String(), strconv.Itoa(port))) // AWAIT CONNECT_ASYNC
		if err == nil {
			conn.Close()  // FINALLY CLOSE(s)
			return "OPEN" // bắt tay ba bước thành công
		}
		if isRefused(err) {
			state = "CLOSED" // nhận gói RST
		}
		// hết thời gian chờ, không có đường đi...: thử địa chỉ tiếp theo
	}
	return state
}

// CheckOne – Mã giả 5.2, dòng 9–16: CHECK_ONE(host, p, gate).
func CheckOne(ctx context.Context, host string, p int, gate chan struct{}) PortResult {
	gate <- struct{}{}        // ACQUIRE_ASYNC(gate): chờ khi đã đủ max_parallel lượt
	defer func() { <-gate }() // FINALLY RELEASE(gate)
	return PortResult{p, PortCheckAsync(ctx, host, p, 1000)}
}

// ScanPorts – Mã giả 5.2, dòng 1–8: SCAN_PORTS(host, ports, max_parallel).
func ScanPorts(ctx context.Context, host string, ports []int, maxParallel int) []PortResult {
	gate := make(chan struct{}, maxParallel) // SEMAPHORE(max_parallel): kênh có bộ đệm k chỗ
	results := make([]PortResult, len(ports))
	var wg sync.WaitGroup
	for i, p := range ports { // FOR EACH p IN ports
		wg.Add(1)
		go func() { // APPEND(tasks, CHECK_ONE(...)): khởi động goroutine, chưa chờ
			defer wg.Done()
			results[i] = CheckOne(ctx, host, p, gate)
		}()
	}
	wg.Wait() // AWAIT WHEN_ALL(tasks): chờ tất cả hoàn tất
	return results
}

// RunScan – quét tuần tự rồi đồng thời và in thời gian.
func RunScan(args []string) int {
	host, first, last, k := "127.0.0.1", 9000, 9019, 10
	if len(args) > 0 {
		host = args[0]
	}
	nums := []*int{&first, &last, &k}
	for i, ptr := range nums {
		if len(args) > i+1 {
			v, err := strconv.Atoi(args[i+1])
			if err != nil || v < 1 || (i < 2 && v > 65535) {
				fmt.Printf("Tham số không hợp lệ: %s\n", args[i+1])
				return 2
			}
			*ptr = v
		}
	}
	var ports []int
	for p := first; p <= last; p++ {
		ports = append(ports, p)
	}
	ctx := context.Background()
	fmt.Printf("[scan] Quét %d cổng %d–%d trên %s, thời gian chờ 1000 ms\n", len(ports), first, last, host)
	t0 := time.Now()
	seq := ScanPorts(ctx, host, ports, 1)
	tSeq := time.Since(t0).Seconds()
	t0 = time.Now()
	par := ScanPorts(ctx, host, ports, k)
	tPar := time.Since(t0).Seconds()
	counts := map[string]int{}
	same := len(seq) == len(par)
	for i, r := range par {
		if r.State != "CLOSED" { // chỉ in cổng không đóng
			fmt.Printf("[scan]   cổng %d: %s\n", r.Port, r.State)
		}
		counts[r.State]++
		same = same && seq[i] == r
	}
	fmt.Printf("[scan] Tổng hợp: OPEN=%d, CLOSED=%d, FILTERED=%d\n", counts["OPEN"], counts["CLOSED"], counts["FILTERED"])
	sameText := "không"
	if same {
		sameText = "có"
	}
	fmt.Printf("[scan] Hai lần quét cho cùng kết quả: %s\n", sameText)
	fmt.Printf("[scan] Tuần tự   (max_parallel=1):  %.2f s\n", tSeq)
	fmt.Printf("[scan] Đồng thời (max_parallel=%d): %.2f s\n", k, tPar)
	return 0
}
