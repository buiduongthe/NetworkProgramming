package main

import (
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
)

// RunTestServer – server thử nghiệm tối giản: chấp nhận kết nối, in địa chỉ client rồi đóng ngay.
// Chỉ dùng để tạo trường hợp kết nối thành công; chuỗi SOCKET – BIND – LISTEN – ACCEPT trình bày ở Chương 3.
func RunTestServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9000)
	if err != nil {
		fmt.Printf("[server] Lỗi cấu hình: %v\n", err)
		return 2
	}
	// net.Listen = SOCKET(INET, STREAM) + SETOPT(REUSEADDR) + BIND + LISTEN
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	stopOnCtrlC(srv)
	for {
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			break // socket lắng nghe đã đóng (Ctrl+C)
		}
		fmt.Printf("[server] Client kết nối: %s\n", conn.RemoteAddr())
		conn.Close() // CLOSE ngay
	}
	fmt.Println("[server] Dừng server")
	return 0
}

// stopOnCtrlC đóng socket lắng nghe khi người dùng nhấn Ctrl+C, để vòng ACCEPT kết thúc.
func stopOnCtrlC(srv net.Listener) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		srv.Close()
	}()
}
