// Bài 7.4 – Server Server-Sent Events nhỏ đi kèm bộ đọc luồng.
//
// Phát luồng text/event-stream (Mục 7.3.3) bằng mã hóa khối (chunked) vì server không biết trước độ dài.
// Luồng gồm: chú thích (nhịp tim), retry, hai sự kiện "delta" có id, một sự kiện nhiều dòng data
// (cố ý chia đôi qua hai khối, cắt giữa dòng) và sự kiện "done"; sau đó gửi khối 0 và đóng kết nối.
// Mỗi kết nối một goroutine (Mã giả 6.1).
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 8080)
package main

import (
	"bytes"
	"fmt"
	"net"
	"strconv"
	"strings"
	"time"
)

var EVENTS = []string{
	": nhip tim\n\n",
	"retry: 3000\n\n",
	"id: 1\nevent: delta\ndata: {\"text\":\"Giao thức\"}\n\n",
	"id: 2\nevent: delta\ndata:{\"text\":\" TCP\"}\n\n", // không có dấu cách sau ':' cũng hợp lệ
	"id: 3\ndata: dòng thứ nhất\ndata: dòng th",         // sự kiện này bị cắt giữa dòng ...
	"ứ hai\n\n", // ... và kết thúc ở khối sau
	"event: done\ndata: [DONE]\n\n",
}

// sendChunk: một khối của Transfer-Encoding: chunked – độ dài hex, CRLF, dữ liệu, CRLF.
func sendChunk(conn net.Conn, text string) (int, error) {
	_, err := conn.Write([]byte(fmt.Sprintf("%X\r\n%s\r\n", len(text), text)))
	return len(text), err
}

func serve(conn net.Conn) {
	defer conn.Close()
	addr := conn.RemoteAddr().String()
	var head []byte
	tmp := make([]byte, 4096)
	for !bytes.Contains(head, []byte("\r\n\r\n")) { // đọc dòng yêu cầu và tiêu đề
		n, err := conn.Read(tmp)
		if n == 0 && err != nil {
			return
		}
		head = append(head, tmp[:n]...)
	}
	line, _, _ := strings.Cut(string(head), "\r\n")
	if parts := strings.Split(line, " "); len(parts) < 2 || parts[1] != "/events" {
		conn.Write([]byte("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n"))
		return
	}
	fmt.Printf("[server] Client %s đăng ký luồng: %s\n", addr, line)
	conn.Write([]byte("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n" +
		"Cache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n"))
	for i, ev := range EVENTS {
		n, err := sendChunk(conn, ev)
		if err != nil {
			fmt.Printf("[server] Client %s ngắt kết nối: %v\n", addr, err)
			return
		}
		shown := strings.ReplaceAll(ev, "\n", "\\n") // in '\n' dạng hai ký tự
		fmt.Printf("[server] Gửi khối %d (%d byte): '%s'\n", i+1, n, shown)
		time.Sleep(300 * time.Millisecond) // sự kiện đến dần theo thời gian
	}
	conn.Write([]byte("0\r\n\r\n")) // khối 0: kết thúc thân
	fmt.Printf("[server] Kết thúc luồng cho %s\n", addr)
}

func serverMain(args []string) {
	host := "0.0.0.0"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	for {
		conn, err := srv.Accept()
		if err != nil {
			continue
		}
		go serve(conn)
	}
}
