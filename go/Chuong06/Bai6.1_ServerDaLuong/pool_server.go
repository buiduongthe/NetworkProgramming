// Bài 6.1 – Server tra cứu dùng nhóm luồng và hàng đợi có giới hạn.
//
// Cài đặt Mã giả 6.2 (Mục 6.2.2, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1: dùng lại
// nguyên serve (thread_server.go); k luồng thợ dùng chung một hàng đợi; hàng đợi đầy → 503.
//
// Chạy:  out/app pool [host] [port] [k] [queue_size]   (mặc định 0.0.0.0 9000 8 16)
package main

import (
	"net"
	"strconv"
	"sync/atomic"
)

// job: một phần tử của hàng đợi – (conn, addr).
type job struct {
	conn net.Conn
	addr string
}

// worker – WORKER(q): lấy kết nối khỏi hàng đợi rồi phục vụ bằng SERVE.
func worker(i int, q chan job) {
	for {
		j := <-q // q.take(): chờ đến khi có việc
		logLine("[server] Luồng thợ %d phục vụ %s", i, j.addr)
		var tmp atomic.Int64 // bộ đếm tạm ATOMIC_COUNTER(1)
		tmp.Store(1)
		serve(j.conn, j.addr, &tmp)
	}
}

// poolServer – POOL_SERVER(args, k, queue_size).
func poolServer(args []string) {
	host, port := parseEndpoint(args, "0.0.0.0", 9000)
	k := intArg(args, 2, 8)
	queueSize := intArg(args, 3, 16)
	// SOCKET, SETOPT, BIND, LISTEN như Mã giả 6.1
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	q := make(chan job, queueSize) // BLOCKING_QUEUE(queue_size): kênh có bộ đệm queue_size chỗ
	for i := 1; i <= k; i++ {      // FOR i ← 1 TO k: SPAWN_THREAD(WORKER, q)
		go worker(i, q)
	}
	logLine("[server] Đang lắng nghe tại %s:%d", host, port)
	logLine("[server] Nhóm %d luồng thợ, hàng đợi %d chỗ", k, queueSize)
	for {
		conn, err := srv.Accept() // ACCEPT
		if err != nil {
			continue
		}
		addr := conn.RemoteAddr().String()
		logLine("[server] Nhận %s", addr)
		select {
		case q <- job{conn, addr}: // q.try_put: thành công nếu còn chỗ
		default: // hàng đợi đầy: không chờ
			logLine("[server] Hàng đợi đầy, trả 503 cho %s", addr)
			sendJSON(conn, map[string]any{"status": 503, "error": "Server bận"}) // client đã đi: bỏ qua lỗi
			conn.Close()
		}
	}
}
