// Bài 7.7 – Gửi ICMP Echo Request và đo thời gian khứ hồi (socket RAW).
//
// Cài đặt Mã giả 7.8 (Mục 7.5.3, Giáo trình Lập trình mạng): INET_CHECKSUM (RFC 1071) và PING.
// Kế thừa: Mã giả 4.4 – cùng mẫu gửi rồi chờ phản hồi có hạn giờ, bỏ qua gói không khớp.
// Socket RAW cần quyền quản trị: Linux/macOS chạy bằng sudo (hoặc tài khoản root).
//
// Chạy:  sudo out/app [host] [số_lần] [thời_gian_chờ_ms]   (mặc định 127.0.0.1 4 1000)
package main

import (
	"crypto/rand"
	"encoding/binary"
	"errors"
	"fmt"
	"net"
	"os"
	"strconv"
	"time"
)

// inetChecksum – Mã giả 7.8, dòng 1–11: tổng kiểm tra Internet (RFC 1071).
func inetChecksum(b []byte) uint16 {
	if len(b)%2 == 1 {
		b = append(b, 0) // thêm BYTE(0) khi độ dài lẻ
	}
	var sum uint32
	for i := 0; i+1 < len(b); i += 2 {
		sum += uint32(binary.BigEndian.Uint16(b[i:])) // FROM_UINT16_BE(b[i..i+1])
	}
	for sum > 0xFFFF {
		sum = (sum & 0xFFFF) + (sum >> 16) // cộng phần tràn
	}
	return ^uint16(sum) // (NOT sum) AND 0xFFFF
}

// errPermission: không mở được socket RAW vì thiếu quyền quản trị.
var errPermission = errors.New("cần quyền quản trị")

// ping – Mã giả 7.8, dòng 13–35: trả về (địa chỉ, RTT, TTL) hoặc lỗi (hết thời gian chờ…).
func ping(host string, seq int, timeoutMs int) (string, time.Duration, int, error) {
	ips, err := net.LookupIP(host) // RESOLVE(host)
	if err != nil {
		return "", 0, 0, err
	}
	var dst net.IP
	for _, ip := range ips { // FIRST_IPV4
		if ip.To4() != nil {
			dst = ip
			break
		}
	}
	if dst == nil {
		return "", 0, 0, errors.New("không có địa chỉ IPv4")
	}
	c, err := net.ListenPacket("ip4:icmp", "0.0.0.0") // SOCKET(INET, RAW, ICMP): cần quyền quản trị
	if errors.Is(err, os.ErrPermission) {
		return "", 0, 0, errPermission
	}
	if err != nil {
		return "", 0, 0, err
	}
	s := c.(*net.IPConn)
	defer s.Close() // FINALLY: CLOSE(s)
	id := os.Getpid() & 0xFFFF
	p := make([]byte, 8, 8+32)
	p[0], p[1] = 8, 0                                  // kiểu 8 (Echo Request), mã 0, checksum 0
	binary.BigEndian.PutUint16(p[4:], uint16(id))      // UINT16_BE(id)
	binary.BigEndian.PutUint16(p[6:], uint16(seq))     // UINT16_BE(seq)
	p = append(p, make([]byte, 32)...)                 // phần dữ liệu
	rand.Read(p[8:])                                   // RANDOM_BYTES(32)
	binary.BigEndian.PutUint16(p[2:], inetChecksum(p)) // điền tổng kiểm tra
	if seq == 1 {
		fmt.Printf("[ping] Tiêu đề ICMP gửi đi: % X (kiểu, mã, checksum, id, seq)\n", p[:8])
	}
	t0 := time.Now()                                              // NOW_PRECISE()
	if _, err := s.WriteTo(p, &net.IPAddr{IP: dst}); err != nil { // SENDTO(s, p, (dst, 0))
		return "", 0, 0, err
	}
	// deadline ← NOW() + timeout_ms: hạn chót chung cho cả vòng nhận (Go dùng hạn chót tuyệt đối,
	// tương đương SETOPT(RECV_TIMEOUT, MAX(1, deadline − NOW())) trước mỗi lần RECVFROM)
	s.SetReadDeadline(time.Now().Add(time.Duration(timeoutMs) * time.Millisecond))
	d := make([]byte, 1500)
	for { // bỏ qua gói ICMP khác
		// RECVFROM(s, 1500): ReadMsgIP trả về cả tiêu đề IP (ReadFrom của Go tự cắt tiêu đề IPv4)
		n, _, _, addr, err := s.ReadMsgIP(d, nil)
		if err != nil {
			return "", 0, 0, err
		}
		ihl := int(d[0]&0x0F) * 4 // độ dài tiêu đề IPv4
		if n < ihl+8 {
			continue
		}
		m := d[ihl:n]
		if m[0] == 0 && int(binary.BigEndian.Uint16(m[4:])) == id && int(binary.BigEndian.Uint16(m[6:])) == seq {
			return addr.IP.String(), time.Since(t0), int(d[8]), nil // d[8]: TTL
		}
		fmt.Printf("[ping]   bỏ qua gói ICMP kiểu %d (không phải Echo Reply của tiến trình này)\n", m[0])
	}
}

func intArg(args []string, i int, def int) int {
	if len(args) <= i {
		return def
	}
	v, err := strconv.Atoi(args[i])
	if err != nil || v < 1 {
		fmt.Fprintln(os.Stderr, "Tham số phải là số nguyên dương: "+args[i])
		os.Exit(1)
	}
	return v
}

func main() {
	args := os.Args[1:]
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	count := intArg(args, 1, 4)
	timeoutMs := intArg(args, 2, 1000)
	fmt.Printf("[ping] PING %s: %d gói Echo Request, dữ liệu 32 byte, chờ tối đa %d ms\n", host, count, timeoutMs)
	var rtts []float64
	for seq := 1; seq <= count; seq++ {
		addr, rtt, ttl, err := ping(host, seq, timeoutMs)
		var ne net.Error
		switch {
		case err == nil:
			ms := float64(rtt.Nanoseconds()) / 1e6
			rtts = append(rtts, ms)
			fmt.Printf("[ping] Trả lời từ %s: seq=%d ttl=%d thời gian=%.3f ms\n", addr, seq, ttl, ms)
		case errors.As(err, &ne) && ne.Timeout():
			fmt.Printf("[ping] seq=%d: hết thời gian chờ\n", seq)
		case errors.Is(err, errPermission):
			fmt.Fprintln(os.Stderr, "[ping] Socket RAW cần quyền quản trị (sudo hoặc tài khoản root)")
			os.Exit(1)
		default:
			fmt.Printf("[ping] seq=%d: lỗi %v\n", seq, err)
		}
		if seq < count {
			time.Sleep(time.Second)
		}
	}
	lost := 100 * (count - len(rtts)) / count
	avg := "không có"
	if len(rtts) > 0 {
		sum := 0.0
		for _, x := range rtts {
			sum += x
		}
		avg = fmt.Sprintf("%.3f ms", sum/float64(len(rtts)))
	}
	fmt.Printf("[ping] Đã gửi %d, nhận %d, mất %d%%; RTT trung bình %s\n", count, len(rtts), lost, avg)
}
