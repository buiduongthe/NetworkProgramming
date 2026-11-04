package main

import (
	"errors"
	"fmt"
	"net"
	"strconv"
)

// RunFrameServer – Echo Server dùng khung JSON (tiền tố độ dài 4 byte).
// Khung sườn ECHO_SERVER của Mã giả 3.1 (Mục 3.2.2), thay HANDLE_ECHO bằng HandleFrames
// dùng RECV_FRAME/SEND_JSON của Mã giả 3.6 (Mục 3.4.3). Mỗi yêu cầu {"text": ...} được
// trả lời bằng {"status": 200, "text": ...}; yêu cầu sai nhận {"status": 400, "error": ...}.
func RunFrameServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9000)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port))) // SOCKET, REUSEADDR, BIND, LISTEN
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	StopOnCtrlC(srv)
	for { // vòng chấp nhận kết nối
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			break
		}
		addr := conn.RemoteAddr().String()
		fmt.Printf("[server] Client kết nối: %s\n", addr)
		if err := HandleFrames(conn, addr); err != nil { // NetError, ConnectionClosed, ProtocolError
			fmt.Printf("[server] Lỗi với %s: %v\n", addr, err) // không dừng server
		}
		conn.Close() // CLOSE(conn)
		fmt.Printf("[server] Client %s đã đóng kết nối\n", addr)
	}
	fmt.Println("[server] Dừng server")
	return 0
}

// HandleFrames – vòng phục vụ một client: mỗi khung nhận được là một thông điệp hoàn chỉnh.
func HandleFrames(conn net.Conn, addr string) error {
	for {
		payload, err := RecvFrame(conn) // RECV_JSON tách hai bước để ghi kích thước khung
		var cc *ConnectionClosed
		if errors.As(err, &cc) && cc.Received == 0 {
			return nil // đóng tại ranh giới khung: kết thúc bình thường
		}
		if err != nil {
			return err // đóng giữa khung hoặc lỗi khác
		}
		fmt.Printf("[server] Nhận khung %d byte từ %s\n", len(payload), addr)
		req, err := JsonDecode(payload)
		if err != nil {
			err = SendJSON(conn, map[string]any{"status": 400, "error": "JSON sai"})
		} else if text, ok := GetString(req, "text"); ok {
			err = SendJSON(conn, map[string]any{"status": 200, "text": text})
		} else {
			err = SendJSON(conn, map[string]any{"status": 400, "error": "Thiếu trường text"})
		}
		if err != nil {
			return err
		}
	}
}
