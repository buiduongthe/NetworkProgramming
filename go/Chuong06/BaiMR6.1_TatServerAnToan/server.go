package main

import (
	"bufio"
	"context"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
)

// Hai ngữ cảnh (context) điều khiển việc tắt:
//   - stopCtx bị hủy khi nhận tín hiệu: ngừng nhận việc mới, kết nối rỗi được chào BYE;
//   - forceCtx bị hủy khi quá hạn chờ: việc đang làm dở bị hủy, client nhận HUY.
var (
	stopCtx, stopNow   = context.WithCancel(context.Background())
	forceCtx, forceNow = context.WithCancel(context.Background())
	done, cancelled    atomic.Int64
	active             sync.Map // mã kết nối -> net.Conn (để đếm kết nối còn mở)
	wg                 sync.WaitGroup
	connID             atomic.Int64
)

func slog(format string, a ...any) { fmt.Printf("[server] "+format+"\n", a...) }

func countActive() int {
	n := 0
	active.Range(func(_, _ any) bool { n++; return true })
	return n
}

// handle – phục vụ một kết nối: đọc dòng "LAM <ms>", làm việc <ms> mili giây, trả "XONG <ms>".
func handle(c net.Conn) {
	defer wg.Done()
	id := connID.Add(1)
	active.Store(id, c)
	defer func() { c.Close(); active.Delete(id) }()
	slog("#%d kết nối từ cổng %d", id, c.RemoteAddr().(*net.TCPAddr).Port)

	lines := make(chan string) // goroutine phụ đọc dòng; đóng kênh khi client đóng hoặc lỗi
	go func() {
		defer close(lines)
		r := bufio.NewReader(c)
		for {
			l, err := r.ReadString('\n')
			if err != nil {
				return
			}
			lines <- strings.TrimSpace(l)
		}
	}()
	w := bufio.NewWriter(c)
	send := func(s string) { w.WriteString(s + "\n"); w.Flush() }
	for {
		var line string
		var ok bool
		select { // chờ dòng lệnh kế tiếp HOẶC tín hiệu dừng
		case line, ok = <-lines:
			if !ok {
				slog("#%d client đóng kết nối", id)
				return
			}
		case <-stopCtx.Done(): // rỗi đúng lúc server tắt
			send("BYE server đang tắt")
			slog("#%d đang rỗi -> gửi BYE, đóng", id)
			return
		}
		f := strings.Fields(line)
		ms, err := 0, error(nil)
		if len(f) == 2 && f[0] == "LAM" {
			ms, err = strconv.Atoi(f[1])
		}
		if len(f) != 2 || f[0] != "LAM" || err != nil || ms < 0 {
			send("LOI lệnh không hợp lệ")
			continue
		}
		slog("#%d bắt đầu LAM %d", id, ms)
		select { // công việc dài, có thể bị hủy khi quá hạn chờ tắt
		case <-time.After(time.Duration(ms) * time.Millisecond):
		case <-forceCtx.Done():
			cancelled.Add(1)
			slog("#%d quá hạn chờ -> hủy LAM %d", id, ms)
			send("HUY yêu cầu bị hủy vì server tắt")
			return
		}
		send(fmt.Sprintf("XONG %d", ms))
		done.Add(1)
		if stopCtx.Err() != nil { // làm xong việc dở thì mới chào
			send("BYE server đang tắt")
			slog("#%d XONG %d -> gửi BYE, đóng", id, ms)
			return
		}
		slog("#%d XONG %d", id, ms)
	}
}

func runServer(host, port string, drain time.Duration) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		slog("Lỗi: %v", err)
		os.Exit(1)
	}
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, syscall.SIGTERM, os.Interrupt)
	os.WriteFile("server.pid", []byte(strconv.Itoa(os.Getpid())), 0o644)
	slog("Lắng nghe tại %s:%s, PID ghi vào server.pid", host, port)

	go func() { // vòng chấp nhận kết nối; kết thúc khi listener bị đóng
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			wg.Add(1)
			go handle(c)
		}
	}()

	sig := <-sigs
	t0 := time.Now()
	ln.Close() // 1. ngừng nhận kết nối mới
	name := "SIGINT"
	if sig == syscall.SIGTERM {
		name = "SIGTERM"
	}
	slog("Nhận %s: ngừng nhận kết nối mới", name)
	slog("Chờ tối đa %d ms cho %d kết nối", drain.Milliseconds(), countActive())
	stopNow() // 2, 3. kết nối rỗi chào BYE, kết nối bận làm nốt việc

	finished := make(chan struct{})
	go func() { wg.Wait(); close(finished) }()
	select {
	case <-finished:
	case <-time.After(drain): // 4. quá hạn: hủy phần còn lại
		slog("Hết hạn chờ: hủy %d kết nối", countActive())
		forceNow()
		<-finished
	}
	os.Remove("server.pid")
	slog("Đã tắt sau %.1f s: xong %d yêu cầu, hủy %d", time.Since(t0).Seconds(), done.Load(), cancelled.Load())
}
