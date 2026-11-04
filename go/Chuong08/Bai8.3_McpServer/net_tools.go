// Bài 8.3 – Thân ba công cụ mạng và lớp kiểm soát đích của MCP server.
//
//   - Mã giả 1.1 RESOLVE_HOST, 1.2 PORT_CHECK, 7.8 PING: chép gọn, PRINT đổi thành LOG ra stderr
//     (mọi dòng in ra stdout sẽ chen vào luồng thông điệp MCP – Mục 8.3.5).
//   - Mã giả 8.7 VALIDATE_NAME, VALIDATE_TARGET, VALIDATE_PORT (Mục 8.5.4): đích phải nằm trong ALLOWED_NETS.
//
// Công cụ dùng socket API, không gọi shell.
package main

import (
	"crypto/rand"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"os"
	"regexp"
	"strconv"
	"strings"
	"syscall"
	"time"
)

// NetError: lỗi khi thực thi công cụ (máy không phản hồi...): trả về mô hình với isError = true.
type NetError struct{ msg string }

func (e *NetError) Error() string { return e.msg }

// ResolveError: không phân giải được tên máy (trả về với isError = true).
type ResolveError struct{ msg string }

func (e *ResolveError) Error() string { return e.msg }

// TimeoutError – Timeout của mã giả: hết thời gian chờ (trả về với isError = true).
type TimeoutError struct{ msg string }

func (e *TimeoutError) Error() string { return e.msg }

// PolicyError: đầu vào bị chính sách từ chối: cũng trả về với isError = true.
type PolicyError struct{ msg string }

func (e *PolicyError) Error() string { return e.msg }

// logErr: nhật ký chỉ ghi ra STDERR.
func logErr(parts ...any) {
	fmt.Fprintln(os.Stderr, append([]any{"[mcp]"}, parts...)...)
}

func familyName(ip net.IP) string {
	if ip.To4() != nil {
		return "INET"
	}
	return "INET6"
}

// resolveHost – Mã giả 1.1, RESOLVE_HOST(name), PRINT -> LOG(stderr); lỗi phân giải -> danh sách rỗng.
func resolveHost(name string) []net.IP {
	addrs, err := net.LookupIP(name)
	if err != nil {
		logErr("  resolve: Không phân giải được", name, err)
		return nil
	}
	for _, a := range addrs {
		logErr("  resolve:", familyName(a), a)
	}
	return addrs
}

// portCheck – Mã giả 1.2, PORT_CHECK(host, port, timeout_ms) -> OPEN | CLOSED | FILTERED | UNRESOLVED.
func portCheck(host string, port int, timeoutMs int) string {
	addrs := resolveHost(host)
	if len(addrs) == 0 {
		return "UNRESOLVED"
	}
	state := "FILTERED"
	for _, a := range addrs {
		c, err := net.DialTimeout("tcp", net.JoinHostPort(a.String(), strconv.Itoa(port)),
			time.Duration(timeoutMs)*time.Millisecond) // CONNECT_TIMEOUT
		if err == nil {
			c.Close()
			return "OPEN"
		}
		if errors.Is(err, syscall.ECONNREFUSED) {
			state = "CLOSED" // nhận gói RST
		} // Timeout, NetworkUnreachable, HostUnreachable: thử địa chỉ kế tiếp
	}
	return state
}

// inetChecksum – Mã giả 7.8, INET_CHECKSUM (RFC 1071).
func inetChecksum(b []byte) uint16 {
	if len(b)%2 == 1 {
		b = append(b, 0)
	}
	var sum uint32
	for i := 0; i+1 < len(b); i += 2 {
		sum += uint32(binary.BigEndian.Uint16(b[i:]))
	}
	for sum > 0xFFFF {
		sum = (sum & 0xFFFF) + (sum >> 16)
	}
	return ^uint16(sum)
}

// ping – Mã giả 7.8, PING(host, seq, timeout_ms) bằng socket RAW (cần quyền quản trị).
func ping(dst string, seq int, timeoutMs int) (string, float64, int, error) {
	c, err := net.ListenPacket("ip4:icmp", "0.0.0.0")
	if err != nil {
		return "", 0, 0, &NetError{"Không mở được socket RAW: cần quyền quản trị"}
	}
	s := c.(*net.IPConn)
	defer s.Close()
	id := os.Getpid() & 0xFFFF
	p := make([]byte, 8+32)
	p[0] = 8 // kiểu 8, mã 0
	binary.BigEndian.PutUint16(p[4:], uint16(id))
	binary.BigEndian.PutUint16(p[6:], uint16(seq))
	rand.Read(p[8:])
	binary.BigEndian.PutUint16(p[2:], inetChecksum(p))
	t0 := time.Now()
	if _, err := s.WriteTo(p, &net.IPAddr{IP: net.ParseIP(dst)}); err != nil {
		return "", 0, 0, &NetError{"Gửi gói ICMP lỗi: " + err.Error()}
	}
	s.SetReadDeadline(time.Now().Add(time.Duration(timeoutMs) * time.Millisecond)) // deadline ← NOW() + timeout_ms: hạn chót chung
	d := make([]byte, 1500)
	for { // bỏ qua gói ICMP khác
		n, _, _, addr, err := s.ReadMsgIP(d, nil) // RAW: có cả tiêu đề IP
		if err != nil {
			if ne, isNet := err.(net.Error); isNet && ne.Timeout() {
				return "", 0, 0, &TimeoutError{fmt.Sprintf("Không nhận được Echo Reply trong %d ms", timeoutMs)}
			}
			return "", 0, 0, &NetError{"Nhận gói ICMP lỗi: " + err.Error()}
		}
		ihl := int(d[0]&0x0F) * 4
		if n < ihl+8 {
			continue
		}
		m := d[ihl:n]
		if m[0] == 0 && int(binary.BigEndian.Uint16(m[4:])) == id && int(binary.BigEndian.Uint16(m[6:])) == seq {
			return addr.IP.String(), float64(time.Since(t0).Nanoseconds()) / 1e6, int(d[8]), nil // (địa chỉ, RTT ms, TTL)
		}
	}
}

// ---------------- Mã giả 8.7 – Kiểm soát đích của công cụ mạng ----------------

// parseCidrs – PARSE_CIDRS: danh sách CIDR phân cách bằng dấu phẩy; biến môi trường vắng -> danh sách rỗng.
func parseCidrs(text string) []*net.IPNet {
	var nets []*net.IPNet
	for _, x := range strings.Split(text, ",") {
		if x = strings.TrimSpace(x); x == "" {
			continue
		}
		_, n, err := net.ParseCIDR(x)
		if err != nil {
			logErr("ALLOWED_NETS sai định dạng:", x)
			os.Exit(1)
		}
		nets = append(nets, n)
	}
	return nets
}

var ALLOWED_NETS = parseCidrs(os.Getenv("ALLOWED_NETS"))

var nameRe = regexp.MustCompile(`^[A-Za-z0-9.-]+$`)

// validateName – VALIDATE_NAME(host): địa chỉ IP viết bằng số (kể cả IPv6 "::1") được nhận ngay.
func validateName(v any) (string, error) {
	host, ok := v.(string)
	if ok && net.ParseIP(host) != nil { // IS_IP_ADDRESS(host): vẫn phải qua ALLOWED_NETS ở VALIDATE_TARGET
		return host, nil
	}
	if !ok || len(host) > 253 || !nameRe.MatchString(host) {
		return "", &PolicyError{"Tên máy không hợp lệ"}
	}
	return host, nil
}

// inAny – IN_ANY(a, ALLOWED_NETS).
func inAny(a net.IP) bool {
	for _, n := range ALLOWED_NETS {
		if n.Contains(a) {
			return true
		}
	}
	return false
}

// validateTarget – VALIDATE_TARGET(host): mọi địa chỉ phân giải được đều phải nằm trong ALLOWED_NETS.
func validateTarget(v any) (string, error) {
	host, err := validateName(v)
	if err != nil {
		return "", err
	}
	addrs, err := net.LookupIP(host) // RESOLVE
	if err != nil || len(addrs) == 0 {
		return "", &ResolveError{"Không phân giải được " + host}
	}
	for _, a := range addrs {
		if !inAny(a) {
			return "", &PolicyError{"Ngoài phạm vi cho phép: " + a.String()}
		}
	}
	return addrs[0].String(), nil // dùng đúng địa chỉ đã kiểm tra (chống DNS rebinding)
}

// validatePort – VALIDATE_PORT(p): phải là số nguyên 1–65535 (22.0, "22", true đều bị từ chối).
func validatePort(v any) (int, error) {
	num, ok := v.(json.Number)
	if !ok {
		return 0, &PolicyError{"Cổng không hợp lệ"}
	}
	p, err := strconv.Atoi(num.String())
	if err != nil || p < 1 || p > 65535 {
		return 0, &PolicyError{"Cổng không hợp lệ"}
	}
	return p, nil
}
