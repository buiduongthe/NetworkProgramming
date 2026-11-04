package main

import (
	"fmt"
	"net"
	"time"
)

// RunRawSend – công cụ thử (không thuộc mã giả): gửi nguyên văn một datagram, không đệm, không cookie,
// rồi chờ phản hồi tối đa 1 giây. Dùng để quan sát server bỏ im lặng yêu cầu quá nhỏ.
func RunRawSend(args []string) int {
	if len(args) < 3 {
		fmt.Println("Cách dùng: go run . raw <host> <port> <nội dung>")
		return 1
	}
	host, port, err := ParseEndpoint(args[:2], "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	server, err := net.ResolveUDPAddr("udp4", net.JoinHostPort(host, fmt.Sprint(port)))
	if err != nil {
		fmt.Printf("[thử] Không phân giải được %s: %v\n", host, err)
		return 1
	}
	s, err := net.ListenUDP("udp4", nil)
	if err != nil {
		fmt.Printf("[thử] Không tạo được socket: %v\n", err)
		return 1
	}
	defer s.Close()
	s.WriteToUDP([]byte(args[2]), server)
	fmt.Printf("[thử] Gửi %d byte %s (không đệm, không cookie)\n", len(args[2]), args[2])
	s.SetReadDeadline(time.Now().Add(time.Second))
	buf := make([]byte, 65535)
	n, _, err := s.ReadFromUDP(buf)
	if err != nil {
		fmt.Println("[thử] Không có phản hồi sau 1 s (server bỏ im lặng)")
		return 0
	}
	fmt.Printf("[thử] Nhận %d byte: %s\n", n, buf[:n])
	return 0
}
