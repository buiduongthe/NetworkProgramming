// Bài 8.4 – Server chat tối giản có TLS và xác thực token.
//
// Cài đặt Mã giả 8.6 phía server (TLS_SESSION, VERIFY_JWT trong hello; Mục 8.4.5 Giáo trình Lập trình mạng),
// kế thừa Mã giả 6.5–6.6 (CHAT_SESSION, BROADCAST, WRITER – bản rút gọn: hello, join, msg, ping).
// Cấu hình: TLS_CERT, TLS_KEY (tệp PEM của chứng chỉ và khóa riêng), CHAT_JWT_KEY (khóa ký JWT).
// Tên người dùng lấy từ claim sub của token đã kiểm tra.
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 9200)
package main

import (
	"crypto/tls"
	"fmt"
	"net"
	"os"
	"os/signal"
	"sort"
	"strconv"
	"sync"
	"time"
)

// Client: một người dùng đã đăng nhập; outbox là hộp thư đi BOUNDED_QUEUE(256).
type Client struct {
	name   string
	conn   net.Conn
	outbox chan map[string]any
	mu     sync.Mutex
	closed bool
	done   chan struct{}
}

var (
	lock  sync.Mutex                     // bảo vệ users, rooms (các goroutine phiên chạy song song)
	users = map[string]*Client{}         // tên → Client
	rooms = map[string]map[string]bool{} // phòng → tập tên
)

// HelloError: lời chào không hợp lệ (400), token sai (401), trùng tên (409).
type HelloError struct {
	code int
	text string
}

func (e *HelloError) Error() string { return e.text }

func slog(format string, a ...any) { fmt.Printf("[server] "+format+"\n", a...) }

// loadCertAndKey – LOAD_CERT_AND_KEY(ENV("TLS_CERT"), …); TLS_WRAP(…, min ← TLS1_2).
func loadCertAndKey() *tls.Config {
	cert, err := tls.LoadX509KeyPair(os.Getenv("TLS_CERT"), os.Getenv("TLS_KEY"))
	if err != nil {
		fail("[server] Không nạp được chứng chỉ (TLS_CERT, TLS_KEY): " + err.Error())
	}
	return &tls.Config{Certificates: []tls.Certificate{cert}, MinVersion: tls.VersionTLS12} // ưu tiên TLS 1.3
}

func enqueue(c *Client, msg map[string]any) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return
	}
	select {
	case c.outbox <- msg:
	default: // client chậm: ngắt kết nối
		slog("Ngắt client chậm %s", c.name)
		c.conn.Close()
	}
}

func sendTo(name string, msg map[string]any, sender *Client) {
	lock.Lock()
	c := users[name]
	lock.Unlock()
	if c != nil {
		enqueue(c, msg)
	} else if sender != nil {
		enqueue(sender, map[string]any{"type": "error", "code": 404, "text": "Không có người nhận"})
	}
}

func broadcast(room string, msg map[string]any, sender *Client) {
	lock.Lock()
	var targets []string // sao chép danh sách thành viên
	for name := range rooms[room] {
		targets = append(targets, name)
	}
	member := sender != nil && rooms[room][sender.name]
	lock.Unlock()
	sort.Strings(targets)
	if sender != nil && !member {
		enqueue(sender, map[string]any{"type": "error", "code": 403, "text": "Chưa tham gia phòng"})
		return
	}
	for _, name := range targets {
		sendTo(name, msg, nil)
	}
}

// writerTask: tác vụ ghi duy nhất của mỗi client.
func writerTask(c *Client) {
	defer close(c.done)
	for msg := range c.outbox {
		if err := sendJSON(c.conn, msg); err != nil {
			c.conn.Close()
			return
		}
	}
}

func recvTimeout(conn net.Conn, ms int) (map[string]any, error) {
	conn.SetReadDeadline(time.Now().Add(time.Duration(ms) * time.Millisecond))
	return recvJSON(conn)
}

// chatSession – CHAT_SESSION (Mã giả 6.5) chạy trên kết nối đã bọc TLS; hello mang token thay cho tên.
func chatSession(conn net.Conn, addr string, jwtKey string) {
	var c *Client
	defer func() {
		if c != nil { // UNREGISTER, đóng hộp thư đi
			lock.Lock()
			delete(users, c.name)
			for _, members := range rooms {
				delete(members, c.name)
			}
			lock.Unlock()
			c.mu.Lock()
			c.closed = true
			close(c.outbox) // báo tác vụ ghi kết thúc sau khi gửi nốt
			c.mu.Unlock()
			select {
			case <-c.done:
			case <-time.After(time.Second):
			}
			slog("%s rời phiên", c.name)
		}
		conn.Close()
	}()
	err := func() error {
		h, err := recvTimeout(conn, 10000)
		if err != nil {
			return err
		}
		if h["type"] != "hello" {
			return &HelloError{400, "Thông điệp đầu phải là hello"}
		}
		claims, err := verifyJWT(h["token"], jwtKey, ISSUER, "chat") // VERIFY_JWT(h.token, …, audience ← "chat")
		if err != nil {
			return &HelloError{401, "Token không hợp lệ: " + err.Error()}
		}
		sub := claims["sub"].(string)
		lock.Lock() // REGISTER(claims.sub, conn): tên lấy từ token
		if _, exists := users[sub]; exists {
			lock.Unlock()
			return &HelloError{409, "Tên đã được sử dụng"}
		}
		c = &Client{name: sub, conn: conn, outbox: make(chan map[string]any, 256), done: make(chan struct{})}
		users[sub] = c
		names := make([]string, 0, len(users))
		for n := range users {
			names = append(names, n)
		}
		lock.Unlock()
		sort.Strings(names)
		slog("%s đăng nhập từ %s (tên lấy từ claim sub)", c.name, addr)
		go writerTask(c)
		enqueue(c, map[string]any{"type": "welcome", "name": c.name, "users": names})
		for {
			m, err := recvTimeout(conn, 90000)
			if err != nil {
				return err
			}
			room, _ := m["room"].(string)
			switch m["type"] {
			case "join":
				lock.Lock()
				if rooms[room] == nil {
					rooms[room] = map[string]bool{}
				}
				rooms[room][c.name] = true
				lock.Unlock()
				broadcast(room, map[string]any{"type": "sys", "text": c.name + " đã vào phòng " + room}, nil)
			case "msg":
				broadcast(room, map[string]any{"type": "msg", "from": c.name, "room": room,
					"text": m["text"], "ts": float64(time.Now().UnixMilli()) / 1000}, c)
			case "ping":
				enqueue(c, map[string]any{"type": "pong"})
			default:
				enqueue(c, map[string]any{"type": "error", "code": 400, "text": "Loại thông điệp không hỗ trợ"})
			}
		}
	}()
	if he, ok := err.(*HelloError); ok { // lời chào không hợp lệ
		slog("Từ chối %s: %d %s", addr, he.code, he.text)
		sendJSON(conn, map[string]any{"type": "error", "code": he.code, "text": he.text})
	} else if c == nil { // kết thúc phiên bình thường hoặc do lỗi
		slog("Kết nối %s đóng trước khi đăng nhập: %v", addr, err)
	}
}

// tlsSession – Mã giả 8.6, TLS_SESSION(conn, addr): thay SERVE_ASYNC; bắt tay TLS trước phiên chat.
func tlsSession(conn net.Conn, cfg *tls.Config, jwtKey string) {
	addr := conn.RemoteAddr().String()
	tc := tls.Server(conn, cfg) // TLS_WRAP_ASYNC(conn, SERVER, cert, min ← TLS1_2)
	tc.SetDeadline(time.Now().Add(10 * time.Second))
	if err := tc.Handshake(); err != nil { // CATCH TlsError AS e
		slog("Bắt tay TLS lỗi %s: %v", addr, err)
		conn.Close()
		return
	}
	tc.SetDeadline(time.Time{})
	st := tc.ConnectionState()
	slog("Bắt tay TLS thành công %s: %s, %s", addr, tls.VersionName(st.Version), tls.CipherSuiteName(st.CipherSuite))
	chatSession(tc, addr, jwtKey) // tls thay cho conn
}

func serverMain(args []string) {
	host, port := envOr("APP_HOST", "0.0.0.0"), envOr("APP_PORT", "9200") // Mã giả 2.1 (rút gọn)
	if len(args) > 0 {
		host = args[0]
	}
	if len(args) > 1 {
		port = args[1]
	}
	p := mustPort(port)
	cfg := loadCertAndKey() // nạp một lần khi khởi động
	jwtKey, err := secret("CHAT_JWT_KEY")
	if err != nil {
		fail("[server] " + err.Error())
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(p)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		slog("Dừng server")
		os.Exit(0)
	}()
	slog("Đang lắng nghe (TLS ≥ 1.2) tại %s:%d", host, p)
	for {
		conn, err := srv.Accept()
		if err != nil {
			continue
		}
		go tlsSession(conn, cfg, jwtKey)
	}
}
