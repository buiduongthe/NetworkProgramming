package main

import (
	"bytes"
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"
	"time"
)

// Minh họa READ_LINE (Mã giả 3.5, Mục 3.4.3): định khung bằng ký tự xuống dòng.
// Chương trình tự mở một kết nối loopback: goroutine "gửi" chia dữ liệu thành các mảnh cắt ngang
// dòng (kể cả cắt giữa hai byte của chữ "à"), luồng chính đọc bằng READ_LINE và in từng dòng
// cùng số byte còn dư trong pending. Mảnh cuối là 100 byte không có '\n' để thấy lỗi "Dòng quá dài".

const maxLen = 64

var pieces = [][]byte{
	[]byte("HELLO\nWOR"),
	[]byte("LD\r\nXin ch\xc3"),
	[]byte("\xa0o\n"),
	bytes.Repeat([]byte("A"), 100),
}

// show hiển thị dãy byte: ký tự ASCII in được giữ nguyên, còn lại dạng \xNN.
func show(data []byte) string {
	var out strings.Builder
	for _, b := range data {
		switch {
		case b == 0x0A:
			out.WriteString(`\n`)
		case b == 0x0D:
			out.WriteString(`\r`)
		case b >= 0x20 && b < 0x7F:
			out.WriteByte(b)
		default:
			fmt.Fprintf(&out, `\x%02x`, b)
		}
	}
	text := out.String()
	if len(text) <= 24 {
		return text
	}
	return text[:21] + "..."
}

// sender gửi lần lượt từng mảnh, nghỉ 300 ms giữa các mảnh để mỗi mảnh đến bên nhận riêng rẽ.
func sender(addr string, done chan<- struct{}) {
	defer close(done)
	conn, err := net.DialTimeout("tcp4", addr, 3*time.Second)
	if err != nil {
		fmt.Printf("[gửi] Lỗi kết nối: %v\n", err)
		return
	}
	defer conn.Close()
	conn.(*net.TCPConn).SetNoDelay(true) // NODELAY: mỗi mảnh đi riêng (Go đã bật sẵn)
	for _, p := range pieces {
		fmt.Printf("[gửi] %d byte: %s\n", len(p), show(p))
		if err := SendAll(conn, p); err != nil {
			return // bên đọc đã đóng kết nối
		}
		time.Sleep(300 * time.Millisecond)
	}
}

// RunLineDemo – chương trình minh họa READ_LINE.
func RunLineDemo(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9000)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	addr := net.JoinHostPort(host, strconv.Itoa(port))
	srv, err := net.Listen("tcp4", addr)
	if err != nil {
		fmt.Printf("[đọc] Không mở được cổng: %v\n", err)
		return 1
	}
	defer srv.Close()
	fmt.Printf("[đọc] Đang lắng nghe tại %s:%d, max_len = %d\n", host, port, maxLen)
	done := make(chan struct{})
	go sender(addr, done)
	conn, err := srv.Accept()
	if err != nil {
		return 1
	}
	r := &Reader{Sock: conn} // Reader { sock, pending ← EMPTY_BYTES }
	for {
		line, ok, err := ReadLine(r, maxLen)
		var pe *ProtocolError
		if errors.As(err, &pe) {
			fmt.Printf("[đọc] ProtocolError: %v (pending %d byte > %d)\n", pe, len(r.Pending), maxLen)
			break
		}
		if err != nil {
			fmt.Printf("[đọc] Lỗi: %v\n", err)
			break
		}
		if !ok {
			fmt.Println("[đọc] Đối tác đã đóng kết nối")
			break
		}
		fmt.Printf("[đọc] READ_LINE -> '%s' (pending còn %d byte)\n", line, len(r.Pending))
	}
	conn.Close()
	<-done // chờ goroutine gửi kết thúc (t.join())
	return 0
}
