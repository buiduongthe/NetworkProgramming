// Các thủ tục kế thừa từ chương trước, chép gọn vào bài để thư mục bài tự chứa:
//   - Mã giả 2.1 PARSE_ENDPOINT (Bài 2.1)
//   - Mã giả 2.2 SAFE_CONNECT (Bài 2.2)

package main

import (
	"errors"
	"net"
	"os"
	"os/signal"
	"strconv"
	"time"
)

// ConfigError – lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được.
type ConfigError struct{ Msg string }

func (e *ConfigError) Error() string { return e.Msg }

// ParseEndpoint – Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
func ParseEndpoint(args []string, defaultHost string, defaultPort int) (string, int, error) {
	host, portText := defaultHost, strconv.Itoa(defaultPort)
	if v, ok := os.LookupEnv("APP_HOST"); ok {
		host = v
	}
	if v, ok := os.LookupEnv("APP_PORT"); ok {
		portText = v
	}
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		portText = args[1]
	}
	port, err := strconv.Atoi(portText)
	if err != nil {
		return "", 0, &ConfigError{"Cổng phải là số nguyên: " + portText}
	}
	if port < 1 || port > 65535 {
		return "", 0, &ConfigError{"Cổng không hợp lệ: " + portText}
	}
	return host, port, nil
}

// NetError – lỗi mạng đã phân loại: Kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}.
type NetError struct {
	Kind  string
	Retry bool
	Cause error
}

func (e *NetError) Error() string { return e.Kind + ": " + e.Cause.Error() }

// SafeConnect – Mã giả 2.2: thử lần lượt các địa chỉ, có thời gian chờ kết nối, phân loại lỗi.
func SafeConnect(host string, port int, timeoutMs int) (net.Conn, error) {
	addrs, err := net.LookupIP(host) // RESOLVE
	if err != nil {
		return nil, &NetError{"NOT_FOUND", false, err}
	}
	refused := 0
	var lastError error
	for _, a := range addrs {
		network := "tcp4" // SOCKET(FAMILY(a), STREAM)
		if a.To4() == nil {
			network = "tcp6"
		}
		d := net.Dialer{Timeout: time.Duration(timeoutMs) * time.Millisecond} // CONNECT_TIMEOUT
		conn, err := d.Dial(network, net.JoinHostPort(a.String(), strconv.Itoa(port)))
		if err == nil {
			return conn, nil
		}
		lastError = err // Dial thất bại đã tự đóng socket
		var ne net.Error
		switch {
		case isRefused(err):
			refused++
		case errors.As(err, &ne) && ne.Timeout():
		case isUnreachable(err):
		default:
			return nil, err
		}
	}
	if refused > 0 && refused == len(addrs) {
		return nil, &NetError{"REFUSED", false, lastError}
	}
	return nil, &NetError{"UNREACHABLE", true, lastError}
}

// StopOnCtrlC đóng socket lắng nghe khi người dùng nhấn Ctrl+C, để vòng ACCEPT kết thúc.
func StopOnCtrlC(srv net.Listener) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		srv.Close()
	}()
}
