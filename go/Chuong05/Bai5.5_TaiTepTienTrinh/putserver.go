package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"math"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// Server nhận tệp có kiểm tra mã băm (đi kèm để chạy thử PUT_FILE_ASYNC).
// Cài đặt Mã giả 3.9 (HANDLE_PUT, Mục 3.5.2, chép từ Bài 3.4) trên nền khung JSON của Mã giả 5.1:
// ghi vào tệp tạm .part, chỉ đổi tên khi mã băm SHA-256 khớp; kết nối đóng giữa chừng thì xóa tệp .part.
// Tham số rate_kib giới hạn tốc độ đọc (KiB/s) để mô phỏng đường truyền chậm; 0 = không giới hạn.

const (
	MaxFile   = 100 * 1048576
	UploadDir = "uploads"
)

var rateKib = 0

// Basename – BASENAME(REPLACE(name, "\\", "/")).
func Basename(name string) string {
	parts := strings.Split(strings.ReplaceAll(name, `\`, "/"), "/")
	return parts[len(parts)-1]
}

func isInteger(v any) (int64, bool) {
	f, ok := v.(float64)
	if !ok || f != math.Trunc(f) || math.Abs(f) > 1<<53 {
		return 0, false
	}
	return int64(f), true
}

func isHex(v any, n int) bool {
	s, ok := v.(string)
	if !ok || len(s) != n {
		return false
	}
	_, err := hex.DecodeString(s)
	return err == nil
}

// handlePut – Mã giả 3.9 HANDLE_PUT.
func handlePut(ctx context.Context, conn net.Conn, req map[string]any) error {
	rawName, _ := GetString(req, "name")
	name := Basename(rawName) // bỏ thư mục: chặn "../"
	size, ok := isInteger(req["size"])
	if name == "" || name == "." || name == ".." || !ok || size < 0 || size > MaxFile || !isHex(req["sha256"], 64) {
		return SendJSONAsync(ctx, conn, map[string]any{"status": 400, "error": "Sai tham số"})
	}
	fmt.Printf("[server] Nhận yêu cầu put: %s, %d byte\n", name, size)
	if err := SendJSONAsync(ctx, conn, map[string]any{"status": 100}); err != nil { // sẵn sàng nhận nội dung
		return err
	}
	if err := os.MkdirAll(UploadDir, 0o755); err != nil {
		return err
	}
	tmp := filepath.Join(UploadDir, name+".part")
	f, err := os.Create(tmp)
	if err != nil {
		return err
	}
	h := sha256.New()
	remaining := size
	buf := make([]byte, 65536)
	for remaining > 0 {
		n, err := conn.Read(buf[:min(int64(len(buf)), remaining)])
		if n > 0 {
			f.Write(buf[:n])
			h.Write(buf[:n])
			remaining -= int64(n)
			if rateKib > 0 { // mô phỏng đường truyền chậm
				time.Sleep(time.Duration(float64(n) / float64(rateKib*1024) * float64(time.Second)))
			}
		}
		if err != nil && remaining > 0 { // mọi lỗi khi nhận: xóa tệp dở dang
			f.Close()
			os.Remove(tmp)
			fmt.Printf("[server] Kết nối đóng khi mới nhận %d/%d byte: xóa %s\n", size-remaining, size, tmp)
			if errors.Is(err, io.EOF) {
				return &ConnectionClosed{Received: int(size - remaining)}
			}
			return err
		}
	}
	f.Close()
	digest, _ := GetString(req, "sha256")
	if hex.EncodeToString(h.Sum(nil)) == digest {
		if err := os.Rename(tmp, filepath.Join(UploadDir, name)); err != nil {
			return err
		}
		fmt.Printf("[server] Đã lưu %s/%s, mã băm khớp (200)\n", UploadDir, name)
		return SendJSONAsync(ctx, conn, map[string]any{"status": 200, "saved": name})
	}
	os.Remove(tmp)
	fmt.Println("[server] Mã băm không khớp (422)")
	return SendJSONAsync(ctx, conn, map[string]any{"status": 422, "error": "Mã băm không khớp"})
}

// session – vòng phục vụ một client (một goroutine cho mỗi kết nối).
func session(conn net.Conn) {
	ctx := context.Background()
	addr := conn.RemoteAddr().String()
	fmt.Printf("[server] Client kết nối: %s\n", addr)
	defer func() {
		conn.Close()
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}()
	for {
		req, err := RecvJSONAsync(ctx, conn)
		var cc *ConnectionClosed
		if errors.As(err, &cc) && cc.Received == 0 {
			return
		}
		if err == nil {
			switch op, _ := GetString(req, "op"); op {
			case "put":
				err = handlePut(ctx, conn, req)
			case "bye":
				return
			default:
				err = SendJSONAsync(ctx, conn, map[string]any{"status": 400, "error": "Sai thao tác"})
			}
		}
		if err != nil {
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err)
			return
		}
	}
}

// RunPutServer – server [host] [port] [rate_kib].
func RunPutServer(args []string) int {
	host, port, err := ParseEndpoint(args[:min(len(args), 2)], "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	if len(args) > 2 {
		if rateKib, err = strconv.Atoi(args[2]); err != nil || rateKib < 0 {
			fmt.Printf("Tốc độ không hợp lệ: %s\n", args[2])
			return 2
		}
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d, giới hạn đọc %d KiB/s\n", host, port, rateKib)
	StopOnCtrlC(srv)
	for {
		conn, err := srv.Accept()
		if err != nil {
			break
		}
		if rateKib > 0 { // bộ đệm nhận nhỏ: mô phỏng đường truyền chậm
			conn.(*net.TCPConn).SetReadBuffer(65536)
		}
		go session(conn)
	}
	fmt.Println("[server] Dừng server")
	return 0
}
