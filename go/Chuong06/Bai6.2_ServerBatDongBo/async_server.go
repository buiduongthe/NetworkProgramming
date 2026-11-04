// Bài 6.2 – Server tra cứu bất đồng bộ: mỗi kết nối một tác vụ.
//
// Cài đặt Mã giả 6.3 (Mục 6.2.3, Giáo trình Lập trình mạng). Kế thừa Mã giả 6.1 (SOCKET, SETOPT,
// BIND, LISTEN), các hàm khung JSON của Mã giả 5.1/3.6 (netutil.go) và PROCESS_REQUEST của Mã giả 3.7.
//
// Go không có từ khóa async/await: tác vụ bất đồng bộ là goroutine. Khi goroutine chờ ở RECV,
// bộ lập lịch của Go (dựa trên epoll) tạm cất goroutine và dùng luồng hệ điều hành cho việc khác,
// nên một lời gọi "chặn" trong goroutine có tác dụng như AWAIT ..._ASYNC.
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 9000)
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"net"
	"os"
	"path/filepath"
	"runtime/pprof"
	"strconv"
	"sync/atomic"
	"time"
)

var DICT map[string]string // LOAD_JSON_FILE("glossary.json")
var TERMS []string         // thuật ngữ theo thứ tự trong tệp

func loadGlossary() {
	path := "glossary.json"
	if _, err := os.Stat(path); err != nil {
		exe, _ := os.Executable()
		path = filepath.Join(filepath.Dir(exe), "..", "glossary.json") // out/app → thư mục bài
	}
	data, err := os.ReadFile(path)
	if err != nil {
		fail("Không đọc được glossary.json: " + err.Error())
	}
	if err := json.Unmarshal(data, &DICT); err != nil {
		fail("glossary.json sai định dạng: " + err.Error())
	}
	dec := json.NewDecoder(bytes.NewReader(data)) // giữ thứ tự khóa cho thao tác list
	dec.Token()
	for dec.More() {
		k, _ := dec.Token()
		var v string
		dec.Decode(&v)
		TERMS = append(TERMS, k.(string))
	}
}

// processRequest – Mã giả 3.7, dòng 2–12 (giữ nguyên): nghiệp vụ nhanh, không chặn.
func processRequest(req map[string]any) map[string]any {
	switch req["op"] {
	case "lookup":
		term, _ := req["term"].(string)
		if text, ok := DICT[term]; ok {
			return map[string]any{"status": 200, "term": term, "text": text}
		}
		return map[string]any{"status": 404, "error": "Không có thuật ngữ"}
	case "list":
		return map[string]any{"status": 200, "terms": TERMS}
	}
	return map[string]any{"status": 400, "error": "Sai thao tác"}
}

// serveAsync – SERVE_ASYNC: vòng yêu cầu–phản hồi của một kết nối.
func serveAsync(conn net.Conn, addr string, tasks *atomic.Int64) {
	defer tasks.Add(-1)
	defer conn.Close() // FINALLY: CLOSE(conn)
	var reason string
	for {
		conn.SetReadDeadline(time.Now().Add(300000 * time.Millisecond)) // WITH_TIMEOUT(300000, …)
		req, err := recvJSON(conn)                                      // AWAIT RECV_JSON_ASYNC(conn)
		if err == nil {
			err = sendJSON(conn, processRequest(req)) // AWAIT SEND_JSON_ASYNC(conn, …)
		}
		if err == nil {
			continue
		}
		// CATCH ConnectionClosed, NetError, Timeout, ProtocolError AS e
		var cc *ConnectionClosed
		var ne net.Error
		var pe *ProtocolError
		var je *JsonError
		switch {
		case errors.As(err, &cc) && cc.Received == 0:
			reason = "client đóng kết nối"
		case errors.As(err, &cc):
			reason = err.Error()
		case errors.As(err, &ne) && ne.Timeout():
			reason = "quá thời gian chờ nhận"
		case errors.As(err, &pe), errors.As(err, &je): // ProtocolError, JSON sai
			reason = "lỗi giao thức: " + err.Error()
		default: // NetError
			reason = "lỗi mạng: " + err.Error()
		}
		break
	}
	logLine("[server] Kết thúc %s: %s", addr, reason) // LOG("Kết thúc", addr, e)
}

// asyncServer – ASYNC_SERVER(args).
func asyncServer(args []string) {
	host, port := parseEndpoint(args, "0.0.0.0", 9000)
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port))) // như Mã giả 6.1
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	logLine("[server] Server bất đồng bộ đang lắng nghe tại %s:%d", host, port)
	threads := pprof.Lookup("threadcreate") // số luồng hệ điều hành mà tiến trình đã tạo
	var tasks atomic.Int64                  // số tác vụ đang chạy
	for {
		conn, err := srv.Accept() // AWAIT ACCEPT_ASYNC(srv)
		if err != nil {
			continue
		}
		addr := conn.RemoteAddr().String()
		n := tasks.Add(1)
		go serveAsync(conn, addr, &tasks) // RUN_BACKGROUND(SERVE_ASYNC(conn, addr)): không chờ
		logLine("[server] Nhận %s (tác vụ: %d, luồng: %d)", addr, n, threads.Count())
	}
}
