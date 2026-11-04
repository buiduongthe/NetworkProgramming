// Bài 6.1 – Server tra cứu đa luồng: mỗi kết nối một luồng (goroutine).
//
// Cài đặt Mã giả 6.1 (Mục 6.2.1, Giáo trình Lập trình mạng). Kế thừa Mã giả 3.1 (khung server),
// phần nghiệp vụ là HANDLE_LOOKUP của Mã giả 3.7 (lookup_service.go).
//
// Chạy:  out/app threaded [host] [port] [max_clients]   (mặc định 0.0.0.0 9000 500)
package main

import (
	"net"
	"strconv"
	"sync/atomic"
	"time"
)

const MAX_CLIENTS = 500 // CONST MAX_CLIENTS ← 500

// idleConn: SETOPT(conn, RECV_TIMEOUT, ms) – Go dùng hạn chót tuyệt đối (deadline),
// nên trước mỗi lần RECV đặt lại hạn chót = bây giờ + thời gian chờ.
type idleConn struct {
	net.Conn
	timeout time.Duration
}

func (c *idleConn) Read(b []byte) (int, error) {
	c.Conn.SetReadDeadline(time.Now().Add(c.timeout))
	return c.Conn.Read(b)
}

// serve – SERVE: khối TRY của Mã giả 3.1, chạy trong goroutine riêng của mỗi kết nối.
// active là ATOMIC_COUNTER: atomic.Int64 của Go tăng, giảm nguyên tử, không cần khóa.
func serve(conn net.Conn, addr string, active *atomic.Int64) {
	defer func() { // FINALLY: luôn chạy, kể cả khi lỗi
		conn.Close()
		active.Add(-1) // active.decrement()
		logLine("[server] Client %s đã đóng kết nối", addr)
	}()
	c := &idleConn{Conn: conn, timeout: 300000 * time.Millisecond} // SETOPT(RECV_TIMEOUT, 300000)
	if err := handleLookup(c); err != nil {                        // CATCH NetError, Timeout, ...
		logLine("[server] Lỗi với %s: %v", addr, err)
	}
}

// threadedServer – THREADED_SERVER(args).
func threadedServer(args []string) {
	host, port := parseEndpoint(args, "0.0.0.0", 9000)
	maxClients := int64(intArg(args, 2, MAX_CLIENTS))
	// SOCKET(INET, STREAM), SETOPT(REUSEADDR), BIND, LISTEN: net.Listen làm cả bốn bước
	// (Go tự bật SO_REUSEADDR trên Linux, backlog lấy theo giới hạn của hệ điều hành).
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	logLine("[server] Đang lắng nghe tại %s:%d", host, port)
	logLine("[server] Mỗi kết nối một luồng, MAX_CLIENTS = %d", maxClients)
	var active atomic.Int64 // ATOMIC_COUNTER(0)
	for {
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			continue
		}
		addr := conn.RemoteAddr().String()
		if active.Load() >= maxClients { // MỚI: giới hạn
			logLine("[server] Từ chối %s: đã đủ %d kết nối", addr, maxClients)
			conn.Close()
			continue
		}
		n := active.Add(1) // active.increment()
		logLine("[server] Nhận %s, tạo luồng mới (đang phục vụ: %d)", addr, n)
		go serve(conn, addr, &active) // SPAWN_THREAD(SERVE, conn, addr, active)
	}
}
