// Bài 7.2 – Client thử server REST: gửi nhiều yêu cầu trên MỘT kết nối giữ lại (keep-alive).
//
// Dùng lại Reader, READ_LINE, READ_HEADERS, READ_EXACT của Mã giả 7.1 (Mục 7.2.1).
// Mỗi dòng nhập có dạng:  PHƯƠNG_THỨC ĐƯỜNG_DẪN [THÂN_JSON]
// ví dụ  GET /api/v1/terms/TCP   hoặc   POST /api/v1/terms {"term":"QUIC","text":"..."}
//
// Chạy:  out/app client [host] [port]      (mặc định 127.0.0.1 8080)
package main

import (
	"bufio"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

func clientMain(args []string) {
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second) // SAFE_CONNECT (rút gọn)
	if err != nil {
		fail("[client] Không kết nối được: " + err.Error())
	}
	defer func() {
		s.Close()
		fmt.Println("[client] Đã đóng kết nối")
	}()
	r := bufio.NewReader(s) // Reader { sock, pending }
	fmt.Printf("[client] Đã kết nối %s:%d (một kết nối cho mọi yêu cầu)\n", host, port)
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		parts := strings.SplitN(strings.TrimSpace(sc.Text()), " ", 3)
		if len(parts) < 2 {
			continue
		}
		method, path := parts[0], parts[1]
		var body []byte
		if len(parts) == 3 {
			body = []byte(parts[2])
		}
		h := method + " " + path + " HTTP/1.1\r\nHost: " + host + ":" + strconv.Itoa(port) + "\r\nAccept: application/json\r\n"
		if len(body) > 0 {
			h += "Content-Type: application/json\r\nContent-Length: " + strconv.Itoa(len(body)) + "\r\n"
		}
		s.SetDeadline(time.Now().Add(5 * time.Second))
		if _, err := s.Write(append([]byte(h+"\r\n"), body...)); err != nil {
			fmt.Printf("[client] Gửi lỗi: %v\n", err)
			return
		}
		statusLine, err := readLineAsync(r, 8192)
		if err == io.EOF {
			fmt.Println("[client] Server đã đóng kết nối")
			return
		}
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return
		}
		hdr, err := readHeadersAsync(r)
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return
		}
		n, _ := strconv.Atoi(hdr["content-length"])
		data := make([]byte, n)
		if _, err := io.ReadFull(r, data); err != nil { // READ_EXACT
			fmt.Printf("[client] Kết nối đóng giữa chừng: %v\n", err)
			return
		}
		fmt.Printf("[client] %s %s -> %s\n", method, path, statusLine)
		if loc, ok := hdr["location"]; ok {
			fmt.Printf("[client]   Location: %s\n", loc)
		}
		if len(data) > 0 {
			fmt.Printf("[client]   %s\n", data)
		}
	}
}
