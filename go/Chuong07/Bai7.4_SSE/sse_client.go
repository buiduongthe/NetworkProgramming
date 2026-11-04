// Bài 7.4 – Bộ đọc luồng Server-Sent Events trên socket TCP.
//
// Cài đặt Mã giả 7.5 (SSE_READ, Mục 7.3.3, Giáo trình Lập trình mạng).
// Kế thừa: Mã giả 7.1 (gửi GET, đọc dòng trạng thái và tiêu đề, giải mã chunked), 3.5 (READ_LINE).
// Khi phản hồi dùng chunked, bộ đọc dòng được đặt phía sau lớp giải mã khối, để các dòng độ dài khối
// không lẫn vào luồng sự kiện.
//
// Chạy:  out/app client [host] [port] [đường_dẫn]   (mặc định 127.0.0.1 8080 /events)
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

// Reader – Mã giả 3.5, Reader { sock, pending }; sock là bất kỳ nguồn nào có Read (io.Reader).
type Reader struct {
	sock    io.Reader
	pending []byte
}

// readLine – Mã giả 3.5, READ_LINE: io.EOF (NULL) khi nguồn đã hết.
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
		n, err := r.sock.Read(buf)
		r.pending = append(r.pending, buf[:n]...)
		if n == 0 && err != nil {
			return "", err
		}
	}
}

// readExact – READ_EXACT: lấy phần dư pending trước, thiếu thì đọc tiếp từ nguồn.
func readExact(r *Reader, n int) ([]byte, error) {
	k := min(n, len(r.pending))
	data := append([]byte(nil), r.pending[:k]...)
	r.pending = r.pending[k:]
	if len(data) < n {
		rest := make([]byte, n-len(data))
		if _, err := io.ReadFull(r.sock, rest); err != nil {
			return nil, errors.New("Kết nối đóng giữa chừng")
		}
		data = append(data, rest...)
	}
	return data, nil
}

// readHeaders – READ_HEADERS (Mã giả 7.1): đến dòng trống; NULL giữa chừng → lỗi ConnectionClosed.
func readHeaders(r *Reader) (map[string]string, error) {
	hdr := map[string]string{}
	for {
		line, err := readLine(r, 8192)
		if err == io.EOF {
			return nil, errors.New("Kết nối đóng giữa phần tiêu đề")
		}
		if err != nil {
			return nil, err
		}
		if line == "" {
			return hdr, nil
		}
		k, v, _ := strings.Cut(line, ":")
		hdr[strings.ToLower(strings.TrimSpace(k))] = strings.TrimSpace(v)
	}
}

// ChunkedSource – lớp giải mã khối (READ_CHUNKED của Mã giả 7.1 viết dạng luồng):
// Read trả dữ liệu thân thuần, nên có thể đặt một Reader mới phía sau.
type ChunkedSource struct {
	r    *Reader
	left int
	done bool
}

func (c *ChunkedSource) Read(p []byte) (int, error) {
	if c.done {
		return 0, io.EOF
	}
	if c.left == 0 {
		line, err := readLine(c.r, 1024)
		if err != nil {
			return 0, err
		}
		sizeText, _, _ := strings.Cut(line, ";")
		size, err := strconv.ParseInt(strings.TrimSpace(sizeText), 16, 64)
		if err != nil {
			return 0, errors.New("Độ dài khối không hợp lệ")
		}
		if size == 0 { // khối 0: hết thân, bỏ trailer
			c.done = true
			if _, err := readHeaders(c.r); err != nil {
				return 0, err
			}
			return 0, io.EOF
		}
		c.left = int(size)
	}
	data, err := readExact(c.r, min(len(p), c.left))
	if err != nil {
		return 0, err
	}
	c.left -= len(data)
	if c.left == 0 {
		readLine(c.r, 2) // CRLF sau mỗi khối
	}
	return copy(p, data), nil
}

// sseState: last_event_id và reconnect_ms của Mã giả 7.5 (dùng khi kết nối lại).
type sseState struct {
	lastEventID string
	reconnectMs int
}

// sseRead – Mã giả 7.5: r là Reader đặt sau phần tiêu đề.
func sseRead(r *Reader, onEvent func(string, string), st *sseState) {
	evType, data := "message", []string{}
	for {
		line, err := readLine(r, 65536)
		if err != nil {
			return // NULL: server đóng luồng
		}
		if line == "" { // dòng trống: hết một sự kiện
			if len(data) > 0 {
				onEvent(evType, strings.Join(data, "\n"))
			}
			evType, data = "message", []string{}
		} else if !strings.HasPrefix(line, ":") { // ":" mở đầu dòng chú thích
			field, value, _ := strings.Cut(line, ":")
			value = strings.TrimPrefix(value, " ") // STRIP_ONE_SPACE
			switch field {
			case "event":
				evType = value
			case "data":
				data = append(data, value)
			case "id":
				st.lastEventID = value
			case "retry":
				st.reconnectMs, _ = strconv.Atoi(value)
			}
		}
	}
}

func clientMain(args []string) {
	host := "127.0.0.1"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	path := "/events"
	if len(args) >= 3 {
		path = args[2]
	}
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second) // SAFE_CONNECT
	if err != nil {
		fail("[client] Không kết nối được: " + err.Error())
	}
	defer s.Close()
	s.SetReadDeadline(time.Now().Add(30 * time.Second))
	s.Write([]byte("GET " + path + " HTTP/1.1\r\nHost: " + host + ":" + strconv.Itoa(port) + "\r\nAccept: text/event-stream\r\n" +
		"Cache-Control: no-cache\r\nConnection: close\r\n\r\n"))
	r := &Reader{sock: s}
	status, _ := readLine(r, 8192)
	hdr, err := readHeaders(r)
	if err != nil {
		fail("[client] " + err.Error())
	}
	fmt.Printf("[client] %s; Content-Type: %s\n", status, hdr["content-type"])
	if parts := strings.Split(status, " "); len(parts) < 2 || parts[1] != "200" {
		return
	}
	if strings.HasSuffix(strings.ToLower(hdr["transfer-encoding"]), "chunked") {
		fmt.Println("[client] Thân dùng chunked: đặt bộ đọc dòng sau lớp giải mã khối")
		r = &Reader{sock: &ChunkedSource{r: r}}
	}
	st := &sseState{}
	sseRead(r, func(t, d string) { // in '\n' dạng hai ký tự để thấy nhiều dòng data
		fmt.Printf("[client] Sự kiện '%s': '%s'\n", t, strings.ReplaceAll(d, "\n", "\\n"))
	}, st)
	fmt.Printf("[client] Server đóng luồng; id cuối = %s, retry = %d ms\n", st.lastEventID, st.reconnectMs)
}
