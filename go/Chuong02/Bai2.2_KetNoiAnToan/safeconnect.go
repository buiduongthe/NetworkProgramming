package main

import (
	"errors"
	"net"
	"strconv"
	"time"
)

// NetError – lỗi mạng đã phân loại: Kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}.
type NetError struct {
	Kind  string // loại lỗi
	Retry bool   // có nên thử lại không
	Cause error  // lỗi gốc
}

func (e *NetError) Error() string { return e.Kind }

// SafeConnect – Mã giả 2.2 – SAFE_CONNECT(host, port, timeout_ms).
// Thành công: trả kết nối (nơi gọi chịu trách nhiệm đóng); thất bại: trả *NetError.
func SafeConnect(host string, port int, timeoutMs int) (net.Conn, error) {
	addrs, err := net.LookupIP(host) // dòng 2–6: RESOLVE(host)
	if err != nil {                  // CATCH ResolveError: tên miền không tồn tại
		return nil, &NetError{Kind: "NOT_FOUND", Retry: false, Cause: err}
	}
	refused := 0              // dòng 7
	var lastError error       // last_error ← NULL
	for _, a := range addrs { // dòng 8: FOR EACH a IN addrs
		network := "tcp4" // dòng 9: SOCKET(FAMILY(a), STREAM)
		if a.To4() == nil {
			network = "tcp6"
		}
		d := net.Dialer{Timeout: time.Duration(timeoutMs) * time.Millisecond}          // dòng 10: CONNECT_TIMEOUT
		conn, err := d.Dial(network, net.JoinHostPort(a.String(), strconv.Itoa(port))) // dòng 11–13: CONNECT
		if err == nil {
			return conn, nil // thành công: trả socket (ok ← TRUE, không đóng)
		}
		// dòng 18–20: Dial thất bại thì Go đã tự đóng socket (IF NOT ok THEN CLOSE(s))
		var ne net.Error
		switch {
		case isRefused(err): // dòng 14–15: nhận gói RST
			refused++
			lastError = err
		case errors.As(err, &ne) && ne.Timeout(): // dòng 16–17: Timeout
			lastError = err
		case isUnreachable(err):
			lastError = err // NetworkUnreachable, HostUnreachable
		default:
			return nil, err // lỗi khác: chuyển lên nơi gọi
		}
	}
	if refused > 0 && refused == len(addrs) { // dòng 22–24
		return nil, &NetError{Kind: "REFUSED", Retry: false, Cause: lastError}
	}
	return nil, &NetError{Kind: "UNREACHABLE", Retry: true, Cause: lastError} // dòng 25
}
