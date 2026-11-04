// Bài 6.1 – Mở nhiều client tra cứu đồng thời để quan sát server đa luồng và nhóm luồng.
//
// Mỗi client chạy trong một goroutine, thực hiện LOOKUP_CLIENT (Mã giả 3.8) rút gọn: gửi một yêu cầu
// lookup "TCP", in phản hồi kèm thời điểm t tính từ lúc chương trình bắt đầu, giữ kết nối hold_ms
// (mô phỏng người dùng chưa rời đi) rồi gửi bye. Các client khởi động cách nhau 100 ms.
//
// Chạy:  out/app client [host] [port] [n] [hold_ms]   (mặc định 127.0.0.1 9000 5 1000)
package main

import (
	"errors"
	"sync"
	"syscall"
	"time"
)

// serverClosed: server đóng kết nối ngay sau ACCEPT (EOF, RST hoặc ống dẫn bị đứt).
func serverClosed(err error) bool {
	var cc *ConnectionClosed
	return errors.As(err, &cc) || errors.Is(err, syscall.ECONNRESET) || errors.Is(err, syscall.EPIPE)
}

func oneClient(i int, host string, port int, holdMs int, t0 time.Time) {
	t := func() int64 { return time.Since(t0).Milliseconds() }
	s, err := safeConnect(host, port, 3000)
	if err != nil {
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	defer s.Close()
	s.SetDeadline(time.Now().Add(5*time.Second + time.Duration(holdMs)*time.Millisecond)) // RECV_TIMEOUT 5000 ms
	err = sendJSON(s, map[string]any{"op": "lookup", "term": "TCP"})
	var resp map[string]any
	if err == nil {
		resp, err = recvJSON(s)
	}
	if serverClosed(err) {
		logLine("[client %d] t = %d ms: server đóng kết nối ngay", i, t())
		return
	}
	if err != nil {
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	if resp["status"] != 200.0 { // số JSON được giải mã thành float64
		logLine("[client %d] t = %d ms: bị từ chối %v – %v", i, t(), resp["status"], resp["error"])
		return
	}
	logLine("[client %d] t = %d ms: 200 %v – %v", i, t(), resp["term"], resp["text"])
	time.Sleep(time.Duration(holdMs) * time.Millisecond) // giữ kết nối
	sendJSON(s, map[string]any{"op": "bye"})
}

func multiClient(args []string) {
	host, port := parseEndpoint(args, "127.0.0.1", 9000)
	n := intArg(args, 2, 5)
	holdMs := intArg(args, 3, 1000)
	logLine("[client] %d client cách nhau 100 ms, giữ kết nối %d ms", n, holdMs)
	t0 := time.Now()
	var wg sync.WaitGroup
	for i := 1; i <= n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			oneClient(i, host, port, holdMs, t0)
		}(i)
		time.Sleep(100 * time.Millisecond)
	}
	wg.Wait() // chờ mọi client kết thúc
	logLine("[client] Hoàn tất sau %d ms", time.Since(t0).Milliseconds())
}
