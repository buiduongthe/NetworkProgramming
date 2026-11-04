package main

import (
	"bufio"
	"errors"
	"fmt"
	"net"
	"os"
	"strings"
	"syscall"
	"time"
)

// runClient – gửi lần lượt "LAM <ms>", in phản hồi; sau đó giữ kết nối và in mọi dòng
// server gửi (BYE, HUY) cho tới khi server đóng kết nối.
func runClient(name, host, port string, jobs []string) {
	tag := "[khach " + name + "]"
	c, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 3*time.Second)
	if err != nil {
		reason := err.Error()
		if errors.Is(err, syscall.ECONNREFUSED) {
			reason = "Connection refused"
		}
		fmt.Printf("%s Không kết nối được: %s\n", tag, reason)
		os.Exit(1)
	}
	defer c.Close()
	c.SetDeadline(time.Now().Add(10 * time.Second))
	r := bufio.NewReader(c)
	t0 := time.Now()
	for _, ms := range jobs {
		fmt.Fprintf(c, "LAM %s\n", ms)
		fmt.Printf("%s Gửi LAM %s\n", tag, ms)
		line, err := r.ReadString('\n')
		if err != nil {
			fmt.Printf("%s Server đóng kết nối khi chưa trả lời\n", tag)
			return
		}
		fmt.Printf("%s t=%.1fs nhận: %s\n", tag, time.Since(t0).Seconds(), strings.TrimSpace(line))
	}
	for { // giữ kết nối, chờ thông báo của server
		line, err := r.ReadString('\n')
		if err != nil {
			break
		}
		fmt.Printf("%s t=%.1fs nhận: %s\n", tag, time.Since(t0).Seconds(), strings.TrimSpace(line))
	}
	fmt.Printf("%s t=%.1fs server đóng kết nối\n", tag, time.Since(t0).Seconds())
}
