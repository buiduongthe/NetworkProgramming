package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// Server tra cứu và nhận tệp có kiểm tra mã băm SHA-256 – Mã giả 3.9 (Mục 3.5.2).
// HANDLE_PUT được gắn vào HANDLE_LOOKUP của Mã giả 3.7 bằng nhánh
// IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE.
// Ba biện pháp an toàn: BASENAME chặn "../"; giới hạn MAX_FILE; ghi tệp tạm .part, chỉ đổi tên khi mã băm khớp.

// MaxFile – CONST MAX_FILE ← 100 * 1048576.
const MaxFile = 100 * 1048576

// UploadDir – thư mục lưu tệp tải lên.
const UploadDir = "uploads"

// Glossary – từ điển thuật ngữ (Mã giả 3.7); Keys giữ thứ tự trong tệp.
type Glossary struct {
	Text map[string]string
	Keys []string
}

// LoadGlossary – DICT ← LOAD_JSON_FILE("glossary.json"), giữ thứ tự khóa.
func LoadGlossary(path string) (*Glossary, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	dec := json.NewDecoder(f)
	if _, err := dec.Token(); err != nil {
		return nil, err
	}
	g := &Glossary{Text: map[string]string{}}
	for dec.More() {
		tok, err := dec.Token()
		if err != nil {
			return nil, err
		}
		key, _ := tok.(string)
		var text string
		if err := dec.Decode(&text); err != nil {
			return nil, err
		}
		g.Text[key] = text
		g.Keys = append(g.Keys, key)
	}
	return g, nil
}

var dict *Glossary

// ProcessRequest – Mã giả 3.7: nghiệp vụ tra cứu, độc lập với socket.
func ProcessRequest(req map[string]any) map[string]any {
	op, _ := GetString(req, "op")
	switch op {
	case "lookup":
		term, ok := GetString(req, "term")
		if text, found := dict.Text[term]; ok && found {
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": dict.Keys}
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"}
}

// Basename – BASENAME(REPLACE(name, "\\", "/")): bỏ mọi thành phần thư mục,
// tách theo cả '/' và '\' để an toàn trên mọi hệ điều hành.
func Basename(name string) string {
	parts := strings.Split(strings.ReplaceAll(name, `\`, "/"), "/")
	return parts[len(parts)-1]
}

// isInteger – IS_INTEGER: số JSON (float64) không có phần lẻ.
func isInteger(v any) (int64, bool) {
	f, ok := v.(float64)
	if !ok || f != math.Trunc(f) || math.Abs(f) > 1<<53 {
		return 0, false
	}
	return int64(f), true
}

// isHex – IS_HEX(s, n): chuỗi đúng n chữ số thập lục phân.
func isHex(v any, n int) bool {
	s, ok := v.(string)
	if !ok || len(s) != n {
		return false
	}
	_, err := hex.DecodeString(s)
	return err == nil
}

// HandlePut – Mã giả 3.9 HANDLE_PUT(conn, req), req = {op, name, size, sha256}.
// Trả về chuỗi mô tả kết quả để ghi nhật ký.
func HandlePut(conn net.Conn, req map[string]any) (string, error) {
	rawName, _ := GetString(req, "name")
	name := Basename(rawName) // bỏ thư mục: chặn "../"
	size, okSize := isInteger(req["size"])
	if name == "" || name == "." || name == ".." || !okSize || size < 0 || size > MaxFile || !isHex(req["sha256"], 64) {
		return "400 (Sai tham số)", SendJSON(conn, map[string]any{"status": 400, "error": "Sai tham số"})
	}
	if err := SendJSON(conn, map[string]any{"status": 100}); err != nil { // sẵn sàng nhận nội dung
		return "", err
	}
	tmp := filepath.Join(UploadDir, name+".part")
	f, err := os.Create(tmp) // OPEN_FILE(tmp, WRITE)
	if err != nil {
		return "", err
	}
	h := sha256.New() // SHA256_INIT()
	remaining := size
	buf := make([]byte, 65536)
	for remaining > 0 { // WHILE remaining > 0
		k, err := conn.Read(buf[:min(int64(len(buf)), remaining)]) // RECV(conn, MIN(65536, remaining))
		if k > 0 {
			f.Write(buf[:k]) // WRITE(f, chunk)
			h.Write(buf[:k]) // SHA256_UPDATE(h, chunk)
			remaining -= int64(k)
		}
		if err != nil && remaining > 0 { // CATCH ANY: mọi lỗi (kể cả RST) → xóa tệp dở dang
			f.Close()
			os.Remove(tmp)
			if errors.Is(err, io.EOF) { // LENGTH(chunk) = 0
				return "", fmt.Errorf("Tệp chưa nhận đủ (%d/%d byte)", size-remaining, size)
			}
			return "", err
		}
	}
	f.Close() // CLOSE_FILE(f)
	digest, _ := GetString(req, "sha256")
	if hex.EncodeToString(h.Sum(nil)) == digest { // HEX(SHA256_FINAL(h)) = req.sha256
		if err := os.Rename(tmp, filepath.Join(UploadDir, name)); err != nil { // RENAME (ghi đè nếu đã có)
			return "", err
		}
		return fmt.Sprintf("200, lưu %s/%s (%d byte)", UploadDir, name, size),
			SendJSON(conn, map[string]any{"status": 200, "saved": name})
	}
	os.Remove(tmp) // DELETE(tmp)
	return "422 (Mã băm không khớp)", SendJSON(conn, map[string]any{"status": 422, "error": "Mã băm không khớp"})
}

// HandleLookup – Mã giả 3.7 HANDLE_LOOKUP, bổ sung nhánh "put" theo Mã giả 3.9.
func HandleLookup(conn net.Conn, addr string) error {
	for {
		req, err := RecvJSON(conn)
		var cc *ConnectionClosed
		var je *JsonError
		switch {
		case errors.As(err, &cc) && cc.Received == 0:
			return nil // đóng tại ranh giới
		case errors.As(err, &je): // JSON sai hoặc không phải đối tượng
			if err := SendJSON(conn, map[string]any{"status": 400, "error": "JSON sai"}); err != nil {
				return err
			}
			continue
		case err != nil:
			return err
		}
		op, _ := GetString(req, "op")
		if op == "bye" {
			return nil
		}
		if op == "put" { // IF req.op = "put" THEN HANDLE_PUT(conn, req); CONTINUE
			result, err := HandlePut(conn, req)
			if err != nil {
				return err
			}
			name, _ := GetString(req, "name")
			fmt.Printf("[server] %s: put '%s' -> %s\n", addr, name, result)
			continue
		}
		resp := ProcessRequest(req)
		if err := SendJSON(conn, resp); err != nil {
			return err
		}
		if op == "" {
			op = "?"
		}
		term := ""
		if op == "lookup" {
			t, _ := GetString(req, "term")
			term = " '" + t + "'"
		}
		fmt.Printf("[server] %s: %s%s -> %v\n", addr, op, term, resp["status"])
	}
}

// RunPutServer – khung sườn ECHO_SERVER (Mã giả 3.1), cổng mặc định 9100.
func RunPutServer(args []string) int {
	var err error
	if dict, err = LoadGlossary("glossary.json"); err != nil {
		fmt.Printf("[server] Không đọc được glossary.json: %v\n", err)
		return 1
	}
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	if err := os.MkdirAll(UploadDir, 0o755); err != nil {
		fmt.Printf("[server] Không tạo được thư mục %s: %v\n", UploadDir, err)
		return 1
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port))) // SOCKET, REUSEADDR, BIND, LISTEN
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d, thư mục tải lên: %s/\n", host, port, UploadDir)
	StopOnCtrlC(srv)
	for {
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			break
		}
		addr := conn.RemoteAddr().String()
		fmt.Printf("[server] Client kết nối: %s\n", addr)
		if err := HandleLookup(conn, addr); err != nil {
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err) // không dừng server
		}
		conn.Close()
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}
	fmt.Println("[server] Dừng server")
	return 0
}
