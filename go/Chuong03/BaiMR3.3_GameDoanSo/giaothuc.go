// Bài mở rộng MR3.3 – Giao thức trò chơi đoán số: khung JSON có tiền tố độ dài, bộ sinh số giả ngẫu nhiên.
//
// Kế thừa Bài 3.2 (Mã giả 3.3–3.6): khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8.
// Các thông điệp (mỗi thông điệp là một đối tượng JSON có trường "type"):
//
//	client -> server: hello {name}, guess {value}, bye
//	server -> client: welcome, start, turn, result, timeout, left, error {code, msg}, game_over, bye
package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
)

// MaxFrame – thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ.
const MaxFrame = 64 * 1024

// errClosed – đối tác đóng kết nối.
var errClosed = errors.New("kết nối đã đóng")

// recvExact – Mã giả 3.4 (RECV_EXACT): nhận đúng n byte.
func recvExact(conn net.Conn, n int) ([]byte, error) {
	buf := make([]byte, n)
	if _, err := io.ReadFull(conn, buf); err != nil {
		if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
			return nil, errClosed
		}
		return nil, err
	}
	return buf, nil
}

// sendFrame – Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi trong một lần Write.
func sendFrame(conn net.Conn, payload []byte) error {
	frame := make([]byte, 4, 4+len(payload))
	binary.BigEndian.PutUint32(frame, uint32(len(payload)))
	_, err := conn.Write(append(frame, payload...))
	return err
}

// recvFrame – Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte.
func recvFrame(conn net.Conn) ([]byte, error) {
	head, err := recvExact(conn, 4)
	if err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(head)
	if n > MaxFrame {
		return nil, fmt.Errorf("Khung quá lớn: %d byte", n)
	}
	return recvExact(conn, int(n))
}

// Msg – một thông điệp JSON.
type Msg = map[string]any

func sendJSON(conn net.Conn, obj Msg) error {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false) // giữ nguyên chữ tiếng Việt và ký tự <, >, &
	if err := enc.Encode(obj); err != nil {
		return err
	}
	return sendFrame(conn, bytes.TrimSuffix(buf.Bytes(), []byte("\n")))
}

// decodeJSON giải mã nội dung khung; trả nil nếu không phải đối tượng JSON hợp lệ.
// UseNumber: số giữ dạng văn bản để phân biệt 50 (số nguyên) với 50.5 hay 5e1.
func decodeJSON(payload []byte) Msg {
	dec := json.NewDecoder(bytes.NewReader(payload))
	dec.UseNumber()
	var obj Msg
	if dec.Decode(&obj) != nil || dec.More() {
		return nil
	}
	return obj
}

func recvJSON(conn net.Conn) (Msg, error) {
	payload, err := recvFrame(conn)
	if err != nil {
		return nil, err
	}
	return decodeJSON(payload), nil
}

// str, num: đọc trường chuỗi / số nguyên của thông điệp (giá trị mặc định nếu sai kiểu).
func str(m Msg, key string) string {
	s, _ := m[key].(string)
	return s
}

func num(m Msg, key string) int {
	v, _ := asInt(m[key])
	return v
}

// asInt: giá trị JSON có phải số nguyên không (true/false, chuỗi, 50.5 đều không phải).
func asInt(v any) (int, bool) {
	switch x := v.(type) {
	case json.Number:
		n, err := strconv.Atoi(x.String())
		return n, err == nil
	case int:
		return x, true
	}
	return 0, false
}

// repr – in giá trị giống repr() của Python để nhật ký hai bản trùng nhau.
func repr(v any) string {
	switch x := v.(type) {
	case nil:
		return "None"
	case string:
		return "'" + x + "'"
	case bool:
		if x {
			return "True"
		}
		return "False"
	}
	return fmt.Sprint(v)
}

func joinNames(list []any) string {
	parts := make([]string, len(list))
	for i, v := range list {
		parts[i], _ = v.(string)
	}
	return strings.Join(parts, ", ")
}

// lcg – bộ sinh số giả ngẫu nhiên đồng dư tuyến tính rất đơn giản, cùng công thức với bản Python
// nên với cùng hạt giống (seed) cho cùng dãy số. Không dùng cho mục đích bảo mật.
type lcg struct{ x uint32 }

func newLcg(seed int64) *lcg { return &lcg{uint32(seed) & 0x7FFFFFFF} }

// nextInt – số nguyên ngẫu nhiên trong [lo, hi].
func (r *lcg) nextInt(lo, hi int) int {
	r.x = (r.x*1103515245 + 12345) & 0x7FFFFFFF
	return lo + int(r.x>>8)%(hi-lo+1)
}

// splitArgs tách tham số vị trí (host, port...) khỏi các cờ --ten giá_trị.
func splitArgs(args []string) (pos, flags []string) {
	for i, a := range args {
		if strings.HasPrefix(a, "-") {
			return pos, args[i:]
		}
		pos = append(pos, a)
	}
	return pos, nil
}

func argOr(pos []string, i int, def string) string {
	if i < len(pos) {
		return pos[i]
	}
	return def
}
