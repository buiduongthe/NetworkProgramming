// Bài 6.4 – Các thủ tục kế thừa dùng chung (chép gọn từ Chương 2, 3 và 5).
//
//   - parseEndpoint: Mã giả 2.1 (Mục 2.1.3)
//   - safeConnect:   Mã giả 2.2 (rút gọn, Mục 2.2)
//   - recvExact:     Mã giả 3.4 (Mục 3.4.3)
//   - sendJSON, recvJSON: Mã giả 3.6 và 5.1 (SEND_JSON_ASYNC, RECV_JSON_ASYNC khi chạy trong goroutine)
package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"strconv"
	"sync"
	"time"
)

const MAX_FRAME = 1048576 // Mã giả 3.6: 1 MiB

// ConnectionClosed: đối tác đóng kết nối; Received = số byte của khung đang nhận dở.
type ConnectionClosed struct{ Received int }

func (e *ConnectionClosed) Error() string {
	return fmt.Sprintf("Kết nối bị đóng sau %d byte", e.Received)
}

// ProtocolError: vi phạm định dạng khung (ví dụ khung quá lớn).
type ProtocolError struct{ Msg string }

func (e *ProtocolError) Error() string { return e.Msg }

// JsonError: nội dung khung không phải đối tượng JSON hợp lệ.
type JsonError struct{ Msg string }

func (e *JsonError) Error() string { return e.Msg }

var printMu sync.Mutex

// logLine: LOG – in một dòng trọn vẹn (các goroutine không chen giữa dòng của nhau).
func logLine(format string, a ...any) {
	printMu.Lock()
	defer printMu.Unlock()
	fmt.Printf(format+"\n", a...)
}

// fail: in thông báo lỗi ra stderr rồi kết thúc (như sys.exit(thông_báo) của Python).
func fail(msg string) {
	fmt.Fprintln(os.Stderr, msg)
	os.Exit(1)
}

// onInterrupt: khi nhấn Ctrl+C (SIGINT), in thông báo rồi kết thúc chương trình.
func onInterrupt(msg string) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		logLine("%s", msg)
		os.Exit(0)
	}()
}

// parseEndpoint – Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định.
func parseEndpoint(args []string, defaultHost string, defaultPort int) (string, int) {
	host := defaultHost
	portText := strconv.Itoa(defaultPort)
	if v := os.Getenv("APP_HOST"); v != "" {
		host = v
	}
	if v := os.Getenv("APP_PORT"); v != "" {
		portText = v
	}
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		portText = args[1]
	}
	port, err := strconv.Atoi(portText)
	if err != nil {
		fail("Cổng phải là số nguyên: " + portText)
	}
	if port < 1 || port > 65535 {
		fail("Cổng không hợp lệ: " + portText)
	}
	return host, port
}

// intArg: đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định.
func intArg(args []string, i int, def int) int {
	if len(args) <= i {
		return def
	}
	v, err := strconv.Atoi(args[i])
	if err != nil {
		fail("Tham số phải là số nguyên: " + args[i])
	}
	return v
}

// safeConnect – Mã giả 2.2 (rút gọn): net.DialTimeout thử lần lượt các địa chỉ, có thời gian chờ kết nối.
func safeConnect(host string, port int, timeoutMs int) (net.Conn, error) {
	addr := net.JoinHostPort(host, strconv.Itoa(port))
	conn, err := net.DialTimeout("tcp", addr, time.Duration(timeoutMs)*time.Millisecond)
	if err != nil {
		return nil, fmt.Errorf("Không kết nối được %s: %v", addr, err)
	}
	return conn, nil
}

// recvExact – Mã giả 3.4: gọi RECV lặp lại đến khi đủ n byte.
func recvExact(r io.Reader, n int) ([]byte, error) {
	buf := make([]byte, n)
	got := 0
	for got < n {
		k, err := r.Read(buf[got:]) // RECV
		got += k
		if err == io.EOF && got < n {
			return nil, &ConnectionClosed{Received: got}
		}
		if err != nil && got < n {
			return nil, err // lỗi mạng hoặc hết thời gian chờ
		}
	}
	return buf, nil
}

// sendJSON – Mã giả 3.6, SEND_JSON: đối tượng → JSON → UTF-8 → khung 4 byte độ dài (big-endian).
func sendJSON(w io.Writer, obj any) error {
	payload, err := json.Marshal(obj)
	if err != nil {
		return err
	}
	frame := make([]byte, 4+len(payload))
	binary.BigEndian.PutUint32(frame, uint32(len(payload)))
	copy(frame[4:], payload)
	_, err = w.Write(frame) // SEND_ALL: Write của Go chỉ trả về khi đã gửi hết hoặc gặp lỗi
	return err
}

// recvJSON – Mã giả 3.6, RECV_JSON (gồm RECV_FRAME). JSON sai gây JsonError.
func recvJSON(r io.Reader) (map[string]any, error) {
	head, err := recvExact(r, 4)
	if err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(head)
	if n > MAX_FRAME {
		return nil, &ProtocolError{Msg: fmt.Sprintf("Khung quá lớn: %d", n)}
	}
	data, err := recvExact(r, int(n))
	if cc, ok := err.(*ConnectionClosed); ok { // đã nhận xong tiêu đề
		return nil, &ConnectionClosed{Received: 4 + cc.Received}
	}
	if err != nil {
		return nil, err
	}
	var obj map[string]any // chỉ nhận đối tượng JSON
	if err := json.Unmarshal(data, &obj); err != nil || obj == nil {
		return nil, &JsonError{Msg: "Thông điệp không phải đối tượng JSON"}
	}
	return obj, nil
}
