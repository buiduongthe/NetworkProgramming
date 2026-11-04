// Bài 6.3 – Nhiều client echo đồng thời để kiểm tra server reactor.
//
// Mỗi client (một goroutine) dựa trên Mã giả 3.2 nhưng nhận bằng RECV_EXACT (Mã giả 3.4): gửi một dòng
// chào, nhận lại đúng số byte đã gửi; sau đó gửi một khối lớn (mặc định 1 MiB) trong một goroutine phụ,
// đồng thời nhận lại đủ khối ở goroutine chính và so khớp. Khối lớn hơn MAX_BUF của server nên kiểm tra
// được việc gửi một phần và áp lực ngược (server ngừng đọc khi bộ đệm gửi đầy). Gửi xong, client đóng
// chiều gửi (half-close) để kiểm tra tập closing của Mã giả 6.4: server gửi nốt dữ liệu rồi mới đóng.
//
// Chạy:  out/app client [host] [port] [n] [size]   (mặc định 127.0.0.1 9000 3 1048576)
package main

import (
	"bytes"
	"crypto/rand"
	"fmt"
	"net"
	"sync"
	"time"
)

func oneClient(i int, host string, port int, size int) {
	s, err := safeConnect(host, port, 3000)
	if err != nil {
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	defer s.Close()
	s.SetDeadline(time.Now().Add(10 * time.Second))
	hello := []byte(fmt.Sprintf("Xin chào reactor từ client %d", i))
	if _, err := s.Write(hello); err != nil { // SEND_ALL
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	back, err := recvExact(s, len(hello))
	if err != nil {
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	logLine("[client %d] Nhận lại: '%s'", i, back)
	block := make([]byte, size)
	rand.Read(block)
	done := make(chan struct{})
	go func() { // gửi song song với nhận
		s.Write(block)
		s.(*net.TCPConn).CloseWrite() // đóng chiều gửi (half-close, gửi FIN): server vẫn phải gửi nốt phần còn lại
		close(done)
	}()
	got, err := recvExact(s, size)
	<-done
	if err != nil {
		logLine("[client %d] Lỗi: %v", i, err)
		return
	}
	match := "không"
	if bytes.Equal(got, block) {
		match = "có"
	}
	logLine("[client %d] Gửi %d byte, nhận lại %d byte, khớp: %s", i, size, len(got), match)
}

func echoClients(args []string) {
	host, port := parseEndpoint(args, "127.0.0.1", 9000)
	n := intArg(args, 2, 3)
	size := intArg(args, 3, 1048576)
	logLine("[client] Mở %d kết nối đồng thời, mỗi kết nối gửi thêm %d byte", n, size)
	t0 := time.Now()
	var wg sync.WaitGroup
	for i := 1; i <= n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			oneClient(i, host, port, size)
		}(i)
	}
	wg.Wait()
	logLine("[client] Hoàn tất sau %d ms", time.Since(t0).Milliseconds())
}
