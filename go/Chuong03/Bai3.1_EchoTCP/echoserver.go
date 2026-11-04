package main

import (
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
)

// RunEchoServer – Mã giả 3.1 ECHO_SERVER: server tuần tự, phục vụ lần lượt từng client.
func RunEchoServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9000) // PARSE_ENDPOINT
	if err != nil {
		fmt.Println(err)
		return 2
	}
	// net.Listen gộp SOCKET(INET, STREAM), SETOPT(REUSEADDR, TRUE), BIND và LISTEN
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	StopOnCtrlC(srv)
	for { // LOOP: vòng chấp nhận kết nối
		conn, err := srv.Accept() // ACCEPT: chờ client mới
		if err != nil {
			break // socket lắng nghe đã đóng (Ctrl+C)
		}
		addr := conn.RemoteAddr()
		fmt.Printf("[server] Client kết nối: %s\n", addr)
		if err := HandleEcho(conn); err != nil { // CATCH NetError, ConnectionClosed...
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err) // LOG: không dừng server
		}
		conn.Close() // FINALLY CLOSE(conn)
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}
	fmt.Println("[server] Dừng server")
	return 0
}

// HandleEcho – vòng phục vụ một client: nhận bao nhiêu gửi trả bấy nhiêu.
func HandleEcho(conn net.Conn) error {
	buf := make([]byte, 4096)
	for {
		n, err := conn.Read(buf) // RECV(conn, 4096)
		if n > 0 {
			if err := SendAll(conn, buf[:n]); err != nil { // SEND_ALL
				return err
			}
		}
		if err != nil {
			if errors.Is(err, io.EOF) { // LENGTH(data) = 0: client đã đóng chiều gửi
				return nil
			}
			return err
		}
	}
}
