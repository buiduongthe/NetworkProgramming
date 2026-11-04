package main

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"net"
	"os"
	"strings"
	"time"
)

// Client tra cứu UDP có xử lý mã thách thức (cookie) của server.
// Theo đoạn văn sau Mã giả 4.8 (Mục 4.6.1): khi nhận phản hồi 401 kèm cookie, client lưu mã và gửi lại
// yêu cầu có trường cookie; các yêu cầu sau dùng lại mã đó. Yêu cầu chưa có cookie được đệm khoảng trắng
// tới MinFirstRequest byte để thách thức 401 không vượt MAX_RATIO lần yêu cầu.
// Kế thừa: Mã giả 4.4 UDP_REQUEST (chép từ Bài 4.2).

// MinFirstRequest – độ dài tối thiểu (byte) của yêu cầu chưa có cookie: 3 × 40 = 120 byte ≥ thách thức (~97–100 byte).
const MinFirstRequest = 40

var lastID = 0

func nextID() int {
	lastID++
	return lastID
}

// UdpRequest – Mã giả 4.4 (chép từ Bài 4.2); pad > 0 thì đệm khoảng trắng sau JSON tới pad byte.
func UdpRequest(s *net.UDPConn, server *net.UDPAddr, req map[string]any, timeoutMs int, maxTries int, pad int) (map[string]any, error) {
	id := nextID()
	req["id"] = id
	payload := ToBytes(req)
	if len(payload) < pad { // JSON cho phép khoảng trắng sau giá trị
		payload = append(payload, bytes.Repeat([]byte(" "), pad-len(payload))...)
	}
	buf := make([]byte, 65535)
	for attempt := 1; attempt <= maxTries; attempt++ {
		s.WriteToUDP(payload, server)
		fmt.Printf("[client]   Lần %d: gửi id=%d (%d byte), chờ tối đa %d ms\n", attempt, id, len(payload), timeoutMs)
		deadline := time.Now().Add(time.Duration(timeoutMs) * time.Millisecond)
		for time.Now().Before(deadline) {
			s.SetReadDeadline(deadline)
			n, addr, err := s.ReadFromUDP(buf)
			var ne net.Error
			if (errors.As(err, &ne) && ne.Timeout()) || isReset(err) {
				break
			}
			if err != nil {
				return nil, err
			}
			resp, err := FromBytes(buf[:n])
			if err != nil {
				continue
			}
			if addr.IP.Equal(server.IP) && addr.Port == server.Port && GetInt(resp, "id") == id {
				return resp, nil
			}
			fmt.Printf("[client]   Bỏ datagram lạ hoặc muộn từ %s\n", addr)
		}
		fmt.Printf("[client]   Hết %d ms không có phản hồi\n", timeoutMs)
		timeoutMs *= 2
	}
	return nil, fmt.Errorf("Không có phản hồi sau %d lần", maxTries)
}

func lookup(s *net.UDPConn, server *net.UDPAddr, term, cookie string) (map[string]any, error) {
	req := map[string]any{"op": "lookup", "term": term}
	pad := MinFirstRequest
	if cookie != "" {
		req["cookie"] = cookie // gửi kèm cookie đã lưu
		pad = 0
	}
	return UdpRequest(s, server, req, 500, 4, pad)
}

// RunCookieClient – vòng đọc thuật ngữ, xử lý thách thức 401.
func RunCookieClient(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	cookie := ""
	if len(args) > 2 {
		cookie = args[2]
	}
	ips, err := net.LookupIP(host)
	if err != nil {
		fmt.Printf("[client] Không phân giải được %s: %v\n", host, err)
		return 1
	}
	server := &net.UDPAddr{IP: ips[0], Port: port}
	network := "udp4"
	if ips[0].To4() == nil {
		network = "udp6"
	}
	s, err := net.ListenUDP(network, nil)
	if err != nil {
		fmt.Printf("[client] Không tạo được socket: %v\n", err)
		return 1
	}
	defer func() {
		s.Close()
		fmt.Println("[client] Đã đóng socket")
	}()
	fmt.Printf("[client] Gửi yêu cầu tới %s:%d qua UDP. Nhập thuật ngữ (Ctrl+Z/Ctrl+D để kết thúc).\n", host, port)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() {
		term := strings.TrimSpace(console.Text())
		if term == "" {
			continue
		}
		withCookie := "không kèm cookie"
		if cookie != "" {
			withCookie = "kèm cookie"
		}
		fmt.Printf("[client] Tra cứu '%s' (%s)\n", term, withCookie)
		resp, err := lookup(s, server, term, cookie)
		if err == nil {
			if c, ok := GetString(resp, "cookie"); ok && GetInt(resp, "status") == 401 {
				cookie = c // lưu mã thách thức
				fmt.Printf("[client] Nhận thách thức 401, lưu cookie %s…; gửi lại kèm cookie\n", cookie[:min(16, len(cookie))])
				resp, err = lookup(s, server, term, cookie) // gửi lại đúng một lần
			}
		}
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			continue
		}
		if GetInt(resp, "status") == 200 {
			t, _ := GetString(resp, "term")
			text, _ := GetString(resp, "text")
			fmt.Printf("[client] %s: %s\n", t, text)
		} else {
			e, ok := GetString(resp, "error")
			if !ok {
				e = "không được phục vụ"
			}
			fmt.Printf("[client] Lỗi %d: %s\n", GetInt(resp, "status"), e)
		}
	}
	return 0
}
