package main

import (
	"os"
	"strconv"
)

// ConfigError – lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được.
type ConfigError struct {
	Msg string
}

func (e *ConfigError) Error() string { return e.Msg }

// ParseEndpoint – Mã giả 2.1 – PARSE_ENDPOINT(args, default_host, default_port).
// Thứ tự ưu tiên: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
func ParseEndpoint(args []string, defaultHost string, defaultPort int) (string, int, error) {
	host := defaultHost                        // dòng 2
	portText := strconv.Itoa(defaultPort)      // dòng 3
	if v, ok := os.LookupEnv("APP_HOST"); ok { // dòng 4: biến môi trường ghi đè mặc định
		host = v
	}
	if v, ok := os.LookupEnv("APP_PORT"); ok { // dòng 5
		portText = v
	}
	if len(args) >= 1 { // dòng 6: tham số dòng lệnh ghi đè tất cả
		host = args[0]
	}
	if len(args) >= 2 { // dòng 7
		portText = args[1]
	}
	port, err := strconv.Atoi(portText) // dòng 8–12: TO_INT; "abc" → FormatError
	if err != nil {
		return "", 0, &ConfigError{"Cổng phải là số nguyên: " + portText}
	}
	if port < 1 || port > 65535 { // dòng 13–15: cổng là số 16 bit, khác 0
		return "", 0, &ConfigError{"Cổng không hợp lệ: " + portText}
	}
	return host, port, nil // dòng 16
}
