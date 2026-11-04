// Bài 6.4 – Server chat: cấu trúc dữ liệu dùng chung, phát tới phòng, gửi riêng và tác vụ ghi.
//
// Cài đặt Mã giả 6.6 (Mục 6.3.3, Giáo trình Lập trình mạng) cùng phần cấu trúc dữ liệu của Mã giả 6.5
// (STRUCT Client, users, rooms, lock) và các thủ tục mà mã giả chỉ nêu tên: REGISTER, UNREGISTER,
// JOIN_ROOM, LEAVE_ROOM. Dùng sendJSON (SEND_JSON_ASYNC của Mã giả 5.1, netutil.go).
package main

import (
	"net"
	"sort"
	"sync"
	"time"
)

const OUTBOX_SIZE = 256 // BOUNDED_QUEUE(256)

var ERRORS = map[int]string{400: "Sai cú pháp", 403: "Chưa tham gia phòng", 404: "Không có người nhận",
	409: "Tên đã được sử dụng", 429: "Gửi quá nhanh"}

// Client – STRUCT Client { name, sock, outbox ← BOUNDED_QUEUE(256) }.
type Client struct {
	name   string
	sock   net.Conn
	outbox chan map[string]any // hộp thư đi: kênh có bộ đệm 256 thông điệp
	mu     sync.Mutex          // bảo vệ cờ closed (không gửi vào kênh đã đóng)
	closed bool
	done   chan struct{} // đóng khi tác vụ ghi kết thúc
	// đếm tin trong cửa sổ 1 giây (lỗi 429); chỉ tác vụ phiên của client dùng
	winStart time.Time
	winCount int
}

// closeOutbox – outbox.close(): đóng kênh để tác vụ ghi gửi nốt tin còn lại rồi dừng.
func (c *Client) closeOutbox() {
	c.mu.Lock()
	defer c.mu.Unlock()
	if !c.closed {
		c.closed = true
		close(c.outbox)
	}
}

var users = map[string]*Client{}         // tên → Client
var rooms = map[string]map[string]bool{} // tên phòng → tập tên
var lock sync.Mutex                      // LOCK() bảo vệ users, rooms

func errorMsg(code int) map[string]any {
	return map[string]any{"type": "error", "code": code, "text": ERRORS[code]}
}

// nowTs – NOW(): thời điểm do server gán, dạng ISO 8601 UTC.
func nowTs() string {
	return time.Now().UTC().Format("2006-01-02T15:04:05.000Z")
}

// sortedKeys: danh sách khóa đã sắp xếp (map của Go không có thứ tự).
func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// register – REGISTER: thêm tên vào users nếu chưa có; trả nil nếu trùng tên.
func register(name string, sock net.Conn) *Client {
	lock.Lock()
	defer lock.Unlock()
	if _, ok := users[name]; ok {
		return nil
	}
	c := &Client{name: name, sock: sock, outbox: make(chan map[string]any, OUTBOX_SIZE),
		done: make(chan struct{})}
	users[name] = c
	return c
}

// unregister – UNREGISTER: xóa client khỏi users và mọi phòng, báo cho các phòng đó.
func unregister(c *Client) {
	var left []string
	lock.Lock()
	delete(users, c.name)
	for _, r := range sortedKeys(rooms) {
		if rooms[r][c.name] {
			left = append(left, r)
			delete(rooms[r], c.name)
			if len(rooms[r]) == 0 {
				delete(rooms, r)
			}
		}
	}
	lock.Unlock()
	for _, r := range left { // phát ngoài khóa
		broadcast(r, map[string]any{"type": "sys", "text": c.name + " đã rời phòng " + r}, nil)
	}
}

// joinRoom – JOIN_ROOM: thêm tên vào phòng rồi phát thông báo hệ thống.
func joinRoom(c *Client, room string) {
	lock.Lock()
	if rooms[room] == nil {
		rooms[room] = map[string]bool{}
	}
	rooms[room][c.name] = true
	lock.Unlock()
	logLine("[server] %s vào phòng %s", c.name, room)
	broadcast(room, map[string]any{"type": "sys", "text": c.name + " đã vào phòng " + room}, nil)
}

// leaveRoom – LEAVE_ROOM: rời phòng (403 nếu không ở trong phòng), báo cho các thành viên còn lại.
func leaveRoom(c *Client, room string) {
	lock.Lock()
	member := rooms[room][c.name]
	if member {
		delete(rooms[room], c.name)
		if len(rooms[room]) == 0 {
			delete(rooms, room)
		}
	}
	lock.Unlock()
	if !member {
		enqueue(c, errorMsg(403))
		return
	}
	logLine("[server] %s rời phòng %s", c.name, room)
	broadcast(room, map[string]any{"type": "sys", "text": c.name + " đã rời phòng " + room}, nil)
}

// broadcast – BROADCAST (Mã giả 6.6, dòng 1–10); sender nil: tin hệ thống.
func broadcast(room string, msg map[string]any, sender *Client) {
	var targets []string // phòng chưa tồn tại: tập rỗng
	lock.Lock()          // giữ khóa ngắn
	if members, ok := rooms[room]; ok {
		targets = sortedKeys(members) // COPY
	}
	lock.Unlock()
	if sender != nil && !contains(targets, sender.name) {
		enqueue(sender, errorMsg(403))
		return
	}
	for _, name := range targets {
		sendTo(name, msg, nil)
	}
}

func contains(list []string, s string) bool {
	for _, x := range list {
		if x == s {
			return true
		}
	}
	return false
}

// sendTo – SEND_TO (Mã giả 6.6, dòng 12–18).
func sendTo(name string, msg map[string]any, sender *Client) {
	lock.Lock()
	c := users[name]
	lock.Unlock()
	if c != nil {
		enqueue(c, msg)
	} else if sender != nil {
		enqueue(sender, errorMsg(404))
	}
}

// enqueue – ENQUEUE (Mã giả 6.6, dòng 20–24): try_put không bao giờ chờ.
func enqueue(c *Client, msg map[string]any) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed { // client đã rời: bỏ tin
		return
	}
	select {
	case c.outbox <- msg: // try_put thành công
	default: // hàng đợi đầy: client chậm
		logLine("[server] Ngắt client chậm %s", c.name)
		c.sock.Close() // CLOSE(c.sock)
	}
}

// writer – WRITER (Mã giả 6.6, dòng 26–34): tác vụ ghi duy nhất của mỗi socket.
func writer(c *Client) {
	defer close(c.done)
	for msg := range c.outbox { // take_async(); kênh đã đóng và hết tin → dừng
		if err := sendJSON(c.sock, msg); err != nil { // NetError
			c.sock.Close() // phiên sẽ kết thúc theo
			return
		}
	}
}
