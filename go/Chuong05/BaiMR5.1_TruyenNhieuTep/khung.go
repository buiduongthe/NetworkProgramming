// Bài mở rộng MR5.1 – Khung giao thức dùng chung: định khung JSON, thùng token, băm SHA-256.
//
// Giao thức (một kết nối TCP cho mỗi lượt truyền): một khung điều khiển JSON có tiền tố độ dài
// 4 byte (big-endian), rồi tới luồng byte nhị phân của tệp (từ offset tới hết) để hỗ trợ tải tiếp.
package main

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	chunk        = 32 * 1024
	khongGioiHan = float64(1 << 40) // "không giới hạn" (1 TiB/s)
)

// Khung – một khung điều khiển JSON (dùng chung cho mọi loại thông điệp).
type Khung struct {
	Op       string `json:"op,omitempty"`
	Name     string `json:"name,omitempty"`
	Offset   int64  `json:"offset,omitempty"`
	Size     int64  `json:"size,omitempty"`
	Sha256   string `json:"sha256,omitempty"`
	OK       bool   `json:"ok"`
	Loi      string `json:"loi,omitempty"`
	Sha256OK bool   `json:"sha256_ok,omitempty"`
}

func guiKhung(conn net.Conn, k Khung) error {
	b, _ := json.Marshal(k)
	hdr := make([]byte, 4)
	binary.BigEndian.PutUint32(hdr, uint32(len(b)))
	_, err := conn.Write(append(hdr, b...))
	return err
}

func nhanKhung(conn net.Conn) (Khung, error) {
	var hdr [4]byte
	if _, err := io.ReadFull(conn, hdr[:]); err != nil {
		return Khung{}, err
	}
	n := binary.BigEndian.Uint32(hdr[:])
	buf := make([]byte, n)
	if _, err := io.ReadFull(conn, buf); err != nil {
		return Khung{}, err
	}
	var k Khung
	err := json.Unmarshal(buf, &k)
	return k, err
}

func sha256Tep(path string) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	h := sha256.New()
	io.Copy(h, f)
	return fmt.Sprintf("%x", h.Sum(nil))
}

// ThungToken – thùng token giới hạn tốc độ (byte/giây), đổ đầy liên tục, đổi được lúc chạy.
type ThungToken struct {
	mu      sync.Mutex
	rate    float64
	sucChua float64
	token   float64
	moc     time.Time
}

func newThung(rate float64) *ThungToken {
	return &ThungToken{rate: rate, sucChua: rate, token: min(rate, chunk*4), moc: time.Now()}
}

func (t *ThungToken) doDay() {
	now := time.Now()
	t.token = min(t.sucChua, t.token+now.Sub(t.moc).Seconds()*t.rate)
	t.moc = now
}

func (t *ThungToken) datRate(rate float64) {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.doDay()
	t.rate = rate
	t.sucChua = rate
	// Giới hạn "bùng" còn 0,25 giây để nhiều tệp chia sẻ đều hơn, tốc độ đo sát giới hạn phiên.
	t.token = min(t.token, rate*0.25)
}

// tieuThu: chặn tới khi lấy được n token; dungKhi() trả true thì bỏ dở (pause/stop).
func (t *ThungToken) tieuThu(n float64, dungKhi func() bool) bool {
	for !dungKhi() {
		t.mu.Lock()
		t.doDay()
		if t.token >= n {
			t.token -= n
			t.mu.Unlock()
			return true
		}
		thieu := (n - t.token) / t.rate
		t.mu.Unlock()
		if thieu > 0.05 {
			thieu = 0.05
		}
		time.Sleep(time.Duration(thieu * float64(time.Second)))
	}
	return false
}

func docSoByte(s string) float64 {
	s = strings.ToUpper(strings.TrimSpace(s))
	s = strings.ReplaceAll(s, "IB", "")
	s = strings.ReplaceAll(s, "B", "")
	nhan := 1.0
	if strings.HasSuffix(s, "K") {
		nhan, s = 1024, s[:len(s)-1]
	} else if strings.HasSuffix(s, "M") {
		nhan, s = 1024*1024, s[:len(s)-1]
	} else if strings.HasSuffix(s, "G") {
		nhan, s = 1024*1024*1024, s[:len(s)-1]
	}
	v, _ := strconv.ParseFloat(s, 64)
	return v * nhan
}

func dinhDangByte(n int64) string {
	switch {
	case n >= 1024*1024:
		return fmt.Sprintf("%.2f MiB", float64(n)/(1024*1024))
	case n >= 1024:
		return fmt.Sprintf("%.2f KiB", float64(n)/1024)
	default:
		return fmt.Sprintf("%d B", n)
	}
}

func thanhTienDo(pct float64) string {
	rong := 20
	day := int(pct / 100 * float64(rong))
	if day > rong {
		day = rong
	}
	if day < 0 {
		day = 0
	}
	return "[" + strings.Repeat("#", day) + strings.Repeat("-", rong-day) + "]"
}
