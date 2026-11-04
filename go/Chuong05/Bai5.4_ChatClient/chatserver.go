package main

import (
	"context"
	"fmt"
	"net"
	"strconv"
	"sync"
	"time"
)

// Server chat tối giản để chạy thử lớp ChatClient (không thuộc Mã giả 5.5).
// Server chỉ cài đặt phần giao thức chat (Mục 6.3.1) mà Mã giả 5.5 dùng tới: hello → welcome
// (lỗi 409 nếu trùng tên), msg → chuyển tiếp tới mọi người trong một phòng chung, ping → pong,
// thông báo sys khi có người vào, ra. Bản đầy đủ (nhiều phòng, tin riêng, hộp thư đi cho mỗi
// client) là Mã giả 6.x ở Chương 6. Mỗi client có một khóa gửi để các khung không xen kẽ.

type member struct {
	conn net.Conn
	lock sync.Mutex // mỗi socket mỗi lúc chỉ ghi một khung
}

var (
	usersMu sync.Mutex
	users   = map[string]*member{} // tên → thành viên
	order   []string               // thứ tự vào phòng (cho danh sách trực tuyến)
)

func sendTo(m *member, obj any) error {
	m.lock.Lock()
	defer m.lock.Unlock()
	return SendJSONAsync(context.Background(), m.conn, obj)
}

func broadcast(obj any) {
	usersMu.Lock()
	list := make([]*member, 0, len(order)) // sao chép danh sách rồi mới gửi
	for _, n := range order {
		list = append(list, users[n])
	}
	usersMu.Unlock()
	for _, m := range list {
		sendTo(m, obj) // client vừa rời: bỏ qua lỗi
	}
}

func chatSession(conn net.Conn) {
	defer conn.Close()
	ctx := context.Background()
	hctx, cancel := context.WithTimeout(ctx, 10*time.Second) // hello trong 10 giây
	h, err := RecvJSONAsync(hctx, conn)
	cancel()
	if err != nil {
		return
	}
	name, _ := GetString(h, "name")
	if t, _ := GetString(h, "type"); t != "hello" || name == "" {
		SendJSONAsync(ctx, conn, map[string]any{"type": "error", "code": 400, "text": "Cần hello"})
		return
	}
	usersMu.Lock()
	if _, taken := users[name]; taken {
		usersMu.Unlock()
		SendJSONAsync(ctx, conn, map[string]any{"type": "error", "code": 409, "text": "Tên đã được dùng"})
		return
	}
	me := &member{conn: conn}
	users[name] = me
	order = append(order, name)
	online := append([]string(nil), order...)
	usersMu.Unlock()
	fmt.Printf("[server] %s đã kết nối\n", name)
	defer func() {
		usersMu.Lock()
		delete(users, name)
		for i, n := range order {
			if n == name {
				order = append(order[:i], order[i+1:]...)
				break
			}
		}
		usersMu.Unlock()
		fmt.Printf("[server] %s đã rời phòng\n", name)
		broadcast(map[string]any{"type": "sys", "text": name + " đã rời phòng"})
	}()
	sendTo(me, map[string]any{"type": "welcome", "users": online})
	broadcast(map[string]any{"type": "sys", "text": name + " đã vào phòng"})
	for {
		rctx, cancel := context.WithTimeout(ctx, 90*time.Second)
		m, err := RecvJSONAsync(rctx, conn)
		cancel()
		if err != nil {
			return // kết thúc phiên bình thường hoặc do lỗi
		}
		switch t, _ := GetString(m, "type"); t {
		case "msg":
			text, _ := GetString(m, "text")
			fmt.Printf("[server] %s: %s\n", name, text)
			broadcast(map[string]any{"type": "msg", "from": name, "room": "chung", "text": text, "ts": time.Now().Format("15:04:05")})
		case "ping":
			sendTo(me, map[string]any{"type": "pong"})
		default:
			sendTo(me, map[string]any{"type": "error", "code": 400, "text": "Sai loại thông điệp"})
		}
	}
}

// RunChatServer – server [host] [port].
func RunChatServer(args []string) int {
	host, port, err := ParseEndpoint(args, "0.0.0.0", 9200)
	if err != nil {
		fmt.Println(err)
		return 2
	}
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fmt.Printf("[server] Không mở được cổng: %v\n", err)
		return 1
	}
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	StopOnCtrlC(srv)
	for {
		conn, err := srv.Accept()
		if err != nil {
			break
		}
		go chatSession(conn)
	}
	usersMu.Lock() // đóng mọi kết nối còn mở khi dừng server
	for _, m := range users {
		m.conn.Close()
	}
	usersMu.Unlock()
	time.Sleep(100 * time.Millisecond) // để các phiên kịp ghi nhật ký rời phòng
	fmt.Println("[server] Dừng server")
	return 0
}
