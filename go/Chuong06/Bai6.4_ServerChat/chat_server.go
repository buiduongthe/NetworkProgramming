// Bài 6.4 – Server chat nhiều phòng: vòng chấp nhận kết nối và tác vụ phiên.
//
// Cài đặt Mã giả 6.5 (Mục 6.3.3, Giáo trình Lập trình mạng): kế thừa Mã giả 6.3 (server bất đồng bộ),
// thay SERVE_ASYNC bằng CHAT_SESSION. Các thủ tục phát, gửi riêng, tác vụ ghi nằm ở chat_broadcast.go
// (Mã giả 6.6). Giao thức: Bảng đặc tả giao thức chat (Mục 6.3.1), cổng mặc định 9200.
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 9200)
package main

import (
	"net"
	"regexp"
	"strconv"
	"time"
)

// Tập ký tự cho phép của tên người dùng, tên phòng: chữ cái (kể cả tiếng Việt), chữ số, '_', '-'.
var NAME_RE = regexp.MustCompile(`^[\p{L}\p{N}_-]{1,32}$`)

const RATE_LIMIT = 10 // tối đa 10 tin msg/pm mỗi giây (Mục 6.3.4)

// HelloError: lời chào không hợp lệ (400) hoặc trùng tên (409).
type HelloError struct{ code int }

func (e *HelloError) Error() string { return strconv.Itoa(e.code) }

func valid(v any) bool {
	s, ok := v.(string)
	return ok && NAME_RE.MatchString(s)
}

func isString(v any) bool {
	_, ok := v.(string)
	return ok
}

func rateOK(c *Client) bool {
	now := time.Now()
	if now.Sub(c.winStart) >= time.Second {
		c.winStart, c.winCount = now, 0
	}
	c.winCount++
	return c.winCount <= RATE_LIMIT
}

// recvWithTimeout – AWAIT WITH_TIMEOUT(ms, RECV_JSON_ASYNC(conn)).
func recvWithTimeout(conn net.Conn, ms int) (map[string]any, error) {
	conn.SetReadDeadline(time.Now().Add(time.Duration(ms) * time.Millisecond))
	return recvJSON(conn)
}

// chatSession – CHAT_SESSION (Mã giả 6.5, dòng 4–33).
func chatSession(conn net.Conn, addr string) {
	var c *Client  // chưa đăng ký tên
	defer func() { // FINALLY
		if c != nil {
			unregister(c)
			c.closeOutbox()
			select { // chờ tác vụ ghi gửi nốt tin còn lại (tối đa 1 giây)
			case <-c.done:
			case <-time.After(time.Second):
			}
			logLine("[server] %s rời server", c.name)
		}
		conn.Close() // CLOSE(conn)
	}()
	err := func() error { // khối TRY
		h, err := recvWithTimeout(conn, 10000) // 10 giây cho hello
		if err != nil {
			return err
		}
		if h["type"] != "hello" || !valid(h["name"]) {
			return &HelloError{400}
		}
		c = register(h["name"].(string), conn) // nil nếu trùng tên
		if c == nil {
			return &HelloError{409}
		}
		logLine("[server] %s đăng ký từ %s", c.name, addr)
		go writer(c) // RUN_BACKGROUND(WRITER(c))
		lock.Lock()
		snapshot := map[string]any{"type": "welcome", "users": sortedKeys(users), "rooms": sortedKeys(rooms)}
		lock.Unlock()
		enqueue(c, snapshot)
		for {
			m, err := recvWithTimeout(conn, 90000) // 3 chu kỳ nhịp tim
			if err != nil {
				return err
			}
			t := m["type"]
			switch {
			case (t == "msg" || t == "pm") && !rateOK(c):
				enqueue(c, errorMsg(429))
			case t == "join" && valid(m["room"]):
				joinRoom(c, m["room"].(string))
			case t == "leave" && isString(m["room"]):
				leaveRoom(c, m["room"].(string))
			case t == "msg" && isString(m["room"]) && isString(m["text"]):
				room := m["room"].(string)
				broadcast(room, map[string]any{"type": "msg", "from": c.name, "room": room,
					"text": m["text"], "ts": nowTs()}, c)
			case t == "pm" && isString(m["to"]) && isString(m["text"]):
				sendTo(m["to"].(string), map[string]any{"type": "pm", "from": c.name, "text": m["text"],
					"ts": nowTs()}, c)
			case t == "ping":
				enqueue(c, map[string]any{"type": "pong"})
			default: // OTHERWISE
				enqueue(c, errorMsg(400))
			}
		}
	}()
	if he, ok := err.(*HelloError); ok { // CATCH HelloError: lời chào không hợp lệ
		logLine("[server] Từ chối %s: lỗi %d", addr, he.code)
		sendJSON(conn, errorMsg(he.code))
	}
	// CATCH ConnectionClosed, Timeout, NetError, ProtocolError: kết thúc phiên bình thường hoặc do lỗi
}

// chatServer: vòng chấp nhận kết nối (như ASYNC_SERVER của Mã giả 6.3).
func chatServer(args []string) {
	host, port := parseEndpoint(args, "0.0.0.0", 9200)
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	logLine("[server] Server chat đang lắng nghe tại %s:%d", host, port)
	for {
		conn, err := srv.Accept() // AWAIT ACCEPT_ASYNC
		if err != nil {
			continue
		}
		go chatSession(conn, conn.RemoteAddr().String()) // RUN_BACKGROUND
	}
}
