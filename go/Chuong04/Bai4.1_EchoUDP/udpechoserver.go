package main

import (
	"fmt"
	"net"
	"strconv"
)

// RunUdpEchoServer – Mã giả 4.1 UDP_ECHO_SERVER.
// So với TCP Echo Server (Mã giả 3.1): không có LISTEN/ACCEPT, một socket phục vụ mọi client;
// mỗi lần RECVFROM nhận đúng một datagram nguyên vẹn kèm địa chỉ người gửi.
func RunUdpEchoServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9000) // dòng 2
	if err != nil {
		fmt.Println(err)
		return 2
	}
	// dòng 3–4: net.ListenPacket("udp4", …) = SOCKET(INET, DGRAM) + BIND
	s, err := net.ListenPacket("udp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe UDP tại %s:%d\n", host, port)
	StopOnCtrlC(s)
	buf := make([]byte, 65535)
	for { // dòng 5: LOOP
		n, addr, err := s.ReadFrom(buf) // dòng 6: RECVFROM(s, 65535) – datagram nguyên vẹn
		if isReset(err) {
			continue // Windows: ICMP port unreachable (Mục 4.2.2)
		}
		if err != nil {
			break // socket đã đóng (Ctrl+C)
		}
		fmt.Printf("[server] Nhận %d byte từ %s\n", n, addr) // dòng 7: LOG
		s.WriteTo(buf[:n], addr)                             // dòng 8: SENDTO – trả lời đúng người gửi
	}
	s.Close()
	fmt.Println("[server] Dừng server")
	return 0
}
