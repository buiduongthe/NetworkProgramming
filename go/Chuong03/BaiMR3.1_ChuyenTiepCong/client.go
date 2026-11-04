package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strings"
	"syscall"
	"time"
)

// runClient – gửi dữ liệu, đóng chiều gửi (CloseWrite), đọc phản hồi tới EOF.
func runClient(host, port, text string, repeat int) {
	data := []byte(strings.Repeat(text, repeat))
	c, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 5*time.Second)
	if err != nil {
		fmt.Println("[client] Không kết nối được:", err)
		os.Exit(1)
	}
	conn := c.(*net.TCPConn)
	conn.SetDeadline(time.Now().Add(5 * time.Second))
	var reply []byte
	_, err = conn.Write(data)
	if err == nil {
		conn.CloseWrite() // báo hết dữ liệu: server mới bắt đầu trả lời
		reply, err = io.ReadAll(conn)
	}
	conn.Close()
	if errors.Is(err, syscall.ECONNRESET) { // bên kia đóng khi còn dữ liệu chưa đọc -> RST
		fmt.Println("[client] Kết nối bị đặt lại (RST) trước khi có phản hồi")
	} else if err != nil {
		fmt.Println("[client] Lỗi:", err)
	}
	fmt.Printf("[client] Gửi %d byte, nhận %d byte\n", len(data), len(reply))
	if len(reply) == 0 {
		fmt.Println("[client] Kết nối bị đóng mà không có dữ liệu trả về")
		os.Exit(1)
	}
	preview := []rune(string(reply))
	more := ""
	if len(preview) > 40 {
		preview, more = preview[:40], "..."
	}
	fmt.Printf("[client] Phản hồi: '%s'%s\n", string(preview), more)
	ok := "SAI"
	if bytes.Equal(reply, []byte(strings.ToUpper(string(data)))) {
		ok = "KHỚP"
	}
	fmt.Printf("[client] So khớp với bản chữ hoa: %s\n", ok)
}
