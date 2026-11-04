package main

// Máy chủ thử có độ trễ giả lập (chỉ chạy cục bộ). Giao thức dòng văn bản trên TCP: client gửi
// "GET <k>\n", máy chủ chờ <trễ> ms rồi trả "OK <k>\n"; mỗi kết nối một goroutine. Sau mỗi "đợt"
// yêu cầu (máy chủ rỗi 300 ms), in số yêu cầu và số kết nối đồng thời lớn nhất của đợt.

import (
	"bufio"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strings"
	"sync"
	"syscall"
	"time"
)

type srvState struct {
	mu                                sync.Mutex
	active, peak, count, total, burst int
}

func slog(format string, a ...any) { fmt.Printf("[server] "+format+"\n", a...) }

func (st *srvState) reportWhenIdle() {
	time.AfterFunc(300*time.Millisecond, func() {
		st.mu.Lock()
		defer st.mu.Unlock()
		if st.active == 0 && st.count > 0 {
			st.burst++
			slog("đợt %2d: %2d yêu cầu, đồng thời tối đa %2d", st.burst, st.count, st.peak)
			st.count, st.peak = 0, 0
		}
	})
}

func runServer(host, port string, delay time.Duration) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		slog("lỗi: %v", err)
		os.Exit(1)
	}
	st := &srvState{}
	slog("lắng nghe %s:%s, mỗi yêu cầu trễ %d ms", host, port, delay.Milliseconds())
	go func() {
		sig := make(chan os.Signal, 1)
		signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
		<-sig
		st.mu.Lock()
		slog("dừng; đã phục vụ %d yêu cầu", st.total)
		os.Exit(0)
	}()
	for {
		c, err := ln.Accept()
		if err != nil {
			continue
		}
		go func() {
			defer c.Close()
			st.mu.Lock()
			st.active++
			st.count++
			st.total++
			st.peak = max(st.peak, st.active)
			st.mu.Unlock()
			line, err := bufio.NewReader(c).ReadString('\n')
			if err == nil {
				time.Sleep(delay) // giả lập độ trễ
				fmt.Fprintf(c, "OK %s\n", strings.TrimPrefix(strings.TrimSpace(line), "GET "))
			}
			st.mu.Lock()
			st.active--
			if st.active == 0 {
				st.reportWhenIdle()
			}
			st.mu.Unlock()
		}()
	}
}
