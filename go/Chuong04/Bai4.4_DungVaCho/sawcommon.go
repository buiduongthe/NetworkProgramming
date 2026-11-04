// Hằng số và tiện ích dùng chung cho bên gửi, bên nhận dừng và chờ (Mã giả 4.6, 4.7).

package main

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"math/rand"
	"os"
	"strconv"
	"strings"
)

// Byte loại trong tiêu đề 5 byte (Mã giả 4.6, dòng 1).
const (
	DATA = 0
	ACK  = 1
	FIN  = 2
)

const (
	MaxTries = 8
	MaxDgram = 1200
	Chunk    = MaxDgram - 5 // khối dữ liệu ≤ 1195 byte
	// Thời gian (ms) cho mạng nội bộ (Mục 4.4.2), không phải 1 s của TCP; MAX_RTO của mã giả là 60000.
	RtoInit = 200
	RtoMin  = 100
	RtoMax  = 2000
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

// LossSim – mô phỏng mất gói ở chiều gửi đi (thêm cho bài thực hành, không có trong mã giả).
//
//	spec = ""      : không mất gói;
//	spec = "0.1"   : mỗi datagram gửi đi mất ngẫu nhiên với xác suất 0,1;
//	spec = "3,7"   : mất đúng datagram gửi đi thứ 3 và thứ 7 (đếm từ 1, kể cả lần gửi lại).
type LossSim struct {
	Spec  string
	count int
	rate  float64
	picks map[int]bool
}

// NewLossSim tạo bộ mô phỏng từ chuỗi đặc tả.
func NewLossSim(spec string) *LossSim {
	l := &LossSim{Spec: spec, picks: map[int]bool{}}
	if strings.Contains(spec, ".") {
		l.rate, _ = strconv.ParseFloat(spec, 64)
	} else {
		for _, x := range strings.Split(spec, ",") {
			if n, err := strconv.Atoi(x); err == nil {
				l.picks[n] = true
			}
		}
	}
	return l
}

// Drop cho biết datagram gửi đi kế tiếp có bị "mất" không.
func (l *LossSim) Drop() bool {
	l.count++
	return l.picks[l.count] || rand.Float64() < l.rate
}

// SpecText – đặc tả mất gói để in ra, "không" nếu không mô phỏng.
func (l *LossSim) SpecText() string {
	if l.Spec == "" {
		return "không"
	}
	return l.Spec
}

func sha256File(path string) string {
	f, err := os.Open(path)
	if err != nil {
		return "?"
	}
	defer f.Close()
	h := sha256.New()
	io.Copy(h, f)
	return hex.EncodeToString(h.Sum(nil))
}
