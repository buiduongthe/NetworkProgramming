package main

import (
	"errors"
	"fmt"
	"strconv"
	"time"
)

// RunClient – client minh họa Mã giả 2.2: gọi SAFE_CONNECT và in kết quả hoặc loại lỗi.
func RunClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9000)
	timeoutMs := 3000
	if err == nil && len(args) >= 3 {
		timeoutMs, err = strconv.Atoi(args[2])
		if err != nil || timeoutMs <= 0 {
			err = fmt.Errorf("Thời gian chờ không hợp lệ: %s", args[2])
		}
	}
	if err != nil {
		fmt.Printf("[client] Lỗi cấu hình: %v\n", err)
		return 2
	}
	fmt.Printf("[client] Kết nối tới %s:%d, thời gian chờ %d ms\n", host, port, timeoutMs)
	t0 := time.Now()
	conn, err := SafeConnect(host, port, timeoutMs)
	ms := time.Since(t0).Milliseconds()
	if err != nil {
		var ne *NetError
		if !errors.As(err, &ne) {
			fmt.Printf("[client] Lỗi khác: %v\n", err)
			return 1
		}
		retry := "không"
		if ne.Retry {
			retry = "có"
		}
		fmt.Printf("[client] NetError %s, thử lại: %s, sau %d ms\n", ne.Kind, retry, ms)
		fmt.Printf("[client] Nguyên nhân: %v\n", ne.Cause)
		return 1
	}
	fmt.Printf("[client] Thành công: %s -> %s, sau %d ms\n", conn.LocalAddr(), conn.RemoteAddr(), ms)
	conn.Close() // nơi gọi chịu trách nhiệm đóng socket
	fmt.Println("[client] Đã đóng kết nối")
	return 0
}
