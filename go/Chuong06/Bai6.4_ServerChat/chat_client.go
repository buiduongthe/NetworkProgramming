// Bài 6.4 – Client chat dòng lệnh tối giản dùng lớp ChatClient.
//
// ChatClient chép gọn từ Mã giả 5.5 (Mục 5.3.3): CONNECT gửi hello rồi chạy nền RECEIVE_LOOP,
// SEND dùng sendLock để mỗi lúc chỉ gửi một khung, DISCONNECT dừng vòng nhận rồi đóng socket.
// Lệnh (Mục 6.3.3): /join phòng, /leave phòng, /pm tên nội dung, /ping, /quit; dòng khác là tin msg
// gửi tới phòng hiện tại (phòng của lệnh /join gần nhất). Bộ hẹn giờ gửi ping mỗi 30 giây.
// Khi hết dữ liệu nhập (Ctrl+D), client chờ 1 giây để nhận nốt thông điệp rồi ngắt kết nối.
//
// Chạy:  out/app client [host] [port] [tên]      (mặc định 127.0.0.1 9200 user)
package main

import (
	"bufio"
	"errors"
	"fmt"
	"net"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// ChatClient – STRUCT ChatClient { sock, send_lock, on_message, on_status } (Mã giả 5.5).
type ChatClient struct {
	sock      net.Conn
	sendLock  sync.Mutex // mỗi lúc gửi một khung
	onMessage func(map[string]any)
	onStatus  func(string)
	stopping  atomic.Bool // đã gọi disconnect: vòng nhận dừng lặng lẽ
}

// connect – CONNECT(host, port, name).
func (c *ChatClient) connect(host string, port int, name string) error {
	s, err := safeConnect(host, port, 3000) // SAFE_CONNECT_ASYNC, 3000 ms
	if err != nil {
		return err
	}
	c.sock = s
	if err := c.send(map[string]any{"type": "hello", "name": name}); err != nil {
		s.Close() // không để rò socket
		return err
	}
	c.onStatus("connected") // báo trước khi vòng nhận chạy nền
	go c.receiveLoop()      // RUN_BACKGROUND(RECEIVE_LOOP)
	return nil
}

// receiveLoop – RECEIVE_LOOP: nhận và chuyển từng thông điệp cho on_message.
func (c *ChatClient) receiveLoop() {
	for {
		m, err := recvJSON(c.sock)
		if err != nil {
			if c.stopping.Load() {
				return // ngắt kết nối chủ động
			}
			c.sock.Close()
			reason := err.Error()
			var cc *ConnectionClosed
			if errors.As(err, &cc) && cc.Received == 0 {
				reason = "server đã đóng kết nối"
			}
			c.onStatus("lost: " + reason)
			return
		}
		c.onMessage(m)
	}
}

// send – SEND_MESSAGE tổng quát: gửi một đối tượng JSON, có khóa gửi.
func (c *ChatClient) send(obj map[string]any) error {
	c.sendLock.Lock()
	defer c.sendLock.Unlock()
	return sendJSON(c.sock, obj)
}

// disconnect – DISCONNECT: dừng vòng nhận trước, đóng socket sau.
func (c *ChatClient) disconnect() {
	c.stopping.Store(true)
	c.sock.Close()
}

func str(v any) string {
	if v == nil {
		return ""
	}
	return fmt.Sprint(v)
}

func joinList(v any) string {
	items, _ := v.([]any)
	parts := make([]string, len(items))
	for i, x := range items {
		parts[i] = str(x)
	}
	return strings.Join(parts, ", ")
}

// show – on_message: hiển thị một thông điệp từ server.
func show(m map[string]any) {
	ts := str(m["ts"])
	hhmmss := ""
	if len(ts) >= 19 {
		hhmmss = ts[11:19]
	}
	switch m["type"] {
	case "welcome":
		rooms := joinList(m["rooms"])
		if rooms == "" {
			rooms = "(chưa có)"
		}
		logLine("[client] Chào mừng! Trực tuyến: %s; phòng: %s", joinList(m["users"]), rooms)
	case "msg":
		logLine("[client] %s [%s] %s: %s", hhmmss, str(m["room"]), str(m["from"]), str(m["text"]))
	case "pm":
		logLine("[client] %s [riêng] %s: %s", hhmmss, str(m["from"]), str(m["text"]))
	case "sys":
		logLine("[client] * %s", str(m["text"]))
	case "error":
		logLine("[client] Lỗi %s: %s", str(m["code"]), str(m["text"]))
	case "pong":
		logLine("[client] pong")
	default:
		logLine("[client] %v", m)
	}
}

func chatClientMain(args []string) {
	host, port := parseEndpoint(args, "127.0.0.1", 9200)
	name := "user"
	if len(args) > 2 {
		name = args[2]
	}
	c := &ChatClient{onMessage: show, onStatus: func(s string) {
		if s == "connected" {
			logLine("[client] Đã kết nối %s:%d, gửi hello với tên %s", host, port, name)
		} else {
			logLine("[client] Mất kết nối: %s", strings.TrimPrefix(s, "lost: "))
		}
	}}
	if err := c.connect(host, port, name); err != nil {
		logLine("[client] Không kết nối được %s:%d: %v", host, port, err)
		return
	}
	lines := make(chan string)
	go func() { // READ_CONSOLE trong goroutine phụ
		sc := bufio.NewScanner(os.Stdin)
		for sc.Scan() {
			lines <- sc.Text()
		}
		close(lines)
	}()
	go func() { // bộ hẹn giờ nhịp tim 30 giây
		for range time.Tick(30 * time.Second) {
			c.send(map[string]any{"type": "ping"})
		}
	}()
	room := "" // phòng hiện tại
	quit := false
	for line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		logLine("[client] > %s", line)
		cmd, rest, _ := strings.Cut(line, " ")
		var err error
		switch {
		case cmd == "/join" && rest != "":
			room = rest
			err = c.send(map[string]any{"type": "join", "room": room})
		case cmd == "/leave" && rest != "":
			err = c.send(map[string]any{"type": "leave", "room": rest})
		case cmd == "/pm" && strings.Contains(rest, " "):
			to, text, _ := strings.Cut(rest, " ")
			err = c.send(map[string]any{"type": "pm", "to": to, "text": text})
		case cmd == "/ping":
			err = c.send(map[string]any{"type": "ping"})
		case cmd == "/quit":
			quit = true
		case strings.HasPrefix(line, "/"):
			logLine("[client] Lệnh không hợp lệ")
		case room == "":
			logLine("[client] Chưa chọn phòng: dùng /join tên_phòng")
		default:
			err = c.send(map[string]any{"type": "msg", "room": room, "text": line})
		}
		if err != nil {
			logLine("[client] Gửi lỗi: %v", err)
			quit = true
		}
		if quit {
			break
		}
	}
	if !quit {
		time.Sleep(time.Second) // hết dữ liệu nhập: chờ nhận nốt
	}
	c.disconnect()
	logLine("[client] Đã ngắt kết nối")
}
