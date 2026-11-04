// Bài 7.2 – Server REST tối giản trên HTTP/1.1.
//
// Cài đặt Mã giả 7.2 (Mục 7.2.3, Giáo trình Lập trình mạng): HTTP_SESSION, ROUTE, RESPOND.
// Kế thừa: Mã giả 6.3 (mỗi kết nối một tác vụ – goroutine), 7.1 (READ_HEADERS), 3.7 (DICT nạp từ glossary.json).
// Bổ sung theo bài tập cuối chương: POST /api/v1/terms (201 + Location) và DELETE /api/v1/terms/{term} (204).
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 8080)
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"sync"
)

const MAX_BODY = 1048576 // 1 MiB, như MAX_FRAME của Mã giả 3.6

var REASON = map[int]string{200: "OK", 201: "Created", 204: "No Content", 400: "Bad Request", 404: "Not Found",
	405: "Method Not Allowed", 409: "Conflict", 413: "Content Too Large", 501: "Not Implemented"}

// Từ điển DICT: map tra cứu + danh sách khóa giữ thứ tự; các goroutine phiên chạy song song nên cần khóa.
var (
	dictMu sync.Mutex
	DICT   = map[string]string{}
	KEYS   []string
)

func loadGlossary() {
	path := "glossary.json"
	if _, err := os.Stat(path); err != nil {
		exe, _ := os.Executable()
		path = filepath.Join(filepath.Dir(exe), "..", "glossary.json") // out/app → thư mục bài
	}
	data, err := os.ReadFile(path)
	if err != nil {
		fail("Không đọc được glossary.json: " + err.Error())
	}
	dec := json.NewDecoder(bytes.NewReader(data)) // đọc từng cặp khóa–giá trị để giữ thứ tự
	if t, err := dec.Token(); err != nil || t != json.Delim('{') {
		fail("glossary.json phải là một đối tượng JSON")
	}
	for dec.More() {
		k, _ := dec.Token()
		var v string
		if err := dec.Decode(&v); err != nil {
			fail("glossary.json sai định dạng: " + err.Error())
		}
		DICT[k.(string)] = v
		KEYS = append(KEYS, k.(string))
	}
}

// Problem – PROBLEM(status, title) theo RFC 9457.
type Problem struct {
	Type   string `json:"type"`
	Title  string `json:"title"`
	Status int    `json:"status"`
}

func problem(status int, title string) Problem {
	return Problem{"about:blank", title, status}
}

type Term struct {
	Term string `json:"term"`
	Text string `json:"text"`
}

// jsonEncode – JSON_ENCODE: UTF-8, không thoát ký tự <, >, & như json.Marshal mặc định.
func jsonEncode(obj any) []byte {
	var b bytes.Buffer
	enc := json.NewEncoder(&b)
	enc.SetEscapeHTML(false)
	enc.Encode(obj)
	return bytes.TrimRight(b.Bytes(), "\n")
}

// readLineAsync – READ_LINE_ASYNC: bufio.Reader đóng vai Reader (có bộ đệm pending riêng).
// Trả về io.EOF (NULL) khi đối tác đã đóng mà chưa gửi byte nào của dòng.
func readLineAsync(r *bufio.Reader, maxLen int) (string, error) {
	line, err := r.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) || len(line) > maxLen {
		return "", errors.New("Dòng quá dài")
	}
	if err != nil && len(line) == 0 {
		return "", err
	}
	return strings.TrimRight(string(line), "\r\n"), nil
}

// readHeadersAsync – Mã giả 7.1, READ_HEADERS (NULL giữa chừng → lỗi).
func readHeadersAsync(r *bufio.Reader) (map[string]string, error) {
	hdr := map[string]string{}
	for {
		line, err := readLineAsync(r, 8192)
		if err == io.EOF { // NULL: RAISE ConnectionClosed() – đóng giữa phần tiêu đề
			return nil, errors.New("Kết nối đóng giữa phần tiêu đề")
		}
		if err != nil {
			return nil, err
		}
		if line == "" { // dòng trống: hết
			return hdr, nil
		}
		k, v, _ := strings.Cut(line, ":")
		hdr[strings.ToLower(strings.TrimSpace(k))] = strings.TrimSpace(v)
	}
}

// respond – Mã giả 7.2, dòng 38–45: Content-Length là số byte sau khi mã hóa UTF-8.
func respond(w io.Writer, status int, obj any, extra [][2]string) error {
	h := fmt.Sprintf("HTTP/1.1 %d %s\r\n", status, REASON[status])
	for _, kv := range extra {
		h += kv[0] + ": " + kv[1] + "\r\n"
	}
	var out []byte
	if status == 204 { // 204: không có thân, không có Content-Length
		out = []byte(h + "\r\n")
	} else {
		b := jsonEncode(obj)
		ct := "application/json"
		if status >= 400 {
			ct = "application/problem+json"
		}
		h += "Content-Type: " + ct + "; charset=utf-8\r\n"
		h += "Content-Length: " + strconv.Itoa(len(b)) + "\r\n\r\n"
		out = append([]byte(h), b...)
	}
	_, err := w.Write(out) // SEND_ALL_ASYNC
	return err
}

// route – Mã giả 7.2, dòng 24–36 (thêm POST, DELETE).
func route(method, path string, body []byte) (int, any, [][2]string) {
	p := strings.Split(strings.Trim(path, "/"), "/") // ["api","v1","terms",…]
	if len(p) < 3 || !slices.Equal(p[:3], []string{"api", "v1", "terms"}) || len(p) > 4 {
		return 404, problem(404, "Không có tài nguyên"), nil
	}
	dictMu.Lock()
	defer dictMu.Unlock()
	if method == "GET" && len(p) == 3 {
		return 200, struct {
			Terms []string `json:"terms"`
		}{slices.Clone(KEYS)}, nil
	}
	if method == "GET" && len(p) == 4 {
		t, _ := url.PathUnescape(p[3]) // URL_DECODE: %20 -> dấu cách
		if text, ok := DICT[t]; ok {
			return 200, Term{t, text}, nil
		}
		return 404, problem(404, "Không có thuật ngữ"), nil
	}
	if method == "POST" && len(p) == 3 { // mở rộng: thêm thuật ngữ
		var obj map[string]any
		json.Unmarshal(body, &obj)
		t, ok1 := obj["term"].(string)
		text, ok2 := obj["text"].(string)
		if !ok1 || !ok2 {
			return 400, problem(400, "JSON sai hoặc thiếu trường term, text"), nil
		}
		if _, exists := DICT[t]; exists {
			return 409, problem(409, "Thuật ngữ đã tồn tại"), nil
		}
		DICT[t] = text
		KEYS = append(KEYS, t)
		return 201, Term{t, text}, [][2]string{{"Location", "/api/v1/terms/" + url.PathEscape(t)}}
	}
	if method == "DELETE" && len(p) == 4 { // mở rộng: xóa thuật ngữ
		t, _ := url.PathUnescape(p[3])
		if _, exists := DICT[t]; !exists {
			return 404, problem(404, "Không có thuật ngữ"), nil
		}
		delete(DICT, t)
		KEYS = slices.DeleteFunc(KEYS, func(k string) bool { return k == t })
		return 204, nil, nil
	}
	return 405, problem(405, "Phương thức không hỗ trợ"), nil
}

// httpSession – Mã giả 7.2, dòng 1–21: phiên keep-alive, nhiều yêu cầu trên một kết nối.
func httpSession(conn net.Conn) {
	addr := conn.RemoteAddr().String()
	defer func() {
		conn.Close()
		fmt.Printf("[server] Đóng kết nối %s\n", addr)
	}()
	r := bufio.NewReaderSize(conn, 65536) // Reader{sock: conn}
	err := func() error {
		for { // keep-alive: nhiều yêu cầu
			line, err := readLineAsync(r, 8192)
			if err == io.EOF {
				return nil // client đóng kết nối
			}
			if err != nil {
				return err
			}
			parts := strings.Split(line, " ")
			if len(parts) != 3 {
				return respond(conn, 400, problem(400, "Dòng yêu cầu sai"), nil)
			}
			method, target := parts[0], parts[1] // (method, target, version)
			hdr, err := readHeadersAsync(r)
			if err != nil {
				return err
			}
			var n int64 // TO_INT64: Content-Length có thể > 4 GB
			if cl, ok := hdr["content-length"]; ok {
				if n, err = strconv.ParseInt(cl, 10, 64); err != nil {
					return errors.New("Content-Length không hợp lệ: " + cl)
				}
			}
			if _, ok := hdr["transfer-encoding"]; ok { // chưa hỗ trợ chunked
				fmt.Printf("[server] %s %s %s -> 501\n", addr, method, target)
				return respond(conn, 501, problem(501, "Chưa hỗ trợ"), nil) // độ dài thân không xác định: đóng
			}
			if n < 0 || n > MAX_BODY {
				fmt.Printf("[server] %s %s %s -> 413\n", addr, method, target)
				return respond(conn, 413, problem(413, "Thân quá lớn"), nil)
			}
			body := make([]byte, n)
			if _, err := io.ReadFull(r, body); err != nil { // READ_EXACT_ASYNC(r, n)
				return err
			}
			path, _, _ := strings.Cut(target, "?") // PATH(target): bỏ phần truy vấn
			status, obj, extra := route(method, path, body)
			fmt.Printf("[server] %s %s %s -> %d\n", addr, method, target, status)
			if err := respond(conn, status, obj, extra); err != nil {
				return err
			}
			if strings.ToLower(hdr["connection"]) == "close" {
				return nil
			}
		}
	}()
	if err != nil {
		fmt.Printf("[server] Kết thúc phiên %s: %v\n", addr, err)
	}
}

func serverMain(args []string) {
	host := "0.0.0.0"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	for {
		conn, err := srv.Accept()
		if err != nil {
			continue
		}
		go httpSession(conn) // mỗi kết nối một tác vụ
	}
}
