// Bài 7.3 – Kiểm thử server WebSocket bằng các khung tự tạo (kể cả khung vi phạm đặc tả).
//
// Sáu ca kiểm thử, mỗi ca một kết nối:
//
//  1. Yêu cầu HTTP thường (không Upgrade)       -> mong đợi 400 (Mã giả 7.3, dòng 3–7)
//  2. Ping                                       -> mong đợi pong cùng dữ liệu
//  3. Văn bản 300 byte (độ dài mở rộng 16 bit)   -> mong đợi echo (Mã giả 7.4, dòng 5)
//  4. Khung văn bản KHÔNG che                    -> mong đợi khung đóng 1002 (dòng 11–14)
//  5. Khung khai báo độ dài 2^40 byte            -> mong đợi khung đóng 1009 (dòng 7–10)
//  6. Sec-WebSocket-Key không phải 16 byte       -> mong đợi 400 (Mã giả 7.3, kiểm tra khóa)
//
// Chạy:  out/app kiemthu [host] [port]      (mặc định 127.0.0.1 8080)
package main

import (
	"bytes"
	"encoding/binary"
	"fmt"
	"net"
	"strconv"
	"strings"
	"time"
)

func mustConnect(host string, port int) net.Conn {
	s, _, err := wsConnect(host, port, "/")
	if err != nil {
		fail("[kiểm thử] " + err.Error())
	}
	return s
}

func kiemThuMain(args []string) {
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	bye := binary.BigEndian.AppendUint16(nil, 1000)

	// ca 1
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second)
	if err != nil {
		fail("[kiểm thử] " + err.Error())
	}
	s.SetDeadline(time.Now().Add(5 * time.Second))
	s.Write([]byte("GET / HTTP/1.1\r\nHost: " + host + ":" + strconv.Itoa(port) + "\r\n\r\n"))
	buf := make([]byte, 4096)
	n, _ := s.Read(buf)
	status, _, _ := strings.Cut(string(buf[:n]), "\r\n")
	fmt.Printf("[kiểm thử] 1. Yêu cầu HTTP thường (không Upgrade) -> %s\n", status)
	s.Close()

	// ca 2
	s = mustConnect(host, port)
	wsSendClient(s, 0x9, []byte("nhip-tim"), true)
	_, opcode, data, _ := wsReadFrame(s, s, false)
	fmt.Printf("[kiểm thử] 2. Ping 'nhip-tim' -> opcode 0x%X, dữ liệu '%s'\n", opcode, data)
	wsSendClient(s, 0x8, bye, true)
	wsReadFrame(s, s, false)
	s.Close()

	// ca 3
	s = mustConnect(host, port)
	wsSendClient(s, 0x1, bytes.Repeat([]byte("A"), 300), true)
	_, opcode, data, _ = wsReadFrame(s, s, false)
	fmt.Printf("[kiểm thử] 3. Văn bản 300 byte -> nhận lại %d byte (opcode 0x%X)\n", len(data), opcode)
	wsSendClient(s, 0x8, bye, true)
	wsReadFrame(s, s, false)
	s.Close()

	// ca 4
	s = mustConnect(host, port)
	wsSendClient(s, 0x1, []byte("khong che"), false)
	_, _, data, _ = wsReadFrame(s, s, false)
	fmt.Printf("[kiểm thử] 4. Khung không che -> khung đóng, mã %d\n", closeCode(data))
	s.Close()

	// ca 5
	s = mustConnect(host, port)
	head := binary.BigEndian.AppendUint64([]byte{0x81, 0x80 | 127}, 1<<40) // chỉ gửi tiêu đề khung
	s.Write(head)
	_, _, data, _ = wsReadFrame(s, s, false)
	fmt.Printf("[kiểm thử] 5. Khung khai báo 2^40 byte -> khung đóng, mã %d\n", closeCode(data))
	s.Close()

	// ca 6
	s, err = net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second)
	if err != nil {
		fail("[kiểm thử] " + err.Error())
	}
	s.SetDeadline(time.Now().Add(5 * time.Second))
	s.Write([]byte("GET / HTTP/1.1\r\nHost: " + host + ":" + strconv.Itoa(port) + "\r\nUpgrade: websocket\r\n" +
		"Connection: Upgrade\r\nSec-WebSocket-Key: a2hvbmctaG9wLWxl\r\nSec-WebSocket-Version: 13\r\n\r\n")) // 12 byte
	n, _ = s.Read(buf)
	status, _, _ = strings.Cut(string(buf[:n]), "\r\n")
	fmt.Printf("[kiểm thử] 6. Sec-WebSocket-Key giải mã ra 12 byte (cần 16) -> %s\n", status)
	s.Close()
}
