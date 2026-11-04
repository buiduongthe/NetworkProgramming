package main

import (
	"context"
	"errors"
	"net"
	"sync"
)

// Lớp dịch vụ ChatClient – Mã giả 5.5 (Mục 5.3.3).
// Kế thừa: Mã giả 5.1 (khung JSON bất đồng bộ); SAFE_CONNECT_ASYNC = Mã giả 2.2 + ct.
// Lớp không tham chiếu tới điều khiển giao diện nào: tầng giao diện (ở đây là chatcli.go)
// chỉ đăng ký hai sự kiện OnMessage, OnStatus.
// Ánh xạ: cts ↔ context.WithCancel; send_lock ↔ sync.Mutex; RUN_BACKGROUND ↔ từ khóa go.

// ChatClient – STRUCT ChatClient { sock, cts, send_lock, on_message, on_status } (dòng 1–2).
type ChatClient struct {
	sock      net.Conn
	ctx       context.Context    // cts.token: tín hiệu hủy dùng chung cho mọi thao tác
	cancel    context.CancelFunc // CANCEL(cts)
	sendLock  sync.Mutex         // ASYNC_LOCK(): mỗi lúc gửi một khung
	OnMessage func(msg map[string]any)
	OnStatus  func(text string)
}

// NewChatClient tạo client với hai sự kiện rỗng.
func NewChatClient() *ChatClient {
	return &ChatClient{OnMessage: func(map[string]any) {}, OnStatus: func(string) {}}
}

// Connect – Mã giả 5.5, dòng 4–14: CONNECT(c, host, port, name).
func (c *ChatClient) Connect(host string, port int, name string) error {
	sock, err := SafeConnectAsync(context.Background(), host, port, 3000) // AWAIT SAFE_CONNECT_ASYNC
	if err != nil {
		return err
	}
	c.sock = sock
	c.ctx, c.cancel = context.WithCancel(context.Background()) // c.cts ← NEW_CANCELLATION()
	hello := map[string]any{"type": "hello", "name": name}
	if err := SendJSONAsync(c.ctx, c.sock, hello); err != nil { // CATCH NetError
		c.sock.Close() // không để rò socket
		return err     // RAISE e
	}
	go c.receiveLoop()      // RUN_BACKGROUND(RECEIVE_LOOP(c)): không chờ
	c.OnStatus("connected") // RAISE_EVENT(c.on_status, "connected")
	return nil
}

// receiveLoop – Mã giả 5.5, dòng 16–27: RECEIVE_LOOP(c), chạy trên goroutine nền.
func (c *ChatClient) receiveLoop() {
	for { // LOOP
		msg, err := RecvJSONAsync(c.ctx, c.sock)
		if errors.Is(err, context.Canceled) { // CATCH Cancelled
			return // ngắt kết nối chủ động
		}
		if err != nil { // CATCH NetError, ConnectionClosed, ProtocolError, JsonError
			c.sock.Close() // giải phóng socket
			c.OnStatus("lost: " + err.Error())
			return
		}
		c.OnMessage(msg) // RAISE_EVENT(c.on_message, msg)
	}
}

// SendMessage – Mã giả 5.5, dòng 29–36: SEND_MESSAGE(c, room, text).
func (c *ChatClient) SendMessage(room, text string) error {
	c.sendLock.Lock()         // AWAIT ACQUIRE_ASYNC(c.send_lock)
	defer c.sendLock.Unlock() // FINALLY RELEASE(c.send_lock)
	m := map[string]any{"type": "msg", "room": room, "text": text}
	return SendJSONAsync(c.ctx, c.sock, m)
}

// Disconnect – Mã giả 5.5, dòng 37–39: hủy trước, đóng socket sau.
func (c *ChatClient) Disconnect() {
	if c.cancel != nil {
		c.cancel() // CANCEL(c.cts)
	}
	if c.sock != nil {
		c.sock.Close() // CLOSE(c.sock)
	}
}
