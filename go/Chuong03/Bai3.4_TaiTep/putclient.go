package main

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Client tra cứu và tải tệp lên server – Mã giả 3.10 (PUT_FILE, Mục 3.5.2) trên nền client tra cứu
// của Mã giả 3.8: cùng một kết nối dùng cho cả tra cứu và tải tệp. Mỗi dòng nhập là một lệnh:
//
//	put <đường dẫn tệp>     – tải tệp lên (đọc và gửi từng khối 64 KiB)
//	<thuật ngữ>             – tra cứu thuật ngữ

// sha256OfFile – SHA256_OF_FILE(path): đọc từng khối, không nạp cả tệp vào bộ nhớ.
func sha256OfFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

// recvJSONTimeout – RECV_JSON với RECV_TIMEOUT 5000 ms.
func recvJSONTimeout(conn net.Conn) (map[string]any, error) {
	conn.SetReadDeadline(time.Now().Add(5000 * time.Millisecond))
	return RecvJSON(conn)
}

// PutFile – Mã giả 3.10: khung tiêu đề -> chờ 100 -> gửi đúng size byte -> nhận kết quả.
func PutFile(conn net.Conn, path string) error {
	digest, err := sha256OfFile(path) // HEX(SHA256_OF_FILE(path))
	if err != nil {
		return err
	}
	info, err := os.Stat(path) // FILE_SIZE(path)
	if err != nil {
		return err
	}
	name, size := filepath.Base(path), info.Size()
	fmt.Printf("[client] Tải lên %s: %d byte, SHA-256 %s…\n", name, size, digest[:16])
	if err := SendJSON(conn, map[string]any{"op": "put", "name": name, "size": size, "sha256": digest}); err != nil {
		return err
	}
	resp, err := recvJSONTimeout(conn)
	if err != nil {
		return err
	}
	if GetInt(resp, "status") != 100 {
		e, _ := GetString(resp, "error")
		return &ProtocolError{e} // RAISE ProtocolError(resp.error)
	}
	f, err := os.Open(path) // OPEN_FILE(path, READ)
	if err != nil {
		return err
	}
	defer f.Close() // CLOSE_FILE(f)
	buf := make([]byte, 65536)
	for {
		k, err := f.Read(buf) // READ(f, 65536)
		if k > 0 {
			if err := SendAll(conn, buf[:k]); err != nil { // SEND_ALL(s, chunk)
				return err
			}
		}
		if errors.Is(err, io.EOF) { // EMPTY_BYTES: hết tệp
			break
		}
		if err != nil {
			return err
		}
	}
	resp, err = recvJSONTimeout(conn)
	if err != nil {
		return err
	}
	text, _ := JsonEncode(resp)
	fmt.Printf("[client] Kết quả: %d %s\n", GetInt(resp, "status"), text) // PRINT("Kết quả:", resp.status, resp)
	return nil
}

// RunPutClient – client tra cứu (Mã giả 3.8) có thêm lệnh put.
func RunPutClient(args []string) int {
	host, port, err := ParseEndpoint(args, "127.0.0.1", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	conn, err := SafeConnect(host, port, 3000)
	if err != nil {
		fmt.Printf("[client] Không kết nối được %s:%d: %v\n", host, port, err)
		return 1
	}
	defer func() {
		conn.Close()
		fmt.Println("[client] Đã đóng kết nối")
	}()
	fmt.Printf("[client] Đã kết nối %s:%d. Nhập 'put <tệp>' hoặc thuật ngữ (Ctrl+D để kết thúc).\n", host, port)
	console := bufio.NewScanner(os.Stdin)
	for console.Scan() { // READ_CONSOLE()
		line := strings.TrimRight(console.Text(), "\r")
		if strings.HasPrefix(line, "put ") {
			path := strings.TrimSpace(line[4:])
			err := PutFile(conn, path)
			var pe *ProtocolError
			switch {
			case errors.Is(err, os.ErrNotExist):
				fmt.Printf("[client] Lỗi: không tìm thấy tệp '%s'\n", path)
			case errors.As(err, &pe): // server không trả 100
				fmt.Printf("[client] Lỗi: %v\n", pe)
			case err != nil:
				fmt.Printf("[client] Lỗi: %v\n", err)
				return 1
			}
			continue
		}
		if err := SendJSON(conn, map[string]any{"op": "lookup", "term": line}); err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		resp, err := recvJSONTimeout(conn)
		if err != nil {
			fmt.Printf("[client] Lỗi: %v\n", err)
			return 1
		}
		if GetInt(resp, "status") == 200 {
			t, _ := GetString(resp, "term")
			text, _ := GetString(resp, "text")
			fmt.Printf("[client] %s: %s\n", t, text)
		} else {
			e, _ := GetString(resp, "error")
			fmt.Printf("[client] Lỗi %d: %s ('%s')\n", GetInt(resp, "status"), e, line)
		}
	}
	SendJSON(conn, map[string]any{"op": "bye"})
	return 0
}
