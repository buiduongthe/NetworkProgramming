// Bài 7.1 – Server HTTP thử nghiệm cho client tối giản.
//
// Server tuần tự (khung của Mã giả 3.1) trả về ba kiểu định khung thân của HTTP/1.1 (Mục 7.2.1):
//
//	/api/v1/terms/TCP  -> Content-Length
//	/chunked           -> Transfer-Encoding: chunked (có phần mở rộng khối và trailer)
//	/dong-ket-noi      -> không có độ dài, server đóng kết nối khi hết dữ liệu
//	đường dẫn khác     -> 404
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 8080)
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net"
	"strconv"
	"strings"
)

// readRequestHead: đọc dòng yêu cầu và tiêu đề đến dòng trống (đủ cho server thử).
func readRequestHead(conn net.Conn) (string, bool) {
	var buf []byte
	tmp := make([]byte, 4096)
	for !bytes.Contains(buf, []byte("\r\n\r\n")) {
		n, err := conn.Read(tmp)
		if n == 0 && err != nil {
			return "", false
		}
		buf = append(buf, tmp[:n]...)
	}
	line, _, _ := strings.Cut(string(buf), "\r\n")
	return line, true
}

func respond(conn net.Conn, path string) string {
	switch path {
	case "/api/v1/terms/TCP":
		body, _ := json.Marshal(struct {
			Term string `json:"term"`
			Text string `json:"text"`
		}{"TCP", "Giao thức vận chuyển hướng kết nối"})
		head := "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n" +
			"Content-Length: " + strconv.Itoa(len(body)) + "\r\n\r\n"
		conn.Write(append([]byte(head), body...))
		return "Content-Length"
	case "/chunked":
		parts := []string{"Thân phản hồi ", "được chia thành ", "nhiều khối."}
		out := "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n"
		out += "Transfer-Encoding: chunked\r\nTrailer: X-So-Khoi\r\n\r\n"
		for i, p := range parts {
			ext := ""
			if i == 0 {
				ext = ";vi-du=mo-rong" // phần mở rộng khối (bị client bỏ qua)
			}
			out += fmt.Sprintf("%X%s\r\n%s\r\n", len(p), ext, p) // len(p): số byte UTF-8
		}
		out += "0\r\nX-So-Khoi: 3\r\n\r\n" // khối 0, trailer, dòng trống
		conn.Write([]byte(out))
		return "chunked"
	case "/dong-ket-noi":
		conn.Write([]byte("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n" +
			"Không có Content-Length: client đọc đến khi kết nối đóng."))
		return "đóng kết nối"
	}
	body := `{"type":"about:blank","title":"Not Found","status":404}`
	conn.Write([]byte("HTTP/1.1 404 Not Found\r\nContent-Type: application/problem+json\r\n" +
		"Content-Length: " + strconv.Itoa(len(body)) + "\r\n\r\n" + body))
	return "Content-Length"
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
		line, ok := readRequestHead(conn)
		if ok {
			parts := strings.Split(line, " ")
			path := ""
			if len(parts) > 1 {
				path = parts[1]
			}
			how := respond(conn, path)
			fmt.Printf("[server] %s -> định khung: %s\n", line, how)
		}
		conn.Close()
	}
}
