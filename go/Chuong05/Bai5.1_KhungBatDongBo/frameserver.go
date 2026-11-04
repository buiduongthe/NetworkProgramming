package main

import (
	"context"
	"errors"
	"fmt"
	"net"
	"strconv"
)

// Server tra cứu "bất đồng bộ" dùng khung JSON (minh họa Mã giả 5.1).
// Server dùng lại nghiệp vụ PROCESS_REQUEST của Mã giả 3.7 (dịch vụ tra cứu, cổng mặc định 9100),
// phần truyền nhận dùng RecvJSONAsync, SendJSONAsync. Mỗi kết nối là một goroutine nên server
// phục vụ được nhiều client cùng lúc.

// Từ điển nhỏ thay cho glossary.json; dictKeys giữ thứ tự cho thao tác "list".
var dict = map[string]string{
	"TCP":         "Giao thức truyền tin cậy, hướng kết nối",
	"UDP":         "Giao thức datagram, không kết nối",
	"Socket":      "Điểm cuối truyền thông của một chương trình",
	"Bất đồng bộ": "Khởi động vào/ra rồi làm việc khác, quay lại khi xong",
}
var dictKeys = []string{"TCP", "UDP", "Socket", "Bất đồng bộ"}

// ProcessRequest – Mã giả 3.7: nghiệp vụ, độc lập với socket.
func ProcessRequest(req map[string]any) map[string]any {
	op, _ := GetString(req, "op")
	switch op {
	case "lookup":
		term, ok := GetString(req, "term")
		if text, found := dict[term]; ok && found {
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": dictKeys}
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"}
}

// HandleLookup – HANDLE_LOOKUP, phiên bản bất đồng bộ (chạy trong goroutine riêng cho mỗi kết nối).
func HandleLookup(ctx context.Context, conn net.Conn) {
	addr := conn.RemoteAddr().String()
	fmt.Printf("[server] Client kết nối: %s\n", addr)
	defer func() {
		conn.Close()
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}()
	for {
		req, err := RecvJSONAsync(ctx, conn) // AWAIT: goroutine chờ khung, các goroutine khác vẫn chạy
		var cc *ConnectionClosed
		var je *JsonError
		switch {
		case errors.As(err, &cc) && cc.Received == 0:
			return // đóng tại ranh giới khung: bình thường
		case errors.As(err, &je):
			if err := SendJSONAsync(ctx, conn, map[string]any{"status": 400, "error": "JSON sai"}); err != nil {
				fmt.Printf("[server] Lỗi với %s: %v\n", addr, err)
				return
			}
			continue
		case err != nil: // đóng giữa khung, khung quá lớn, lỗi mạng: lỗi một client không dừng server
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err)
			return
		}
		fmt.Printf("[server] Nhận từ %s: %s\n", addr, JsonBytes(req))
		if op, _ := GetString(req, "op"); op == "bye" {
			return
		}
		if err := SendJSONAsync(ctx, conn, ProcessRequest(req)); err != nil {
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err)
			return
		}
	}
}

// RunFrameServer – vòng ACCEPT; mỗi kết nối mới: một goroutine HandleLookup (RUN_BACKGROUND).
func RunFrameServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9100)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port))) // BIND + LISTEN
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	StopOnCtrlC(srv)
	ctx := context.Background()
	for {
		conn, err := srv.Accept()
		if err != nil {
			break
		}
		go HandleLookup(ctx, conn)
	}
	fmt.Println("[server] Dừng server")
	return 0
}
