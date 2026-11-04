package main

// Các tác vụ mạng chạy trên goroutine và bộ giới hạn đồng thời đổi được khi đang chạy.
// Mỗi tác vụ nhận một context: khi người dùng bấm Hủy, context bị hủy và mọi lời gọi mạng đang
// chờ (DialContext, request HTTP có context) trả về ngay – hủy kiểu hợp tác như Bài 5.5, 5.6.

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

type progressFn func(pct int, text string)

// limiter – giới hạn số tác vụ chạy cùng lúc, cấp lượt theo đúng thứ tự xếp hàng (FIFO).
// enter() được gọi ngay khi tạo tác vụ (theo thứ tự bấm nút) và trả về một kênh "vé"; kênh bị đóng
// khi tác vụ được vào chạy. Giới hạn đổi được khi đang chạy (thanh trượt).
type limiter struct {
	mu             sync.Mutex
	limit, running int
	queue          []chan struct{}
}

func newLimiter(n int) *limiter { return &limiter{limit: n} }

// grant – cấp lượt cho các vé đầu hàng đợi khi còn chỗ (gọi khi đang giữ mu).
func (l *limiter) grant() {
	for l.running < l.limit && len(l.queue) > 0 {
		l.running++
		close(l.queue[0])
		l.queue = l.queue[1:]
	}
}

func (l *limiter) get() int {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.limit
}

func (l *limiter) set(n int) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.limit = n
	l.grant()
}

func (l *limiter) enter() chan struct{} {
	l.mu.Lock()
	defer l.mu.Unlock()
	ticket := make(chan struct{})
	l.queue = append(l.queue, ticket)
	l.grant()
	return ticket
}

// wait – chờ tới lượt; nếu bị hủy khi đang chờ thì rời hàng đợi và trả về false.
func (l *limiter) wait(ctx context.Context, ticket chan struct{}) bool {
	select {
	case <-ticket:
		return true
	case <-ctx.Done():
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	for i, t := range l.queue {
		if t == ticket { // vẫn đang xếp hàng: rời hàng
			l.queue = append(l.queue[:i], l.queue[i+1:]...)
			return false
		}
	}
	l.running-- // vừa được cấp lượt đúng lúc bị hủy: trả lại lượt
	l.grant()
	return false
}

func (l *limiter) release() {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.running--
	l.grant()
}

// download – tải /tep?kb=..&tre=.. qua HTTP, đọc từng phần, báo tiến trình theo Content-Length.
func download(ctx context.Context, addr string, kb, delay int, progress progressFn) (string, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", fmt.Sprintf("http://%s/tep?kb=%d&tre=%d", addr, kb, delay), nil)
	if err != nil {
		return "", err
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	total, got := resp.ContentLength, int64(0)
	buf := make([]byte, 16*1024)
	for got < total {
		n, err := resp.Body.Read(buf)
		got += int64(n)
		if n > 0 {
			progress(int(got*100/total), fmt.Sprintf("%d/%d KB", got/1024, total/1024))
		}
		if err == io.EOF && got < total {
			return "", errors.New("máy chủ đóng kết nối sớm")
		}
		if err != nil && err != io.EOF {
			return "", err
		}
	}
	return fmt.Sprintf("xong %d KB", total/1024), nil
}

// checkPorts – kiểm tra lần lượt từng cổng TCP (như Bài 1.2): kết nối được = mở.
func checkPorts(ctx context.Context, host string, ports []int, progress progressFn) (string, error) {
	opened := 0
	for i, p := range ports {
		d := net.Dialer{Timeout: 300 * time.Millisecond}
		c, err := d.DialContext(ctx, "tcp", net.JoinHostPort(host, fmt.Sprint(p)))
		if ctx.Err() != nil {
			return "", ctx.Err()
		}
		state := "đóng"
		if err == nil {
			c.Close()
			opened++
			state = "mở"
		}
		select { // quét chậm cho dễ quan sát
		case <-time.After(300 * time.Millisecond):
		case <-ctx.Done():
			return "", ctx.Err()
		}
		progress((i+1)*100/len(ports), fmt.Sprintf("cổng %d %s", p, state))
	}
	return fmt.Sprintf("%d mở, %d đóng", opened, len(ports)-opened), nil
}

// lookupNames – tra cứu tên qua máy chủ "DNS" giả lập trên UDP, có thời gian chờ.
func lookupNames(ctx context.Context, addr string, names []string, progress progressFn) (string, error) {
	var d net.Dialer
	c, err := d.DialContext(ctx, "udp", addr)
	if err != nil {
		return "", err
	}
	defer c.Close()
	stop := context.AfterFunc(ctx, func() { c.SetDeadline(time.Now()) }) // hủy: bỏ chờ ngay
	defer stop()
	found := 0
	buf := make([]byte, 512)
	for i, name := range names {
		if ctx.Err() != nil {
			return "", ctx.Err()
		}
		c.SetDeadline(time.Now().Add(time.Second))
		fmt.Fprintf(c, "A %s", name)
		answer := name + " (hết thời gian chờ)"
		if n, err := c.Read(buf); err == nil {
			answer = string(buf[:n])
		} else if ctx.Err() != nil {
			return "", ctx.Err()
		}
		if !strings.Contains(answer, "NXDOMAIN") {
			found++
		}
		progress((i+1)*100/len(names), answer)
	}
	return fmt.Sprintf("%d/%d tên có địa chỉ", found, len(names)), nil
}
