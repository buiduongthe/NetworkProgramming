package main

import (
	"bufio"
	"errors"
	"fmt"
	"net"
	"os"
	"strings"
	"time"
)

// RunUdpEchoClient – Mã giả 4.2 UDP_ECHO_CLIENT.
// Đọc từng dòng từ bàn phím, gửi một datagram tới server và chờ phản hồi tối đa 2 giây;
// hết thời gian chờ thì báo datagram có thể đã mất (không có kết nối nên không có lỗi nào khác).
func RunUdpEchoClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9000) // dòng 2
	if err != nil {
		fmt.Println(err)
		return 2
	}
	ips, err := net.LookupIP(host) // dòng 3: server ← (FIRST(RESOLVE(host)), port)
	if err != nil {
		fmt.Printf("[client] Không phân giải được %s: %v\n", host, err)
		return 1
	}
	server := &net.UDPAddr{IP: ips[0], Port: port}
	network := "udp4" // dòng 4: SOCKET(FAMILY(server), DGRAM)
	if ips[0].To4() == nil {
		network = "udp6"
	}
	s, err := net.ListenUDP(network, nil) // socket chưa kết nối, cổng nguồn do HĐH chọn
	if err != nil {
		fmt.Printf("[client] Không tạo được socket: %v\n", err)
		return 1
	}
	defer func() {
		s.Close() // dòng 15: CLOSE(s)
		fmt.Println("[client] Đã đóng socket")
	}()
	fmt.Printf("[client] Gửi tới %s:%d qua UDP. Nhập dòng cần gửi (Ctrl+Z/Ctrl+D để kết thúc).\n", host, port)
	buf := make([]byte, 65535)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // dòng 6: READ_CONSOLE()
		line := strings.TrimRight(console.Text(), "\r")
		s.WriteToUDP([]byte(line), server)                         // dòng 7: SENDTO(s, ENCODE(line, "UTF-8"), server)
		s.SetReadDeadline(time.Now().Add(2000 * time.Millisecond)) // dòng 5: RECV_TIMEOUT 2000 ms
		n, _, err := s.ReadFromUDP(buf)                            // dòng 9: RECVFROM(s, 65535)
		var ne net.Error
		if (errors.As(err, &ne) && ne.Timeout()) || isReset(err) { // dòng 11: CATCH Timeout, ConnectionReset
			// ConnectionReset: Windows báo ICMP port unreachable (Mục 4.2.2)
			fmt.Printf("[client] Gửi: '%s' -> Không có phản hồi: datagram có thể đã mất\n", line)
			continue
		}
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		fmt.Printf("[client] Gửi: '%s' -> Nhận: '%s'\n", line, buf[:n]) // PRINT(DECODE(data, "UTF-8"))
	}
	return 0
}
