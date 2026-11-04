package main

import (
	"context"
	"errors"
	"fmt"
	"net"
	"strconv"
	"sync"
	"time"
)

// Server tra cứu "khó tính" để chạy thử hạn chót và thử lại (không thuộc mã giả).
// Server dùng khung JSON của Mã giả 5.1 và cố ý gặp sự cố ở các yêu cầu đầu tiên:
//
//	yêu cầu #1: trả lời chậm 2000 ms (vượt hạn chót của client);
//	yêu cầu #2: trả lời {status: 503, retry_after_ms: 300} – quá tải tạm thời;
//	từ yêu cầu #3: trả lời bình thường. Trường delay_ms (nếu có) là thời gian xử lý giả lập.

var flakyDict = map[string]string{"TCP": "Giao thức truyền tin cậy, hướng kết nối", "UDP": "Giao thức datagram, không kết nối"}

var (
	countMu sync.Mutex
	count   = 0 // số yêu cầu đã nhận
)

func lookupTerm(term string) map[string]any {
	if text, ok := flakyDict[term]; ok {
		return map[string]any{"status": 200, "term": term, "text": text}
	}
	return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
}

func answer(req map[string]any) map[string]any {
	countMu.Lock()
	count++
	n := count
	countMu.Unlock()
	term, _ := GetString(req, "term")
	switch n {
	case 1:
		fmt.Printf("[server] Yêu cầu #%d: cố ý trả lời chậm 2000 ms\n", n)
		time.Sleep(2000 * time.Millisecond)
		return lookupTerm(term)
	case 2:
		fmt.Printf("[server] Yêu cầu #%d: trả lời 503, đề nghị chờ 300 ms\n", n)
		return map[string]any{"status": 503, "error": "Quá tải tạm thời", "retry_after_ms": 300}
	}
	delay := GetInt(req, "delay_ms")
	fmt.Printf("[server] Yêu cầu #%d: xử lý %d ms rồi trả lời\n", n, delay)
	time.Sleep(time.Duration(delay) * time.Millisecond)
	return lookupTerm(term)
}

func flakySession(conn net.Conn) {
	defer conn.Close()
	ctx := context.Background()
	for {
		req, err := RecvJSONAsync(ctx, conn)
		var cc *ConnectionClosed
		if errors.As(err, &cc) {
			if cc.Received > 0 {
				fmt.Printf("[server] Kết nối đóng giữa khung: %v\n", err)
			}
			return
		}
		if err != nil {
			return
		}
		if err := SendJSONAsync(ctx, conn, answer(req)); err != nil {
			fmt.Println("[server] Không gửi được phản hồi, client đã đóng kết nối")
			return
		}
	}
}

// RunFlakyServer – server [host] [port].
func RunFlakyServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	StopOnCtrlC(srv)
	for {
		conn, err := srv.Accept()
		if err != nil {
			break
		}
		go flakySession(conn)
	}
	fmt.Println("[server] Dừng server")
	return 0
}
