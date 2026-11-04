// Các hàm gửi, nhận khung JSON "bất đồng bộ" theo cách của Go – Mã giả 5.1, chép từ Bài 5.1.
//
// Kế thừa: Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.6 (khung JSON), 2.1 (PARSE_ENDPOINT),
// 2.2 (SAFE_CONNECT) – giữ nguyên logic, thêm tín hiệu hủy ct.
//
// Go không có từ khóa async/await. Tương tự luồng ảo của Java, mỗi tác vụ là một goroutine
// (luồng nhẹ do môi trường chạy Go quản lý) gọi các hàm vào/ra chặn thông thường; khi goroutine
// chờ mạng, môi trường chạy dùng epoll/kqueue/IOCP để cho goroutine khác chạy. Tín hiệu hủy ct là
// context.Context: khi ctx bị hủy, context.AfterFunc đặt deadline của kết nối về quá khứ để
// lời gọi Read/Write đang chờ kết thúc ngay, và hàm trả lỗi context.Canceled.

package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"os/signal"
	"strconv"
	"time"
	"unicode/utf8"
)

// MaxFrame – 1 MiB (Mã giả 3.6).
const MaxFrame = 1048576

// ConnectionClosed – đối tác đóng kết nối; Received = số byte của khung đã nhận được.
type ConnectionClosed struct{ Received int }

func (e *ConnectionClosed) Error() string {
	return fmt.Sprintf("Kết nối đã đóng (đã nhận %d byte)", e.Received)
}

// ProtocolError – vi phạm giao thức, ví dụ khung quá lớn.
type ProtocolError struct{ Msg string }

func (e *ProtocolError) Error() string { return e.Msg }

// JsonError – nội dung khung không phải đối tượng JSON hợp lệ mã hóa UTF-8.
type JsonError struct{ Msg string }

func (e *JsonError) Error() string { return e.Msg }

// NetError – lỗi mạng đã phân loại (Mã giả 2.2).
type NetError struct {
	Kind  string
	Retry bool
	Cause error
}

func (e *NetError) Error() string { return e.Kind + ": " + e.Cause.Error() }

// JsonBytes – ENCODE(JSON_ENCODE(obj), "UTF-8"): giữ nguyên chữ có dấu, không thêm khoảng trắng.
func JsonBytes(obj any) []byte {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.Encode(obj)
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n"))
}

// JsonDecode – JSON_DECODE(DECODE(data, "UTF-8")); lỗi UTF-8, lỗi cú pháp, không phải đối tượng → JsonError.
func JsonDecode(data []byte) (map[string]any, error) {
	if !utf8.Valid(data) {
		return nil, &JsonError{"Nội dung không phải UTF-8"}
	}
	var v any
	if err := json.Unmarshal(data, &v); err != nil {
		return nil, &JsonError{err.Error()}
	}
	obj, ok := v.(map[string]any)
	if !ok {
		return nil, &JsonError{"JSON không phải đối tượng"}
	}
	return obj, nil
}

// GetString lấy trường chuỗi của đối tượng JSON; ok = false nếu thiếu hoặc không phải chuỗi.
func GetString(obj map[string]any, key string) (string, bool) {
	v, ok := obj[key].(string)
	return v, ok
}

// GetInt lấy trường số nguyên của đối tượng JSON (số JSON được giải mã thành float64); 0 nếu thiếu.
func GetInt(obj map[string]any, key string) int {
	switch v := obj[key].(type) {
	case float64:
		return int(v)
	case int:
		return v
	}
	return 0
}

// watchCancel: khi ctx bị hủy, đặt deadline của conn về quá khứ để Read/Write đang chờ kết thúc ngay.
// Trả về hàm dừng theo dõi (gọi khi thao tác xong).
func watchCancel(ctx context.Context, conn net.Conn) func() bool {
	return context.AfterFunc(ctx, func() { conn.SetDeadline(time.Unix(1, 0)) })
}

// RecvExactAsync – Mã giả 5.1, dòng 1–10: RECV_EXACT_ASYNC(s, n, ct).
func RecvExactAsync(ctx context.Context, conn net.Conn, n int) ([]byte, error) {
	stop := watchCancel(ctx, conn)
	defer stop()
	buf := make([]byte, n)
	got := 0
	for got < n {
		k, err := conn.Read(buf[got:]) // AWAIT RECV_ASYNC(s, n - LENGTH(buf), ct): goroutine chờ, chương trình không bị chặn
		got += k
		if err != nil && got < n {
			if ctx.Err() != nil {
				return nil, ctx.Err() // ct báo hủy: Cancelled
			}
			if errors.Is(err, io.EOF) {
				return nil, &ConnectionClosed{Received: got} // đối tác đã đóng kết nối
			}
			return nil, err
		}
	}
	return buf, nil
}

// RecvJSONAsync – Mã giả 5.1, dòng 12–20: RECV_JSON_ASYNC(s, ct).
func RecvJSONAsync(ctx context.Context, conn net.Conn) (map[string]any, error) {
	head, err := RecvExactAsync(ctx, conn, 4)
	if err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(head) // FROM_UINT32_BE
	if n > MaxFrame {
		return nil, &ProtocolError{"Khung quá lớn"}
	}
	data, err := RecvExactAsync(ctx, conn, int(n))
	var cc *ConnectionClosed
	if errors.As(err, &cc) { // đã nhận xong tiêu đề
		return nil, &ConnectionClosed{Received: 4 + cc.Received}
	}
	if err != nil {
		return nil, err
	}
	return JsonDecode(data)
}

// MakeFrame – UINT32_BE(LENGTH(p)) + p.
func MakeFrame(obj any) []byte {
	p := JsonBytes(obj)
	return append(binary.BigEndian.AppendUint32(nil, uint32(len(p))), p...)
}

// SendJSONAsync – Mã giả 5.1, dòng 22–24: SEND_JSON_ASYNC(s, obj, ct).
func SendJSONAsync(ctx context.Context, conn net.Conn, obj any) error {
	stop := watchCancel(ctx, conn)
	defer stop()
	_, err := conn.Write(MakeFrame(obj)) // AWAIT SEND_ALL_ASYNC: Write của Go gửi hết hoặc trả lỗi
	if err != nil && ctx.Err() != nil {
		return ctx.Err()
	}
	return err
}

// ConfigError – lỗi cấu hình: giá trị địa chỉ hoặc cổng không dùng được.
type ConfigError struct{ Msg string }

func (e *ConfigError) Error() string { return e.Msg }

// ParseEndpoint – Mã giả 2.1: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > giá trị mặc định.
func ParseEndpoint(args []string, defaultHost string, defaultPort int) (string, int, error) {
	host, portText := defaultHost, strconv.Itoa(defaultPort)
	if v, ok := os.LookupEnv("APP_HOST"); ok {
		host = v
	}
	if v, ok := os.LookupEnv("APP_PORT"); ok {
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
		return "", 0, &ConfigError{"Cổng phải là số nguyên: " + portText}
	}
	if port < 1 || port > 65535 {
		return "", 0, &ConfigError{"Cổng không hợp lệ: " + portText}
	}
	return host, port, nil
}

// SafeConnectAsync – SAFE_CONNECT_ASYNC = Mã giả 2.2 + ct: thử lần lượt các địa chỉ, có thời gian chờ, phân loại lỗi.
func SafeConnectAsync(ctx context.Context, host string, port int, timeoutMs int) (net.Conn, error) {
	addrs, err := net.DefaultResolver.LookupIPAddr(ctx, host) // RESOLVE
	if err != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, &NetError{"NOT_FOUND", false, err}
	}
	refused := 0
	var lastError error
	for _, a := range addrs {
		network := "tcp4"
		if a.IP.To4() == nil {
			network = "tcp6"
		}
		d := net.Dialer{Timeout: time.Duration(timeoutMs) * time.Millisecond}
		conn, err := d.DialContext(ctx, network, net.JoinHostPort(a.IP.String(), strconv.Itoa(port)))
		if err == nil {
			return conn, nil
		}
		if ctx.Err() != nil {
			return nil, ctx.Err() // người dùng hủy: không thử địa chỉ khác
		}
		lastError = err
		if isRefused(err) {
			refused++
		}
	}
	if refused > 0 && refused == len(addrs) {
		return nil, &NetError{"REFUSED", false, lastError}
	}
	return nil, &NetError{"UNREACHABLE", true, lastError}
}

// StopOnCtrlC đóng socket lắng nghe khi người dùng nhấn Ctrl+C, để vòng ACCEPT kết thúc.
func StopOnCtrlC(srv io.Closer) {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		srv.Close()
	}()
}

// SendAllAsync – SEND_ALL_ASYNC(s, data, ct): gửi hết dữ liệu, dừng ngay khi ct bị hủy.
func SendAllAsync(ctx context.Context, conn net.Conn, data []byte) error {
	stop := watchCancel(ctx, conn)
	defer stop()
	_, err := conn.Write(data)
	if err != nil && ctx.Err() != nil {
		return ctx.Err()
	}
	return err
}
