// Bài 7.3 – Client WebSocket tự viết bằng thư viện chuẩn (Go không có client WebSocket chuẩn).
//
// Phía client của Mã giả 7.3 (gửi yêu cầu nâng cấp, kiểm tra Sec-WebSocket-Accept) và Mã giả 7.4
// (đọc khung với from_client = FALSE). Mọi khung client gửi đều được che bằng khóa ngẫu nhiên 4 byte.
// Đọc từng dòng từ bàn phím, gửi thành khung văn bản và in khung trả lời; cuối cùng gửi khung đóng 1000.
//
// Chạy:  out/app client [host] [port]      (mặc định 127.0.0.1 8080)
package main

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

// wsConnect: gửi yêu cầu nâng cấp (Sec-WebSocket-Key = Base64 của 16 byte ngẫu nhiên), kiểm tra phản hồi 101.
func wsConnect(host string, port int, path string) (net.Conn, string, error) {
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second)
	if err != nil {
		return nil, "", err
	}
	s.SetDeadline(time.Now().Add(5 * time.Second))
	raw := make([]byte, 16)
	rand.Read(raw)
	key := base64.StdEncoding.EncodeToString(raw)
	s.Write([]byte("GET " + path + " HTTP/1.1\r\nHost: " + host + ":" + strconv.Itoa(port) + "\r\nUpgrade: websocket\r\n" +
		"Connection: Upgrade\r\nSec-WebSocket-Key: " + key + "\r\nSec-WebSocket-Version: 13\r\n\r\n"))
	var head []byte
	one := make([]byte, 1)
	for !bytes.HasSuffix(head, []byte("\r\n\r\n")) { // đọc từng byte để không lấy lố sang khung đầu tiên
		if _, err := io.ReadFull(s, one); err != nil {
			s.Close()
			return nil, "", errors.New("Kết nối đóng giữa phản hồi bắt tay")
		}
		head = append(head, one[0])
	}
	lines := strings.Split(string(head), "\r\n")
	hdr := map[string]string{}
	for _, l := range lines[1:] {
		if k, v, ok := strings.Cut(l, ":"); ok {
			hdr[strings.ToLower(strings.TrimSpace(k))] = strings.TrimSpace(v)
		}
	}
	if !strings.HasPrefix(lines[0], "HTTP/1.1 101") || hdr["sec-websocket-accept"] != acceptKey(key) {
		s.Close()
		return nil, "", errors.New("Bắt tay thất bại: " + lines[0])
	}
	return s, lines[0], nil
}

// wsSendClient: gửi một khung FIN = 1; client che dữ liệu bằng khóa 4 byte ngẫu nhiên (mask = true).
func wsSendClient(s net.Conn, opcode byte, data []byte, mask bool) error {
	n := len(data)
	var b1 byte
	if mask {
		b1 = 0x80
	}
	var head []byte
	switch {
	case n < 126:
		head = []byte{0x80 | opcode, b1 | byte(n)}
	case n < 65536:
		head = binary.BigEndian.AppendUint16([]byte{0x80 | opcode, b1 | 126}, uint16(n))
	default:
		head = binary.BigEndian.AppendUint64([]byte{0x80 | opcode, b1 | 127}, uint64(n))
	}
	payload := append([]byte(nil), data...)
	if mask {
		key := make([]byte, 4)
		rand.Read(key)
		for i := range payload {
			payload[i] ^= key[i%4]
		}
		head = append(head, key...)
	}
	_, err := s.Write(append(head, payload...))
	return err
}

func clientMain(args []string) {
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	s, status, err := wsConnect(host, port, "/")
	if err != nil {
		fail("[client] " + err.Error())
	}
	defer func() {
		s.Close()
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Bắt tay thành công: %s\n", status)
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		s.SetDeadline(time.Now().Add(5 * time.Second))
		wsSendClient(s, 0x1, []byte(sc.Text()), true)
		_, opcode, data, err := wsReadFrame(s, s, false) // Mã giả 7.4 với from_client = FALSE
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return
		}
		if opcode == 0x8 {
			fmt.Printf("[client] Server đóng kết nối, mã %d\n", closeCode(data))
			return
		}
		fmt.Printf("[client] Nhận: '%s'\n", data)
	}
	wsSendClient(s, 0x8, binary.BigEndian.AppendUint16(nil, 1000), true) // khung đóng, mã 1000: đóng bình thường
	_, _, data, err := wsReadFrame(s, s, false)
	if err == nil {
		fmt.Printf("[client] Server xác nhận đóng, mã %d\n", closeCode(data))
	}
}
