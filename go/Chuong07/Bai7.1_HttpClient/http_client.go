// Bài 7.1 – Client HTTP/1.1 tối giản trên socket TCP.
//
// Cài đặt Mã giả 7.1 (Mục 7.2.1, Giáo trình Lập trình mạng): HTTP_REQUEST, READ_HEADERS, READ_CHUNKED.
// Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.4 (RECV_EXACT), 3.5 (READ_LINE, cấu trúc Reader).
// Client gửi GET tới từng đường dẫn và in mã trạng thái, cách định khung thân và nội dung thân.
//
// Chạy:  out/app client [host] [port] [đường_dẫn ...]   (mặc định 127.0.0.1 8080 /)
package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
	"time"
)

// safeConnect – Mã giả 2.2 (rút gọn): kết nối có thời gian chờ.
func safeConnect(host string, port int, timeoutMs int) (net.Conn, error) {
	addr := net.JoinHostPort(host, strconv.Itoa(port))
	conn, err := net.DialTimeout("tcp", addr, time.Duration(timeoutMs)*time.Millisecond)
	if err != nil {
		return nil, fmt.Errorf("Không kết nối được %s: %v", addr, err)
	}
	return conn, nil
}

// Reader – Mã giả 3.5, STRUCT Reader { sock, pending }: giữ phần byte nhận dư giữa các lần đọc.
type Reader struct {
	sock    io.Reader
	pending []byte
}

// readLine – Mã giả 3.5, READ_LINE: trả về dòng (bỏ CRLF); io.EOF (NULL) khi đối tác đã đóng.
func readLine(r *Reader, maxLen int) (string, error) {
	buf := make([]byte, 4096)
	for {
		if i := bytes.IndexByte(r.pending, '\n'); i >= 0 {
			line := r.pending[:i]
			r.pending = r.pending[i+1:]
			return strings.TrimRight(string(line), "\r"), nil
		}
		if len(r.pending) > maxLen {
			return "", errors.New("Dòng quá dài")
		}
		n, err := r.sock.Read(buf) // RECV(sock, 4096)
		r.pending = append(r.pending, buf[:n]...)
		if n == 0 && err != nil {
			return "", err // io.EOF: đối tác đã đóng
		}
	}
}

// readExact – READ_EXACT: lấy phần dư r.pending trước, thiếu thì RECV_EXACT (Mã giả 3.4).
func readExact(r *Reader, n int) ([]byte, error) {
	k := min(n, len(r.pending))
	data := append([]byte(nil), r.pending[:k]...)
	r.pending = r.pending[k:]
	buf := make([]byte, 4096)
	for len(data) < n {
		m, err := r.sock.Read(buf[:min(len(buf), n-len(data))])
		data = append(data, buf[:m]...)
		if m == 0 && err != nil {
			return nil, fmt.Errorf("Kết nối đóng khi mới nhận %d/%d byte", len(data), n)
		}
	}
	return data, nil
}

// readUntilClose – READ_UNTIL_CLOSE: đọc đến khi server đóng kết nối.
func readUntilClose(r *Reader) ([]byte, error) {
	rest, err := io.ReadAll(r.sock)
	data := append(r.pending, rest...)
	r.pending = nil
	return data, err
}

// ConnectionClosed: kết nối đóng giữa chừng khi chưa đọc xong phần tiêu đề.
var ConnectionClosed = errors.New("Kết nối đóng giữa phần tiêu đề")

// readHeaders – Mã giả 7.1, READ_HEADERS: đọc các trường tiêu đề đến dòng trống.
func readHeaders(r *Reader) (map[string]string, error) {
	hdr := map[string]string{}
	for {
		line, err := readLine(r, 8192)
		if err == io.EOF { // line = NULL: RAISE ConnectionClosed() – đóng giữa chừng
			return nil, ConnectionClosed
		}
		if err != nil {
			return nil, err
		}
		if line == "" { // dòng trống: hết
			return hdr, nil
		}
		k, v, _ := strings.Cut(line, ":")                                 // SPLIT_FIRST tại dấu ':' đầu tiên
		hdr[strings.ToLower(strings.TrimSpace(k))] = strings.TrimSpace(v) // không phân biệt hoa/thường
	}
}

// readChunked – Mã giả 7.1, dòng 33–43: giải mã thân Transfer-Encoding: chunked.
func readChunked(r *Reader) ([]byte, error) {
	var data []byte
	for {
		line, err := readLine(r, 1024)
		if err != nil {
			return nil, err
		}
		sizeText, _, _ := strings.Cut(line, ";") // bỏ phần mở rộng sau ';'
		n, err := strconv.ParseInt(strings.TrimSpace(sizeText), 16, 64)
		if err != nil {
			return nil, errors.New("Độ dài khối không hợp lệ: " + line)
		}
		if n == 0 {
			break
		}
		chunk, err := readExact(r, int(n))
		if err != nil {
			return nil, err
		}
		data = append(data, chunk...)
		readLine(r, 2) // CRLF sau mỗi khối
	}
	_, err := readHeaders(r) // bỏ phần trailer
	return data, err
}

// header: một cặp (tên, giá trị) – giữ đúng thứ tự gửi.
type header struct{ k, v string }

// httpRequest – Mã giả 7.1, dòng 1–23: gửi một yêu cầu HTTP/1.1 và đọc phản hồi.
func httpRequest(host string, port int, method, path string, headers []header, body []byte) (int, map[string]string, []byte, error) {
	s, err := safeConnect(host, port, 3000)
	if err != nil {
		return 0, nil, nil, err
	}
	defer s.Close() // CLOSE(s)
	s.SetReadDeadline(time.Now().Add(5 * time.Second))
	r := &Reader{sock: s}
	h := method + " " + path + " HTTP/1.1\r\n"
	h += "Host: " + host + ":" + strconv.Itoa(port) + "\r\n" // kèm cổng
	for _, kv := range headers {
		h += kv.k + ": " + kv.v + "\r\n"
	}
	if body != nil {
		h += "Content-Length: " + strconv.Itoa(len(body)) + "\r\n"
	}
	h += "Connection: close\r\n\r\n"                               // dòng trống kết thúc tiêu đề
	if _, err := s.Write(append([]byte(h), body...)); err != nil { // SEND_ALL
		return 0, nil, nil, err
	}
	line, err := readLine(r, 8192)
	if err == io.EOF {
		return 0, nil, nil, errors.New("Server đóng kết nối trước khi gửi phản hồi")
	}
	if err != nil {
		return 0, nil, nil, err
	}
	parts := strings.Split(line, " ")
	if len(parts) < 2 {
		return 0, nil, nil, errors.New("Dòng trạng thái sai: " + line)
	}
	status, _ := strconv.Atoi(parts[1]) // TO_INT(SPLIT(…, " ")[1])
	hdr, err := readHeaders(r)
	if err != nil {
		return 0, nil, nil, err
	}
	var data []byte
	te := strings.ToLower(hdr["transfer-encoding"])
	cl, hasLength := hdr["content-length"]
	switch {
	case strings.HasSuffix(te, "chunked"): // vd. "gzip, chunked"
		data, err = readChunked(r)
	case hasLength:
		n, _ := strconv.Atoi(cl)
		data, err = readExact(r, n)
	default:
		data, err = readUntilClose(r)
	}
	return status, hdr, data, err
}

func clientMain(args []string) {
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	paths := []string{"/"}
	if len(args) > 2 {
		paths = args[2:]
	}
	for _, path := range paths {
		status, hdr, data, err := httpRequest(host, port, "GET", path, []header{{"Accept", "application/json"}}, nil)
		if err != nil {
			fmt.Printf("[client] GET %s -> lỗi: %v\n", path, err)
			continue
		}
		fmt.Printf("[client] GET %s -> %d\n", path, status)
		how := "đọc đến khi server đóng kết nối"
		if strings.HasSuffix(strings.ToLower(hdr["transfer-encoding"]), "chunked") {
			how = "Transfer-Encoding = chunked"
		} else if cl, ok := hdr["content-length"]; ok {
			how = "Content-Length = " + cl
		}
		fmt.Printf("[client] Định khung thân: %s\n", how)
		fmt.Printf("[client] Thân (%d byte): %s\n", len(data), data)
	}
}
