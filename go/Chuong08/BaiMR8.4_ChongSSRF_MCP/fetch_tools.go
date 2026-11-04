// Bài mở rộng MR8.4 – Thân công cụ fetch_url: bản NGÂY THƠ và bản AN TOÀN (chống SSRF).
//
// SSRF (Server-Side Request Forgery) là lỗ hổng khi server nhận một URL từ bên ngoài rồi tự đi tải:
// kẻ tấn công ép server truy cập dịch vụ nội bộ (máy chủ siêu dữ liệu 169.254.169.254, dịch vụ chỉ
// nghe loopback, mạng riêng 10/8…) mà client bên ngoài không với tới.
//
// Hai bảng được "tiêm" qua biến môi trường để demo chạy offline, tất định:
//
//	SSRF_RESOLVER = {"tên miền": ["ip1","ip2",...]}  – phân giải tên giả lập; nhiều IP -> mỗi lần trả
//	                IP kế tiếp (mô phỏng DNS rebinding / TOCTOU).
//	SSRF_DIAL     = {"ip": cổng}                      – nối tới 127.0.0.1:cổng thay cho ip thật.
//
// Hai bảng này CHỈ cho demo offline; thực tế dùng DNS thật và nối thẳng tới IP đã kiểm tra.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	maxRedirect = 5
	maxBytes    = 64 * 1024
	hanGiay     = 5 * time.Second
)

// Dải địa chỉ BỊ CHẶN (liệt kê tường minh, KHÔNG dựa vào "is private" vì dải tài liệu cũng private).
var blockedNets []*net.IPNet

func init() {
	for _, c := range []string{
		"0.0.0.0/8", "127.0.0.0/8", "10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16",
		"169.254.0.0/16", "::1/128", "::/128", "fc00::/7", "fe80::/10",
	} {
		_, n, _ := net.ParseCIDR(c)
		blockedNets = append(blockedNets, n)
	}
	if s := os.Getenv("SSRF_RESOLVER"); s != "" {
		json.Unmarshal([]byte(s), &resolverTable)
	}
	if s := os.Getenv("SSRF_DIAL"); s != "" {
		json.Unmarshal([]byte(s), &dialTable)
	}
}

var (
	resolverTable = map[string][]string{}
	dialTable     = map[string]int{}
)

// ssrfError: URL bị chính sách an toàn từ chối. fetchError: lỗi khi thực thi.
type ssrfError struct{ msg string }

func (e *ssrfError) Error() string { return e.msg }

type fetchError struct{ msg string }

func (e *fetchError) Error() string { return e.msg }

// resolveOnce: phân giải tên -> một IP. Bảng giả lập trả IP kế tiếp mỗi lần (IP cuối "dính").
func resolveOnce(host string) (string, error) {
	if net.ParseIP(host) != nil {
		return host, nil // host vốn là IP dạng số
	}
	if lst, ok := resolverTable[host]; ok && len(lst) > 0 {
		ip := lst[0]
		if len(lst) > 1 {
			resolverTable[host] = lst[1:] // advance
		}
		return ip, nil
	}
	ips, err := net.LookupHost(host) // DNS thật (ngoài demo)
	if err != nil || len(ips) == 0 {
		return "", &fetchError{"Không phân giải được tên " + host}
	}
	return ips[0], nil
}

// validateIP: chặn nếu IP thuộc dải nội bộ/loopback/link-local… (dùng cho bản AN TOÀN).
func validateIP(ipStr string) error {
	ip := net.ParseIP(ipStr)
	if ip == nil {
		return &ssrfError{"Chặn SSRF: địa chỉ không hợp lệ " + ipStr}
	}
	for _, n := range blockedNets {
		if n.Contains(ip) {
			return &ssrfError{fmt.Sprintf("Chặn SSRF: địa chỉ %s thuộc dải cấm %s", ipStr, n)}
		}
	}
	return nil
}

// dial: mở kết nối tới (ip, port). Bảng dialTable (chỉ demo) ánh xạ ip -> 127.0.0.1:cổng_cục_bộ.
func dial(ip string, port int, timeout time.Duration) (net.Conn, error) {
	if p, ok := dialTable[ip]; ok {
		return net.DialTimeout("tcp", net.JoinHostPort("127.0.0.1", strconv.Itoa(p)), timeout)
	}
	return net.DialTimeout("tcp", net.JoinHostPort(ip, strconv.Itoa(port)), timeout)
}

func splitURL(raw string) (*url.URL, string, int, string, error) {
	u, err := url.Parse(raw)
	if err != nil {
		return nil, "", 0, "", &fetchError{"URL hỏng: " + err.Error()}
	}
	host := u.Hostname()
	port := 80
	if u.Scheme == "https" {
		port = 443
	}
	if u.Port() != "" {
		port, _ = strconv.Atoi(u.Port())
	}
	path := u.Path
	if path == "" {
		path = "/"
	}
	if u.RawQuery != "" {
		path += "?" + u.RawQuery
	}
	return u, host, port, path, nil
}

// httpGet: gửi GET HTTP/1.0 (Connection: close) tới IP đã chọn, đọc phản hồi (giới hạn kích thước).
func httpGet(ip, host string, port int, path string, timeout time.Duration, limit int64) (int, map[string]string, []byte, error) {
	c, err := dial(ip, port, timeout)
	if err != nil {
		return 0, nil, nil, &fetchError{"Không nối được: " + err.Error()}
	}
	defer c.Close()
	c.SetDeadline(time.Now().Add(timeout))
	req := fmt.Sprintf("GET %s HTTP/1.0\r\nHost: %s\r\nAccept: */*\r\nConnection: close\r\n\r\n", path, host)
	if _, err := c.Write([]byte(req)); err != nil {
		return 0, nil, nil, &fetchError{"Lỗi gửi: " + err.Error()}
	}
	data, _ := io.ReadAll(io.LimitReader(bufio.NewReader(c), limit))
	sep := strings.Index(string(data), "\r\n\r\n")
	if sep < 0 {
		return 0, nil, nil, &fetchError{"Phản hồi hỏng"}
	}
	head := string(data[:sep])
	body := data[sep+4:]
	lines := strings.Split(head, "\r\n")
	status := 0
	if parts := strings.Split(lines[0], " "); len(parts) > 1 {
		status, _ = strconv.Atoi(parts[1])
	}
	headers := map[string]string{}
	for _, ln := range lines[1:] {
		if i := strings.Index(ln, ":"); i > 0 {
			headers[strings.ToLower(strings.TrimSpace(ln[:i]))] = strings.TrimSpace(ln[i+1:])
		}
	}
	return status, headers, body, nil
}

func laChuyenHuong(status int) bool {
	return status == 301 || status == 302 || status == 303 || status == 307 || status == 308
}

func noiTiep(cur, loc string) string {
	base, err := url.Parse(cur)
	if err != nil {
		return loc
	}
	ref, err := url.Parse(loc)
	if err != nil {
		return loc
	}
	return base.ResolveReference(ref).String()
}

// fetchNaive: bản NGÂY THƠ – KHÔNG kiểm tra gì; phân giải tên khi "kiểm tra" rồi phân giải LẠI khi
// nối (lỗi TOCTOU), theo mọi chuyển hướng, không giới hạn. Ví dụ dễ bị SSRF – không dùng thật.
func fetchNaive(raw string) (int, []byte, []string, error) {
	var steps []string
	for i := 0; i <= maxRedirect; i++ {
		u, host, port, path, err := splitURL(raw)
		if err != nil {
			return 0, nil, steps, err
		}
		_ = u
		resolveOnce(host)            // "kiểm tra" hình thức rồi vứt đi (vô dụng)
		ip, err := resolveOnce(host) // nối bằng lần phân giải MỚI -> rebinding lọt
		if err != nil {
			return 0, nil, steps, err
		}
		steps = append(steps, fmt.Sprintf("nối tới %s (%s):%d%s", host, ip, port, path))
		status, headers, body, err := httpGet(ip, host, port, path, hanGiay, 1<<30)
		if err != nil {
			return 0, nil, steps, err
		}
		if laChuyenHuong(status) && headers["location"] != "" {
			raw = noiTiep(raw, headers["location"])
			steps = append(steps, "theo chuyển hướng -> "+raw)
			continue
		}
		return status, body, steps, nil
	}
	return 0, nil, steps, &fetchError{"Quá nhiều chuyển hướng"}
}

// fetchSafe: bản AN TOÀN – lần lượt áp các lớp phòng thủ SSRF.
func fetchSafe(raw string) (int, []byte, []string, error) {
	var steps []string
	deadline := time.Now().Add(hanGiay)
	for hop := 0; hop <= maxRedirect; hop++ {
		u, host, port, path, err := splitURL(raw)
		if err != nil {
			return 0, nil, steps, err
		}
		if u.Scheme != "http" && u.Scheme != "https" { // (1) chỉ cho http/https
			return 0, nil, steps, &ssrfError{"Chặn SSRF: giao thức '" + u.Scheme + "' không được phép"}
		}
		if u.User != nil { // (2) loại bỏ thông tin đăng nhập
			steps = append(steps, "gỡ bỏ thông tin đăng nhập nhúng trong URL")
		}
		if host == "" {
			return 0, nil, steps, &ssrfError{"Chặn SSRF: URL thiếu tên máy"}
		}
		ip, err := resolveOnce(host) // (3) phân giải MỘT lần
		if err != nil {
			return 0, nil, steps, err
		}
		if err := validateIP(ip); err != nil { // (4) chặn dải nội bộ/loopback/link-local
			return 0, nil, steps, err
		}
		conLai := time.Until(deadline)
		if conLai <= 0 { // (5) giới hạn tổng thời gian
			return 0, nil, steps, &fetchError{"Hết thời gian cho phép"}
		}
		steps = append(steps, fmt.Sprintf("cho phép %s (%s):%d%s; nối tới ĐÚNG IP đã kiểm tra", host, ip, port, path))
		to := 2 * time.Second
		if conLai < to {
			to = conLai
		}
		// (6) nối tới chính IP đã kiểm tra (chống rebinding/TOCTOU), (7) giới hạn kích thước
		status, headers, body, err := httpGet(ip, host, port, path, to, maxBytes)
		if err != nil {
			return 0, nil, steps, err
		}
		if laChuyenHuong(status) && headers["location"] != "" {
			if hop >= maxRedirect { // (8) giới hạn số lần chuyển hướng
				return 0, nil, steps, &ssrfError{"Chặn SSRF: vượt số lần chuyển hướng cho phép"}
			}
			raw = noiTiep(raw, headers["location"])
			steps = append(steps, "kiểm tra lại chặng chuyển hướng -> "+raw)
			continue // (9) kiểm tra lại MỌI chặng
		}
		return status, body, steps, nil
	}
	return 0, nil, steps, &fetchError{"Quá nhiều chuyển hướng"}
}

func logErr(parts ...any) {
	fmt.Fprintln(os.Stderr, append([]any{"[ssrf]"}, parts...)...)
}
