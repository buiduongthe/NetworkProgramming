package main

import (
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"strings"
)

// runBackend – server đích "chữ hoa": đọc tới EOF (client đã đóng chiều gửi) rồi mới trả lời.
// Vì vậy bộ chuyển tiếp ở giữa phải chuyển tiếp đúng việc đóng một chiều (half-close).
func runBackend(host, port string) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		fmt.Println("[dich] Lỗi:", err)
		os.Exit(1)
	}
	fmt.Printf("[dich] Server chữ hoa lắng nghe tại %s:%s\n", host, port)
	stopOnInterrupt(ln, "[dich] Dừng server")
	for {
		conn, err := ln.Accept()
		if err != nil {
			return // listener đã đóng
		}
		go func(c net.Conn) {
			defer c.Close()
			body, err := io.ReadAll(c) // đọc tới khi Read trả io.EOF
			if err != nil {
				fmt.Println("[dich] Lỗi đọc:", err)
				return
			}
			reply := []byte(strings.ToUpper(string(body)))
			if _, err := c.Write(reply); err != nil {
				fmt.Println("[dich] Lỗi gửi:", err)
				return
			}
			fmt.Printf("[dich] %s gửi %d byte, trả %d byte chữ hoa\n", c.RemoteAddr(), len(body), len(reply))
		}(conn)
	}
}

// stopOnInterrupt – khi nhận Ctrl+C (SIGINT): in thông báo và đóng listener để vòng Accept kết thúc.
func stopOnInterrupt(ln net.Listener, msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		fmt.Println(msg)
		ln.Close()
	}()
}
