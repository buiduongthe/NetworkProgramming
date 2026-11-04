// Thư viện định khung thông điệp trên dòng byte TCP (Bài 3.2).
//
// Cài đặt Mã giả 3.3 (SEND_ALL, Mục 3.2.4), Mã giả 3.4 (RECV_EXACT), Mã giả 3.5 (READ_LINE)
// và Mã giả 3.6 (SEND_FRAME, RECV_FRAME, SEND_JSON, RECV_JSON) – Mục 3.4.3 Giáo trình Lập trình mạng.
// Khung = 4 byte độ dài (big-endian, không dấu) + nội dung; thông điệp JSON mã hóa UTF-8.

package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"unicode/utf8"
)

// MaxFrame – CONST MAX_FRAME ← 1048576 (1 MiB).
const MaxFrame = 1048576

// ConnectionClosed – đối tác đóng kết nối; Received = số byte đã nhận của khung dở dang.
type ConnectionClosed struct{ Received int }

func (e *ConnectionClosed) Error() string {
	return fmt.Sprintf("Kết nối đóng giữa chừng (đã nhận %d byte)", e.Received)
}

// ProtocolError – đối tác vi phạm định dạng giao thức (khung quá lớn, dòng quá dài...).
type ProtocolError struct{ Msg string }

func (e *ProtocolError) Error() string { return e.Msg }

// JsonError – nội dung khung không phải JSON hợp lệ mã hóa UTF-8.
type JsonError struct{ Msg string }

func (e *JsonError) Error() string { return e.Msg }

// SendAll – Mã giả 3.3: SEND có thể gửi ít hơn yêu cầu nên lặp đến khi hết.
// (conn.Write của Go đã tự lặp như vậy; vòng lặp được giữ lại để đối chiếu với mã giả.)
func SendAll(conn net.Conn, data []byte) error {
	sent := 0
	for sent < len(data) {
		n, err := conn.Write(data[sent:]) // có thể gửi ít hơn yêu cầu
		if err != nil {
			return err
		}
		sent += n
	}
	return nil
}

// RecvExact – Mã giả 3.4: nhận đúng n byte; báo ConnectionClosed kèm số byte đã nhận nếu đóng giữa chừng.
func RecvExact(conn net.Conn, n int) ([]byte, error) {
	buf := make([]byte, n)
	got := 0
	for got < n { // WHILE LENGTH(buf) < n
		k, err := conn.Read(buf[got:]) // RECV(s, n - LENGTH(buf))
		got += k
		if err != nil && got < n {
			if errors.Is(err, io.EOF) { // LENGTH(chunk) = 0: đối tác đã đóng
				return nil, &ConnectionClosed{Received: got}
			}
			return nil, err // lỗi mạng khác (hết thời gian chờ, RST...)
		}
	}
	return buf, nil
}

// Reader – Mã giả 3.5: STRUCT Reader { sock, pending } – bộ đệm tồn tại giữa các lần READ_LINE.
type Reader struct {
	Sock    net.Conn
	Pending []byte
}

// ReadLine – Mã giả 3.5: đọc một dòng kết thúc bằng LF hoặc CRLF; ok = false khi đối tác đã đóng (NULL).
func ReadLine(r *Reader, maxLen int) (line string, ok bool, err error) {
	for {
		i := bytes.IndexByte(r.Pending, 0x0A) // FIND(r.pending, BYTE(0x0A)): vị trí '\n'
		if i > maxLen {
			return "", false, &ProtocolError{"Dòng quá dài"}
		}
		if i >= 0 {
			raw := r.Pending[:i]
			r.Pending = r.Pending[i+1:]
			raw = bytes.TrimSuffix(raw, []byte{0x0D}) // STRIP_CR
			if !utf8.Valid(raw) {                     // DECODE(…, "UTF-8")
				return "", false, &ProtocolError{"Dòng không phải UTF-8"}
			}
			return string(raw), true, nil
		}
		if len(r.Pending) > maxLen {
			return "", false, &ProtocolError{"Dòng quá dài"}
		}
		chunk := make([]byte, 4096)
		k, err := r.Sock.Read(chunk) // RECV(r.sock, 4096)
		if k == 0 && err != nil {
			if errors.Is(err, io.EOF) {
				return "", false, nil // đối tác đã đóng: RETURN NULL
			}
			return "", false, err
		}
		r.Pending = append(r.Pending, chunk[:k]...)
	}
}

// SendFrame – Mã giả 3.6: ghép 4 byte độ dài với nội dung rồi gửi một lần.
func SendFrame(conn net.Conn, payload []byte) error {
	frame := make([]byte, 4, 4+len(payload))
	binary.BigEndian.PutUint32(frame, uint32(len(payload))) // UINT32_BE(LENGTH(payload))
	frame = append(frame, payload...)
	return SendAll(conn, frame)
}

// RecvFrame – Mã giả 3.6: nhận đủ tiêu đề, kiểm tra MAX_FRAME, nhận đúng n byte nội dung.
func RecvFrame(conn net.Conn) ([]byte, error) {
	head, err := RecvExact(conn, 4)
	if err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(head) // FROM_UINT32_BE
	if n > MaxFrame {
		return nil, &ProtocolError{fmt.Sprintf("Khung quá lớn: %d", n)}
	}
	payload, err := RecvExact(conn, int(n))
	var cc *ConnectionClosed
	if errors.As(err, &cc) { // đã nhận xong tiêu đề
		return nil, &ConnectionClosed{Received: 4 + cc.Received}
	}
	return payload, err
}

// JsonEncode – JSON_ENCODE: giữ nguyên chữ tiếng Việt và ký tự <, >, & (không thoát \uXXXX), không thêm khoảng trắng.
func JsonEncode(obj any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(obj); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil // Encode thêm '\n' ở cuối
}

// JsonDecode – DECODE(…, "UTF-8") rồi JSON_DECODE; mọi lỗi giải mã đều quy về JsonError.
func JsonDecode(payload []byte) (map[string]any, error) {
	if !utf8.Valid(payload) {
		return nil, &JsonError{"Nội dung không phải UTF-8"}
	}
	var v any
	if err := json.Unmarshal(payload, &v); err != nil {
		return nil, &JsonError{err.Error()}
	}
	obj, ok := v.(map[string]any) // giao thức của giáo trình luôn dùng đối tượng JSON
	if !ok {
		return nil, &JsonError{"JSON không phải đối tượng"}
	}
	return obj, nil
}

// SendJSON – Mã giả 3.6, SEND_JSON.
func SendJSON(conn net.Conn, obj any) error {
	payload, err := JsonEncode(obj)
	if err != nil {
		return err
	}
	return SendFrame(conn, payload)
}

// RecvJSON – Mã giả 3.6, RECV_JSON.
func RecvJSON(conn net.Conn) (map[string]any, error) {
	payload, err := RecvFrame(conn)
	if err != nil {
		return nil, err
	}
	return JsonDecode(payload)
}

// GetString lấy trường chuỗi của đối tượng JSON; ok = false nếu thiếu hoặc không phải chuỗi.
func GetString(obj map[string]any, key string) (string, bool) {
	v, ok := obj[key].(string)
	return v, ok
}
