// Bài 7.3 – Server WebSocket tối giản.
//
// Cài đặt Mã giả 7.3 (WS_ACCEPT – bắt tay) và Mã giả 7.4 (WS_READ_FRAME – đọc khung), Mục 7.3.2.
// Kế thừa: Mã giả 7.2 (đọc dòng đầu, READ_HEADERS, RESPOND), 3.4 (READ_EXACT), 6.3 (mỗi kết nối một tác vụ).
// Server gửi trả khung văn bản "echo: …", trả lời ping bằng pong, đáp khung đóng; khung vi phạm bị đóng
// với mã 1009 (quá lớn) hoặc 1002 (khung từ client không che).
//
// Chạy:  out/app server [host] [port]      (mặc định 0.0.0.0 8080)
package main

import (
	"bufio"
	"crypto/sha1"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"strings"
)

const WS_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
const MAX_FRAME = 1048576 // 1 MiB (Mã giả 3.6)

// ProtocolError: khung vi phạm đặc tả WebSocket.
type ProtocolError struct{ Msg string }

func (e *ProtocolError) Error() string { return e.Msg }

func readLineAsync(r *bufio.Reader) (string, error) {
	line, err := r.ReadSlice('\n')
	if errors.Is(err, bufio.ErrBufferFull) || len(line) > 8192 {
		return "", &ProtocolError{"Dòng quá dài"}
	}
	if err != nil {
		return "", err
	}
	return strings.TrimRight(string(line), "\r\n"), nil
}

func readHeadersAsync(r *bufio.Reader) (map[string]string, error) {
	hdr := map[string]string{}
	for {
		line, err := readLineAsync(r)
		if err != nil {
			return nil, err
		}
		if line == "" {
			return hdr, nil
		}
		k, v, _ := strings.Cut(line, ":")
		hdr[strings.ToLower(strings.TrimSpace(k))] = strings.TrimSpace(v)
	}
}

// respond400 – RESPOND(conn, 400, PROBLEM(400, title)) của Mã giả 7.2.
func respond400(w io.Writer, title string) error {
	b := `{"type":"about:blank","title":"` + title + `","status":400}`
	_, err := w.Write([]byte("HTTP/1.1 400 Bad Request\r\nContent-Type: application/problem+json; charset=utf-8\r\n" +
		"Content-Length: " + strconv.Itoa(len(b)) + "\r\n\r\n" + b))
	return err
}

// acceptKey: BASE64(SHA1(ENCODE(key + WS_GUID, "ASCII"))).
func acceptKey(key string) string {
	sum := sha1.Sum([]byte(key + WS_GUID))
	return base64.StdEncoding.EncodeToString(sum[:])
}

// wsAccept – Mã giả 7.3: kiểm tra yêu cầu nâng cấp và gửi 101 kèm Sec-WebSocket-Accept.
func wsAccept(conn net.Conn, hdr map[string]string) (bool, error) {
	key := hdr["sec-websocket-key"]                  // key ← hdr["sec-websocket-key"] OR ""
	raw, err := base64.StdEncoding.DecodeString(key) // khóa hợp lệ: Base64 của đúng 16 byte
	if strings.ToLower(hdr["upgrade"]) != "websocket" || hdr["sec-websocket-version"] != "13" ||
		err != nil || len(raw) != 16 {
		return false, respond400(conn, "Sai WebSocket")
	}
	acc := acceptKey(key)
	h := "HTTP/1.1 101 Switching Protocols\r\n"
	h += "Upgrade: websocket\r\nConnection: Upgrade\r\n"
	h += "Sec-WebSocket-Accept: " + acc + "\r\n\r\n"
	_, err = conn.Write([]byte(h)) // SEND_ALL_ASYNC
	return err == nil, err         // từ đây trao đổi bằng khung WebSocket
}

// wsSend: gửi một khung từ server – FIN = 1, không che (server không che dữ liệu).
func wsSend(w io.Writer, opcode byte, data []byte) error {
	n := len(data)
	var head []byte
	switch {
	case n < 126:
		head = []byte{0x80 | opcode, byte(n)}
	case n < 65536:
		head = []byte{0x80 | opcode, 126, 0, 0}
		binary.BigEndian.PutUint16(head[2:], uint16(n))
	default:
		head = make([]byte, 10)
		head[0], head[1] = 0x80|opcode, 127
		binary.BigEndian.PutUint64(head[2:], uint64(n))
	}
	_, err := w.Write(append(head, data...))
	return err
}

// wsClose – WS_CLOSE(s, code): khung opcode 0x8 chứa mã đóng 2 byte.
func wsClose(w io.Writer, code uint16) error {
	return wsSend(w, 0x8, binary.BigEndian.AppendUint16(nil, code))
}

// wsReadFrame – Mã giả 7.4: đọc một khung WebSocket.
func wsReadFrame(r io.Reader, w io.Writer, fromClient bool) (bool, byte, []byte, error) {
	b := make([]byte, 2)
	if _, err := io.ReadFull(r, b); err != nil { // READ_EXACT_ASYNC(r, 2)
		return false, 0, nil, err
	}
	fin := b[0]&0x80 != 0
	opcode := b[0] & 0x0F
	masked := b[1]&0x80 != 0
	n := uint64(b[1] & 0x7F)
	if n == 126 {
		ext := make([]byte, 2)
		if _, err := io.ReadFull(r, ext); err != nil {
			return false, 0, nil, err
		}
		n = uint64(binary.BigEndian.Uint16(ext)) // FROM_UINT16_BE
	} else if n == 127 {
		ext := make([]byte, 8)
		if _, err := io.ReadFull(r, ext); err != nil {
			return false, 0, nil, err
		}
		n = binary.BigEndian.Uint64(ext) // FROM_UINT64_BE
	}
	if n > MAX_FRAME { // kể cả khi bit cao = 1
		wsClose(w, 1009) // 1009: thông điệp quá lớn
		return false, 0, nil, &ProtocolError{"Khung quá lớn"}
	}
	if fromClient && !masked {
		wsClose(w, 1002) // 1002: vi phạm giao thức
		return false, 0, nil, &ProtocolError{"Khung client phải được che"}
	}
	var mask []byte
	if masked {
		mask = make([]byte, 4)
		if _, err := io.ReadFull(r, mask); err != nil {
			return false, 0, nil, err
		}
	}
	data := make([]byte, n)
	if _, err := io.ReadFull(r, data); err != nil { // READ_EXACT_ASYNC(r, n)
		return false, 0, nil, err
	}
	if masked {
		for i := range data {
			data[i] ^= mask[i%4] // XOR với byte khóa ở vị trí i MOD 4
		}
	}
	return fin, opcode, data, nil
}

func closeCode(data []byte) uint16 {
	if len(data) >= 2 {
		return binary.BigEndian.Uint16(data)
	}
	return 1005 // khung đóng không có mã
}

// session: một kết nối – bắt tay rồi vòng đọc khung.
func session(conn net.Conn) {
	addr := conn.RemoteAddr().String()
	defer func() {
		conn.Close()
		fmt.Printf("[server] Đóng kết nối %s\n", addr)
	}()
	r := bufio.NewReader(conn)
	err := func() error {
		line, err := readLineAsync(r)
		if err != nil {
			return err
		}
		hdr, err := readHeadersAsync(r)
		if err != nil {
			return err
		}
		ok, err := wsAccept(conn, hdr)
		if !ok {
			fmt.Printf("[server] %s %s -> 400 (không phải yêu cầu WebSocket)\n", addr, line)
			return err
		}
		fmt.Printf("[server] Bắt tay WebSocket thành công với %s\n", addr)
		var message []byte
		var msgType byte
		for {
			fin, opcode, data, err := wsReadFrame(r, conn, true)
			if err != nil {
				return err
			}
			switch opcode {
			case 0x8: // đóng: gửi lại mã đóng rồi kết thúc
				code := closeCode(data)
				fmt.Printf("[server] %s gửi khung đóng, mã %d\n", addr, code)
				if code == 1005 {
					code = 1000
				}
				return wsClose(conn, code)
			case 0x9: // ping -> pong cùng dữ liệu
				fmt.Printf("[server] Nhận ping: '%s' -> gửi pong\n", data)
				if err := wsSend(conn, 0xA, data); err != nil {
					return err
				}
				continue
			case 0x1, 0x2:
				message, msgType = data, opcode
			case 0x0: // khung tiếp nối
				message = append(message, data...)
			}
			if !fin {
				continue
			}
			if msgType == 0x1 {
				text := []rune(string(message))
				fmt.Printf("[server] Nhận văn bản (%d byte): '%s'\n", len(message), string(text[:min(40, len(text))]))
				err = wsSend(conn, 0x1, []byte("echo: "+string(message)))
			} else {
				fmt.Printf("[server] Nhận nhị phân (%d byte)\n", len(message))
				err = wsSend(conn, 0x2, message)
			}
			if err != nil {
				return err
			}
		}
	}()
	var pe *ProtocolError
	if errors.As(err, &pe) {
		fmt.Printf("[server] Lỗi giao thức với %s: %v\n", addr, err)
	} else if err != nil {
		fmt.Printf("[server] Kết nối %s đóng đột ngột: %v\n", addr, err)
	}
}

func serverMain(args []string) {
	host := "0.0.0.0"
	if len(args) >= 1 {
		host = args[0]
	}
	port := portArg(args, 1, 8080)
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server] Không mở được cổng: " + err.Error())
	}
	defer srv.Close()
	onInterrupt("[server] Dừng server")
	fmt.Printf("[server] Đang lắng nghe tại %s:%d\n", host, port)
	for {
		conn, err := srv.Accept()
		if err != nil {
			continue
		}
		go session(conn) // mỗi kết nối một tác vụ
	}
}
