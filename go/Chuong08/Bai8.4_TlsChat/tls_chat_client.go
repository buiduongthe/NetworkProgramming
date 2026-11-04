// Bài 8.4 – Client chat tối giản có TLS và xác thực token.
//
// Cài đặt Mã giả 8.6 phía client (CONNECT: SAFE_CONNECT_ASYNC rồi TLS_WRAP_ASYNC với server_name ← host;
// Mục 8.4.5 Giáo trình Lập trình mạng), kế thừa Mã giả 5.5 (ChatClient: RECEIVE_LOOP, SEND_MESSAGE có khóa).
// Client gửi hello kèm token, vào phòng "chung", gửi từng dòng đọc từ bàn phím, rồi gửi ping và ngắt khi nhận pong.
// Tệp CA: nếu chỉ định, client tin cậy CA nội bộ đó; nếu không, dùng kho tin cậy của hệ điều hành.
//
// Chạy:  out/app client <host> <port> <tệp_token> [tệp_CA]
// Ví dụ: out/app client localhost 9200 certs/an.jwt certs/ca.crt
package main

import (
	"bufio"
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

func clog(format string, a ...any) { fmt.Printf("[client] "+format+"\n", a...) }

// ChatClient – STRUCT ChatClient (Mã giả 5.5) thêm kênh báo sự kiện welcome, pong.
type ChatClient struct {
	conn        net.Conn
	sendLock    sync.Mutex // mỗi lúc gửi một khung
	welcome     chan struct{}
	pong        chan struct{}
	welcomeOnce sync.Once
	pongOnce    sync.Once
	ok          atomic.Bool
	stopping    atomic.Bool
	recvDone    chan struct{}
}

func newChatClient() *ChatClient {
	return &ChatClient{welcome: make(chan struct{}), pong: make(chan struct{}), recvDone: make(chan struct{})}
}

func (c *ChatClient) signalWelcome() { c.welcomeOnce.Do(func() { close(c.welcome) }) }
func (c *ChatClient) signalPong()    { c.pongOnce.Do(func() { close(c.pong) }) }

// connect – CONNECT của Mã giả 5.5 + 8.6: kết nối TCP, bọc TLS, gửi hello kèm token.
func (c *ChatClient) connect(host string, port int, token string, cfg *tls.Config) error {
	raw, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 3*time.Second) // SAFE_CONNECT_ASYNC
	if err != nil {
		return err
	}
	tc := tls.Client(raw, cfg) // TLS_WRAP_ASYNC(raw, CLIENT, server_name ← host)
	tc.SetDeadline(time.Now().Add(10 * time.Second))
	if err := tc.Handshake(); err != nil {
		raw.Close()
		return &tlsError{err}
	}
	tc.SetDeadline(time.Time{})
	c.conn = tc
	st := tc.ConnectionState()
	clog("Đã kết nối TLS tới %s:%d: %s, %s", host, port, tls.VersionName(st.Version), tls.CipherSuiteName(st.CipherSuite))
	clog("Chứng chỉ server CN=%s hợp lệ, tên '%s' khớp SAN", st.PeerCertificates[0].Subject.CommonName, host)
	if err := c.send(map[string]any{"type": "hello", "token": token}); err != nil {
		return err
	}
	go c.receiveLoop() // RUN_BACKGROUND: không chờ
	return nil
}

type tlsError struct{ err error }

func (e *tlsError) Error() string { return e.err.Error() }

func (c *ChatClient) receiveLoop() {
	defer close(c.recvDone)
	for {
		m, err := recvJSON(c.conn)
		if err != nil {
			if !c.stopping.Load() {
				clog("Server đã đóng kết nối")
			}
			c.signalWelcome()
			c.signalPong()
			return
		}
		switch m["type"] {
		case "welcome":
			c.ok.Store(true)
			var names []string
			if list, isList := m["users"].([]any); isList {
				for _, x := range list {
					names = append(names, fmt.Sprint(x))
				}
			}
			clog("Server chấp nhận: tên = %v (từ token), trực tuyến: %s", m["name"], strings.Join(names, ", "))
			c.signalWelcome()
		case "error":
			clog("Lỗi từ server: %v %v", m["code"], m["text"])
			c.signalWelcome()
		case "sys":
			clog("* %v", m["text"])
		case "msg":
			clog("[%v] %v: %v", m["room"], m["from"], m["text"])
		case "pong":
			c.signalPong()
		}
	}
}

// send – SEND_MESSAGE: khóa gửi.
func (c *ChatClient) send(obj map[string]any) error {
	c.sendLock.Lock()
	defer c.sendLock.Unlock()
	return sendJSON(c.conn, obj)
}

// disconnect – DISCONNECT.
func (c *ChatClient) disconnect() {
	c.stopping.Store(true)
	if c.conn != nil {
		c.conn.Close()
	}
}

func waitFor(ch chan struct{}, d time.Duration) bool {
	select {
	case <-ch:
		return true
	case <-time.After(d):
		return false
	}
}

func clientMain(args []string) int {
	if len(args) < 3 {
		fail("Cách dùng: out/app client <host> <port> <tệp_token> [tệp_CA]")
	}
	host, port, tokenFile := args[0], mustPort(args[1]), args[2]
	caFile := ""
	if len(args) > 3 {
		caFile = args[3]
	}
	data, err := os.ReadFile(tokenFile)
	if err != nil {
		fail("[client] Không đọc được tệp token: " + err.Error())
	}
	token := strings.TrimSpace(string(data))
	// kiểm tra chứng chỉ và tên máy (mặc định của crypto/tls, không bao giờ tắt)
	cfg := &tls.Config{ServerName: host, MinVersion: tls.VersionTLS12}
	trust := "kho chứng chỉ của hệ điều hành"
	if caFile != "" {
		pem, err := os.ReadFile(caFile)
		if err != nil {
			fail("[client] Không đọc được tệp CA: " + err.Error())
		}
		cfg.RootCAs = x509.NewCertPool()
		cfg.RootCAs.AppendCertsFromPEM(pem)
		trust = "CA trong " + caFile
	}
	clog("Tin cậy: %s; token: %s", trust, tokenFile)
	c := newChatClient()
	if err := c.connect(host, port, token, cfg); err != nil {
		if te, isTLS := err.(*tlsError); isTLS {
			clog("Bắt tay TLS thất bại: %v", te)
		} else {
			clog("Không kết nối được: %v", err)
		}
		c.disconnect()
		return 1
	}
	waitFor(c.welcome, 5*time.Second)
	if !c.ok.Load() {
		waitFor(c.recvDone, time.Second) // để vòng nhận in nốt thông báo server đóng kết nối
		c.disconnect()
		return 1
	}
	c.send(map[string]any{"type": "join", "room": "chung"})
	sc := bufio.NewScanner(os.Stdin) // READ_CONSOLE: đọc các dòng cần gửi
	for sc.Scan() {
		c.send(map[string]any{"type": "msg", "room": "chung", "text": sc.Text()})
	}
	c.send(map[string]any{"type": "ping"}) // pong về sau mọi tin trước đó
	waitFor(c.pong, 5*time.Second)
	c.disconnect() // DISCONNECT
	clog("Đã ngắt kết nối")
	return 0
}
