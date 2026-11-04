// Bài mở rộng MR6.4 – Giao thức trò chơi Caro: khung JSON có tiền tố độ dài (chép từ MR3.3, Bài 3.2).
//
// Khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8. Mỗi thông điệp có trường "type":
//
//	client -> server: hello {name}, list, create {room}, join {room}, watch {room}, move {r, c},
//	                  rematch, leave, bye
//	server -> client: welcome, rooms, joined, start, board, moved, over, rematch_wait, left,
//	                  room_closed, error {code, msg}
//
// Khám phá server trong LAN (UDP broadcast, như Bài 6.5):
//
//	client phát {"type": "discover", "service": "caro", "id": n}
//	server trả unicast {"id": n, "name", "port", "rooms", "players"}
package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand/v2"
	"net"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"time"
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

const discService = "caro"

type found struct {
	ip             string
	port           int
	name           string
	rooms, players int
}

// discover – Mã giả 6.7 (DISCOVER, Bài 6.5): phát một yêu cầu broadcast, thu thập MỌI phản hồi đến hết
// thời gian chờ. Danh sách được sắp theo tên để kết quả ổn định.
func discover(discPort, waitMs int, bcast string) ([]found, error) {
	lc := net.ListenConfig{Control: setOpt(syscall.SO_BROADCAST)} // bắt buộc để gửi tới địa chỉ broadcast
	pc, err := lc.ListenPacket(context.Background(), "udp4", "0.0.0.0:0")
	if err != nil {
		return nil, err
	}
	defer pc.Close()
	qid := rand.IntN(999999) + 1
	q, _ := json.Marshal(Msg{"type": "discover", "service": discService, "id": qid})
	dst, err := net.ResolveUDPAddr("udp4", net.JoinHostPort(bcast, strconv.Itoa(discPort)))
	if err != nil {
		return nil, err
	}
	if _, err := pc.WriteTo(q, dst); err != nil {
		return nil, err
	}
	var list []found
	pc.SetReadDeadline(time.Now().Add(time.Duration(waitMs) * time.Millisecond))
	buf := make([]byte, 2048)
	for {
		n, addr, err := pc.ReadFrom(buf)
		if err != nil {
			break // hết thời gian chờ
		}
		r := decodeJSON(buf[:n])
		if r == nil || num(r, "id") != qid { // chỉ nhận phản hồi đúng yêu cầu của mình
			continue
		}
		list = append(list, found{addr.(*net.UDPAddr).IP.String(), num(r, "port"), str(r, "name"),
			num(r, "rooms"), num(r, "players")})
	}
	sort.Slice(list, func(i, j int) bool {
		a, b := list[i], list[j]
		if a.name != b.name {
			return a.name < b.name
		}
		if a.ip != b.ip {
			return a.ip < b.ip
		}
		return strconv.Itoa(a.port) < strconv.Itoa(b.port)
	})
	return list, nil
}

// setOpt trả về hàm Control cho net.ListenConfig: bật một tùy chọn SOL_SOCKET ngay sau SOCKET, trước BIND
// (chép từ Bài 6.5).
func setOpt(opt int) func(network, address string, c syscall.RawConn) error {
	return func(network, address string, c syscall.RawConn) error {
		var err error
		c.Control(func(fd uintptr) {
			err = setsockoptInt(fd, syscall.SOL_SOCKET, opt, 1)
		})
		return err
	}
}

// nameOrNil – tên người chơi, hoặc null trong JSON nếu ghế trống.
func nameOrNil(c *client) any {
	if c == nil {
		return nil
	}
	return c.name
}
