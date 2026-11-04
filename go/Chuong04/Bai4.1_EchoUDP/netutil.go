// Thủ tục kế thừa từ chương trước, chép gọn vào bài để thư mục bài tự chứa:
//   - Mã giả 2.1 PARSE_ENDPOINT (Bài 2.1)

package main

import (
	"io"
	"os"
	"os/signal"
	"strconv"
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

// StopOnCtrlC đóng socket khi người dùng nhấn Ctrl+C, để vòng lặp nhận kết thúc.
func StopOnCtrlC(s io.Closer) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		s.Close()
	}()
}
