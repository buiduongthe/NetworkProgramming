package main

import (
	"errors"
	"fmt"
	"net"
	"strconv"
	"syscall"
	"time"
)

// PortCheck – Mã giả 1.2 – PORT_CHECK(host, port, timeout_ms).
// Trả về OPEN, CLOSED, FILTERED hoặc UNRESOLVED; error khác nil chỉ khi gặp lỗi mà mã giả không bắt.
func PortCheck(host string, port int, timeoutMs int) (string, error) {
	addrs := ResolveHost(host) // dòng 2: lỗi phân giải → rỗng
	if len(addrs) == 0 {       // dòng 3
		return "UNRESOLVED", nil
	}
	state := "FILTERED"       // dòng 4: chưa địa chỉ nào phản hồi
	for _, a := range addrs { // dòng 5: FOR EACH a IN addrs
		network := "tcp4" // dòng 6: SOCKET(FAMILY(a), STREAM) – họ socket theo họ địa chỉ
		if Family(a) == "INET6" {
			network = "tcp6"
		}
		d := net.Dialer{Timeout: time.Duration(timeoutMs) * time.Millisecond}          // dòng 7: SETOPT(CONNECT_TIMEOUT)
		conn, err := d.Dial(network, net.JoinHostPort(a.String(), strconv.Itoa(port))) // dòng 9: bắt tay ba bước
		if err == nil {
			fmt.Printf("[port-check] %s cổng %d: kết nối thành công\n", a, port)
			conn.Close()       // dòng 16: FINALLY CLOSE(s)
			return "OPEN", nil // dòng 10
		}
		// Khi Dial thất bại, Go đã tự đóng socket (tương ứng FINALLY CLOSE(s))
		var ne net.Error
		var errno syscall.Errno
		errors.As(err, &errno) // mã lỗi của hệ điều hành (nếu có)
		switch {
		case isRefused(err): // dòng 11: CATCH ConnectionRefused
			fmt.Printf("[port-check] %s cổng %d: bị từ chối (RST)\n", a, port)
			state = "CLOSED" // dòng 12
		case errors.As(err, &ne) && ne.Timeout(): // dòng 13: CATCH Timeout
			fmt.Printf("[port-check] %s cổng %d: hết thời gian chờ\n", a, port)
			continue // dòng 14
		case isUnreachable(err): // dòng 13
			fmt.Printf("[port-check] %s cổng %d: không có đường đi (%v)\n", a, port, errno)
			continue
		default:
			return "", err // lỗi khác: để thoát ra như mã giả
		}
	}
	return state, nil // dòng 19: CLOSED hoặc FILTERED
}
