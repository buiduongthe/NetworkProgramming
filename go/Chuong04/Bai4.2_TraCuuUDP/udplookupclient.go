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

// Client tra cứu thuật ngữ trên UDP, gửi lại có lùi thời gian chờ – Mã giả 4.4 UDP_REQUEST (Mục 4.3.1).
// Kế thừa: Mã giả 4.2 (client UDP có thời gian chờ), vòng đọc thuật ngữ của client tra cứu TCP.
// Mỗi yêu cầu mang id; client gửi lại tối đa maxTries lần, thời gian chờ nhân đôi sau mỗi lần,
// bỏ qua datagram không đến từ server hoặc không khớp id.

var lastID = 0

// nextID – NEXT_ID(): mã định danh tăng dần 1, 2, 3...
func nextID() int {
	lastID++
	return lastID
}

// UdpRequest – Mã giả 4.4 UDP_REQUEST(s, server, req, timeout_ms, max_tries).
// Các dòng in "[client]   ..." là nhật ký minh họa, không có trong mã giả.
func UdpRequest(s *net.UDPConn, server *net.UDPAddr, req map[string]any, timeoutMs int, maxTries int) (map[string]any, error) {
	id := nextID()
	req["id"] = id          // dòng 2: req.id ← NEXT_ID()
	payload := ToBytes(req) // dòng 3
	buf := make([]byte, 65535)
	for attempt := 1; attempt <= maxTries; attempt++ { // dòng 4
		s.WriteToUDP(payload, server) // dòng 5: SENDTO(s, payload, server)
		fmt.Printf("[client]   Lần %d: gửi id=%d (%d byte), chờ tối đa %d ms\n", attempt, id, len(payload), timeoutMs)
		deadline := time.Now().Add(time.Duration(timeoutMs) * time.Millisecond) // dòng 6
		for time.Now().Before(deadline) {                                       // dòng 7: WHILE NOW() < deadline
			s.SetReadDeadline(deadline)        // dòng 8: deadline của Go là mốc tuyệt đối = phần thời gian còn lại
			n, addr, err := s.ReadFromUDP(buf) // dòng 10: RECVFROM(s, 65535)
			var ne net.Error
			if (errors.As(err, &ne) && ne.Timeout()) || isReset(err) {
				break // dòng 12–13: CATCH Timeout, ConnectionReset (Windows: ICMP) → BREAK
			}
			if err != nil {
				return nil, err
			}
			resp, err := FromBytes(buf[:n]) // dòng 11: JSON_DECODE
			if err != nil {                 // dòng 14–15: CATCH JsonError → CONTINUE
				continue
			}
			if addr.IP.Equal(server.IP) && addr.Port == server.Port && GetInt(resp, "id") == id {
				return resp, nil // dòng 17: IF addr = server AND resp.id = req.id THEN RETURN resp
			}
			fmt.Printf("[client]   Bỏ datagram lạ hoặc muộn từ %s\n", addr)
		}
		fmt.Printf("[client]   Hết %d ms không có phản hồi\n", timeoutMs)
		timeoutMs *= 2 // dòng 19: lùi theo cấp số nhân
	}
	return nil, fmt.Errorf("Không có phản hồi sau %d lần", maxTries) // dòng 21: RAISE NetError
}

// RunUdpLookupClient – vòng đọc thuật ngữ, mỗi thuật ngữ một lần UdpRequest(…, 500, 4).
func RunUdpLookupClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	ips, err := net.LookupIP(host) // FIRST(RESOLVE(host))
	if err != nil {
		fmt.Printf("[client] Không phân giải được %s: %v\n", host, err)
		return 1
	}
	server := &net.UDPAddr{IP: ips[0], Port: port}
	network := "udp4"
	if ips[0].To4() == nil {
		network = "udp6"
	}
	s, err := net.ListenUDP(network, nil) // SOCKET(FAMILY(server), DGRAM)
	if err != nil {
		fmt.Printf("[client] Không tạo được socket: %v\n", err)
		return 1
	}
	defer func() {
		s.Close()
		fmt.Println("[client] Đã đóng socket")
	}()
	fmt.Printf("[client] Gửi yêu cầu tới %s:%d qua UDP (chờ 500 ms, tối đa 4 lần). Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).\n", host, port)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // READ_CONSOLE()
		term := strings.TrimSpace(console.Text())
		if term == "" {
			continue
		}
		fmt.Printf("[client] Tra cứu '%s'\n", term)
		t0 := time.Now()
		resp, err := UdpRequest(s, server, map[string]any{"op": "lookup", "term": term}, 500, 4)
		ms := time.Since(t0).Milliseconds()
		if err != nil {
			fmt.Printf("[client] Lỗi: %v (%d ms)\n", err, ms)
			continue
		}
		if GetInt(resp, "status") == 200 {
			t, _ := GetString(resp, "term")
			text, _ := GetString(resp, "text")
			fmt.Printf("[client] %s: %s (%d ms)\n", t, text, ms)
		} else {
			e, _ := GetString(resp, "error")
			fmt.Printf("[client] Lỗi %d: %s (%d ms)\n", GetInt(resp, "status"), e, ms)
		}
	}
	return 0
}
