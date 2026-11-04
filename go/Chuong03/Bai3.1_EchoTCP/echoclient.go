package main

import (
	"bufio"
	"fmt"
	"net"
	"os"
	"time"
)

// RunEchoClient – Mã giả 3.2 ECHO_CLIENT (phiên bản đầu, chưa định khung).
// Lưu ý (như trong giáo trình): một lần Read giả định nhận đủ phản hồi;
// điều này không đúng với thông điệp lớn và được khắc phục ở Bài 3.2 (định khung).
func RunEchoClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9000) // PARSE_ENDPOINT
	if err != nil {
		fmt.Println(err)
		return 2
	}
	conn, err := SafeConnect(host, port, 3000) // SAFE_CONNECT(host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được %s:%d: %v\n", host, port, err)
		return 1
	}
	defer func() { // FINALLY CLOSE(s)
		conn.Close()
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Đã kết nối %s:%d. Nhập dòng cần gửi (Ctrl+D để kết thúc).\n", host, port)
	buf := make([]byte, 4096)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // WHILE (line ← READ_CONSOLE()) ≠ NULL
		line := console.Text()
		if err := SendAll(conn, []byte(line)); err != nil { // SEND_ALL(ENCODE(line, "UTF-8")): chuỗi Go vốn là UTF-8
			fmt.Printf("[client] Lỗi gửi: %v\n", err)
			return 1
		}
		conn.SetReadDeadline(time.Now().Add(5000 * time.Millisecond)) // SETOPT(RECV_TIMEOUT, 5000)
		n, err := conn.Read(buf)                                      // RECV(s, 4096): giả định nhận đủ một lần
		if err != nil {                                               // LENGTH(reply) = 0 (EOF) hoặc hết thời gian chờ
			fmt.Printf("[client] Dừng nhận: %v\n", err)
			break
		}
		fmt.Printf("[client] Gửi: '%s' -> Nhận: '%s'\n", line, buf[:n]) // PRINT(DECODE(reply, "UTF-8"))
	}
	conn.(*net.TCPConn).CloseWrite() // SHUTDOWN(s, WRITE): báo hết dữ liệu cho server
	return 0
}
