// Bài mở rộng MR6.4 – Server Caro nhiều phòng trên TCP, khám phá trong LAN bằng UDP broadcast.
//
// Thiết kế (mở rộng từ MR3.3):
//   - Goroutine chấp nhận + một goroutine đọc cho mỗi kết nối: chỉ nhận khung JSON, gửi sự kiện vào kênh.
//   - Goroutine khám phá (RESPONDER của Bài 6.5): nhận yêu cầu discover trên UDP, gửi sự kiện vào kênh.
//   - MỘT goroutine trò chơi giữ toàn bộ trạng thái (sảnh, các phòng, bàn cờ), xử lý tuần tự từng sự kiện.
//     Hạn chót của nước đi trong mọi phòng được gom lại: chờ kênh đến hạn chót sớm nhất.
//   - Server là trọng tài: kiểm tra lượt, tọa độ, ô trống, phát hiện thắng; client chỉ hiển thị.
//   - Mỗi nước đi hợp lệ được phát (broadcast) tới cả hai người chơi và mọi người xem trong phòng.
//
// Trạng thái của một phòng: CHO (thiếu người) -> DANG_CHOI -> KET_THUC -(cả hai xin chơi lại)-> DANG_CHOI.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"net"
	"os"
	"slices"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const (
	roomWaiting  = "CHO"
	roomPlaying  = "DANG_CHOI"
	roomFinished = "KET_THUC"
)

var reasonText = map[string]string{"five": "5 quân liên tiếp", "timeout": "đối thủ hết giờ",
	"left": "đối thủ rời phòng", "draw": "hòa (kín bàn)"}

var errorText = map[string]string{
	"BAD_JSON":         "Khung không phải đối tượng JSON",
	"NEED_HELLO":       "Phải gửi hello trước",
	"BAD_NAME":         "Tên phải là chuỗi 1–16 ký tự",
	"NAME_TAKEN":       "Tên đã có người dùng",
	"UNKNOWN_TYPE":     "Loại thông điệp không hỗ trợ",
	"ALREADY_IN_ROOM":  "Bạn đang ở trong một phòng",
	"BAD_ROOM_NAME":    "Tên phòng phải là chuỗi 1–16 ký tự",
	"ROOM_EXISTS":      "Phòng đã tồn tại",
	"NO_SUCH_ROOM":     "Không có phòng này",
	"ROOM_FULL":        "Phòng đã đủ hai người chơi",
	"NOT_IN_ROOM":      "Bạn chưa vào phòng nào",
	"NOT_PLAYER":       "Người xem không được đánh",
	"GAME_NOT_RUNNING": "Ván chưa bắt đầu hoặc đã kết thúc",
	"NOT_YOUR_TURN":    "Chưa đến lượt bạn",
	"BAD_MOVE":         "Nước đi phải có r, c là số nguyên",
	"OUT_OF_BOARD":     "Ô nằm ngoài bàn cờ",
	"CELL_TAKEN":       "Ô đã có quân",
}

// event – sự kiện gửi tới goroutine trò chơi: "open", "msg", "closed", "discover".
type event struct {
	kind string
	conn net.Conn
	addr string
	udp  net.Addr
	msg  Msg
}

// ---------------------------------------------------------------- các goroutine nhận
func reader(conn net.Conn, events chan<- event) {
	for {
		payload, err := recvFrame(conn)
		if err != nil {
			events <- event{kind: "closed", conn: conn}
			return
		}
		events <- event{kind: "msg", conn: conn, msg: decodeJSON(payload)}
	}
}

func acceptor(ln net.Listener, events chan<- event) {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		events <- event{kind: "open", conn: conn, addr: conn.RemoteAddr().String()}
		go reader(conn, events)
	}
}

// responder – RESPONDER (Mã giả 6.7): nhận discover, để goroutine trò chơi trả lời (nó mới biết số phòng).
func responder(pc net.PacketConn, events chan<- event) {
	buf := make([]byte, 2048)
	for {
		n, addr, err := pc.ReadFrom(buf)
		if err != nil {
			return
		}
		q := decodeJSON(buf[:n])
		if q != nil && str(q, "type") == "discover" && str(q, "service") == discService {
			events <- event{kind: "discover", udp: addr, msg: q}
		}
	}
}

// ---------------------------------------------------------------- trạng thái
type client struct {
	conn       net.Conn
	addr, name string
	room       string // tên phòng đang ở ("" = sảnh)
	role       string // "X", "O" hoặc "xem"
}

type room struct {
	name     string
	seats    map[string]*client // hai người chơi
	watchers []*client
	board    *board
	state    string
	turn     string
	moves    int
	gameNo   int
	deadline time.Time // zero = không có hạn chót
	rematch  map[string]bool
}

func newRoom(name string) *room {
	return &room{name: name, seats: map[string]*client{"X": nil, "O": nil}, board: newBoard(),
		state: roomWaiting, turn: "X", rematch: map[string]bool{}}
}

func (r *room) members() []*client {
	var list []*client
	for _, m := range []string{"X", "O"} {
		if r.seats[m] != nil {
			list = append(list, r.seats[m])
		}
	}
	return append(list, r.watchers...)
}

func (r *room) summary() Msg {
	return Msg{"room": r.name, "x": nameOrNil(r.seats["X"]), "o": nameOrNil(r.seats["O"]),
		"watchers": len(r.watchers), "state": r.state}
}

func other(mark string) string {
	if mark == "X" {
		return "O"
	}
	return "X"
}

type server struct {
	name      string
	port      int
	timeoutMs int
	udp       net.PacketConn
	clients   map[net.Conn]*client
	rooms     map[string]*room
	order     []string // thứ tự tạo phòng (map của Go không giữ thứ tự)
}

// ----- gửi -----
func (s *server) send(c *client, obj Msg) { sendJSON(c.conn, obj) }

func (s *server) toRoom(r *room, obj Msg) {
	for _, c := range r.members() {
		s.send(c, obj)
	}
}

func (s *server) fail(c *client, code, extra string) {
	who := c.name
	if who == "" {
		who = c.addr
	}
	fmt.Printf("[server] Từ chối %s: %s%s\n", who, code, extra)
	s.send(c, Msg{"type": "error", "code": code, "msg": errorText[code]})
}

func (s *server) roomList() []any {
	list := []any{}
	for _, name := range s.order {
		list = append(list, s.rooms[name].summary())
	}
	return list
}

// ----- ván cờ -----
func (s *server) deadline() time.Time {
	return time.Now().Add(time.Duration(s.timeoutMs) * time.Millisecond)
}

func (s *server) startGame(r *room) {
	r.gameNo++
	r.board, r.turn, r.moves = newBoard(), "X", 0
	r.state, r.rematch = roomPlaying, map[string]bool{}
	r.deadline = s.deadline()
	x, o := r.seats["X"].name, r.seats["O"].name
	fmt.Printf("[server] Phòng %s, ván %d: X = %s, O = %s\n", r.name, r.gameNo, x, o)
	s.toRoom(r, Msg{"type": "start", "room": r.name, "game_no": r.gameNo, "x": x, "o": o,
		"size": N, "timeout_ms": s.timeoutMs})
}

func (s *server) finish(r *room, winnerMark, reason string, line [][2]int) {
	r.state, r.deadline = roomFinished, time.Time{}
	var winner, mark any
	result := "hòa"
	if winnerMark != "" && r.seats[winnerMark] != nil {
		winner, mark = r.seats[winnerMark].name, winnerMark
		result = fmt.Sprintf("%s (%s) thắng", r.seats[winnerMark].name, winnerMark)
	}
	fmt.Printf("[server] Phòng %s, ván %d kết thúc sau %d nước: %s – %s\n",
		r.name, r.gameNo, r.moves, result, reasonText[reason])
	cells := []any{}
	for _, p := range line {
		cells = append(cells, []int{p[0], p[1]})
	}
	s.toRoom(r, Msg{"type": "over", "room": r.name, "game_no": r.gameNo, "winner": winner,
		"mark": mark, "reason": reason, "moves": r.moves, "line": cells})
}

func (s *server) onMove(c *client, m Msg) {
	r := s.rooms[c.room]
	switch {
	case r == nil:
		s.fail(c, "NOT_IN_ROOM", "")
		return
	case c.role == "xem":
		s.fail(c, "NOT_PLAYER", "")
		return
	case r.state != roomPlaying:
		s.fail(c, "GAME_NOT_RUNNING", "")
		return
	case c.role != r.turn:
		s.fail(c, "NOT_YOUR_TURN", "")
		return
	}
	row, ok1 := asInt(m["r"])
	col, ok2 := asInt(m["c"])
	if !ok1 || !ok2 {
		s.fail(c, "BAD_MOVE", "")
		return
	}
	if !inside(row, col) {
		s.fail(c, "OUT_OF_BOARD", fmt.Sprintf(" (%d, %d)", row, col))
		return
	}
	if r.board[row][col] != empty {
		s.fail(c, "CELL_TAKEN", " ("+cellName(row, col)+")")
		return
	}
	r.board[row][col] = c.role[0] // nước đi hợp lệ: ghi vào bàn cờ của server
	r.moves++
	line := fiveLine(r.board, row, col) // phán xử TRƯỚC khi báo nước đi
	ended := line != nil || r.moves == N*N
	var next any = other(c.role)
	if ended {
		next = nil
	}
	s.toRoom(r, Msg{"type": "moved", "room": r.name, "r": row, "c": col, "mark": c.role,
		"by": c.name, "move_no": r.moves, "next": next}) // next = null: ván đã hết
	if line != nil {
		s.finish(r, c.role, "five", line)
		return
	}
	if ended {
		s.finish(r, "", "draw", nil)
		return
	}
	r.turn = other(c.role)
	r.deadline = s.deadline() // mỗi nước có hạn chót riêng
}

func (s *server) onTimeout(r *room) {
	fmt.Printf("[server] Phòng %s: %s (%s) hết giờ\n", r.name, r.seats[r.turn].name, r.turn)
	s.finish(r, other(r.turn), "timeout", nil)
}

// ----- phòng -----
func (s *server) enter(c *client, r *room, role string) {
	c.room, c.role = r.name, role
	what := "quân " + role
	if role == "xem" {
		r.watchers = append(r.watchers, c)
		what = "người xem"
	} else {
		r.seats[role] = c
	}
	fmt.Printf("[server] %s vào phòng %s (%s)\n", c.name, r.name, what)
	s.send(c, Msg{"type": "joined", "room": r.name, "role": role, "info": r.summary()})
	if role == "xem" && r.state != roomWaiting { // người xem đến muộn: gửi ảnh chụp bàn cờ
		rows := make([]any, N)
		for i := range rows {
			rows[i] = string(r.board[i][:])
		}
		var turn any
		if r.state == roomPlaying {
			turn = r.turn
		}
		s.send(c, Msg{"type": "board", "room": r.name, "game_no": r.gameNo, "rows": rows,
			"moves": r.moves, "turn": turn, "state": r.state})
	}
}

func (s *server) leaveRoom(c *client) {
	r := s.rooms[c.room]
	if r == nil {
		return
	}
	role := c.role
	c.room, c.role = "", ""
	if role == "xem" {
		r.watchers = slices.DeleteFunc(r.watchers, func(w *client) bool { return w == c })
		return
	}
	if r.state == roomPlaying { // bỏ dở ván: đối thủ thắng
		s.finish(r, other(role), "left", nil)
	}
	r.seats[role] = nil
	fmt.Printf("[server] %s rời phòng %s\n", c.name, r.name)
	s.toRoom(r, Msg{"type": "left", "room": r.name, "player": c.name, "role": role})
	r.state, r.deadline, r.rematch = roomWaiting, time.Time{}, map[string]bool{}
	if r.seats["X"] == nil && r.seats["O"] == nil {
		fmt.Printf("[server] Phòng %s đóng (không còn người chơi)\n", r.name)
		for _, w := range r.watchers {
			w.room, w.role = "", ""
			s.send(w, Msg{"type": "room_closed", "room": r.name})
		}
		delete(s.rooms, r.name)
		s.order = slices.DeleteFunc(s.order, func(n string) bool { return n == r.name })
	}
}

// ----- xử lý thông điệp -----
func validName(v any) (string, bool) {
	name, ok := v.(string)
	name = strings.TrimSpace(name)
	n := len([]rune(name))
	return name, ok && n >= 1 && n <= 16
}

func (s *server) onMessage(c *client, m Msg) {
	if m == nil {
		s.fail(c, "BAD_JSON", "")
		return
	}
	kind := m["type"]
	if c.name == "" {
		if kind != "hello" {
			s.fail(c, "NEED_HELLO", "")
			return
		}
		name, ok := validName(m["name"])
		if !ok {
			s.fail(c, "BAD_NAME", "")
			return
		}
		for _, o := range s.clients {
			if o.name == name {
				s.fail(c, "NAME_TAKEN", " ("+name+")")
				return
			}
		}
		c.name = name
		fmt.Printf("[server] %s vào sảnh (%s)\n", c.name, c.addr)
		s.send(c, Msg{"type": "welcome", "name": c.name, "rooms": s.roomList()})
		return
	}
	switch kind {
	case "list":
		s.send(c, Msg{"type": "rooms", "rooms": s.roomList()})
	case "create", "join", "watch":
		s.onRoomRequest(c, kind.(string), m["room"])
	case "move":
		s.onMove(c, m)
	case "rematch":
		s.onRematch(c)
	case "leave":
		if c.room == "" {
			s.fail(c, "NOT_IN_ROOM", "")
			return
		}
		s.leaveRoom(c)
	case "bye":
		s.leaveRoom(c)
		fmt.Printf("[server] %s tạm biệt\n", c.name)
		c.name = ""                              // không còn trong sảnh; "closed" đến sau sẽ im lặng
		if tc, ok := c.conn.(*net.TCPConn); ok { // goroutine đọc sẽ báo "closed"
			tc.CloseRead()
			tc.CloseWrite()
		}
	default:
		s.fail(c, "UNKNOWN_TYPE", " ("+repr(kind)+")")
	}
}

func (s *server) onRoomRequest(c *client, kind string, v any) {
	if c.room != "" {
		s.fail(c, "ALREADY_IN_ROOM", "")
		return
	}
	name, ok := validName(v)
	if !ok {
		s.fail(c, "BAD_ROOM_NAME", "")
		return
	}
	r := s.rooms[name]
	if kind == "create" {
		if r != nil {
			s.fail(c, "ROOM_EXISTS", " ("+name+")")
			return
		}
		r = newRoom(name)
		s.rooms[name] = r
		s.order = append(s.order, name)
		fmt.Printf("[server] %s tạo phòng %s\n", c.name, name)
		s.enter(c, r, "X")
		return
	}
	if r == nil {
		s.fail(c, "NO_SUCH_ROOM", " ("+name+")")
		return
	}
	if kind == "watch" {
		s.enter(c, r, "xem")
		return
	}
	free := ""
	for _, m := range []string{"X", "O"} {
		if r.seats[m] == nil {
			free = m
			break
		}
	}
	if free == "" {
		s.fail(c, "ROOM_FULL", " ("+name+")")
		return
	}
	s.enter(c, r, free)
	if r.seats["X"] != nil && r.seats["O"] != nil {
		s.startGame(r)
	}
}

func (s *server) onRematch(c *client) {
	r := s.rooms[c.room]
	if r == nil || (c.role != "X" && c.role != "O") {
		s.fail(c, "NOT_PLAYER", "")
		return
	}
	if r.state != roomFinished {
		s.fail(c, "GAME_NOT_RUNNING", "")
		return
	}
	r.rematch[c.role] = true
	if len(r.rematch) < 2 {
		s.toRoom(r, Msg{"type": "rematch_wait", "room": r.name, "by": c.name})
		return
	}
	r.seats["X"], r.seats["O"] = r.seats["O"], r.seats["X"] // đổi màu quân: người kia đi trước
	for _, m := range []string{"X", "O"} {
		r.seats[m].role = m
	}
	s.startGame(r)
}

func (s *server) onDiscover(addr net.Addr, q Msg) {
	players := 0
	for _, c := range s.clients {
		if c.name != "" {
			players++
		}
	}
	id := q["id"]
	if n, ok := asInt(id); ok {
		id = n
	}
	reply, _ := json.Marshal(Msg{"id": id, "name": s.name, "port": s.port,
		"rooms": len(s.rooms), "players": players})
	fmt.Printf("[server] Trả lời discover từ %s\n", addr)
	s.udp.WriteTo(reply, addr)
}

func (s *server) onClosed(conn net.Conn) {
	c := s.clients[conn]
	if c == nil {
		return
	}
	delete(s.clients, conn)
	if c.name != "" {
		s.leaveRoom(c)
		fmt.Printf("[server] %s ngắt kết nối\n", c.name)
	}
	conn.Close()
}

// expiredRoom – phòng có hạn chót sớm nhất (hoặc nil).
func (s *server) expiredRoom() *room {
	var best *room
	for _, name := range s.order {
		r := s.rooms[name]
		if !r.deadline.IsZero() && (best == nil || r.deadline.Before(best.deadline)) {
			best = r
		}
	}
	return best
}

func runServer(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("server", flag.ExitOnError)
	disc := fs.Int("kham-pha", 9999, "cổng UDP nhận yêu cầu khám phá")
	name := fs.String("ten", "Caro", "tên server hiển thị khi khám phá")
	timeoutMs := fs.Int("het-gio-ms", 30000, "thời gian tối đa cho mỗi nước đi")
	fs.Parse(flags)
	host, portText := argOr(pos, 0, "0.0.0.0"), argOr(pos, 1, "9100")
	port, _ := strconv.Atoi(portText)

	ln, err := net.Listen("tcp4", net.JoinHostPort(host, portText))
	if err != nil {
		fmt.Println("[server] Không mở được cổng:", err)
		os.Exit(1)
	}
	defer ln.Close()
	lc := net.ListenConfig{Control: setOpt(syscall.SO_REUSEADDR)}
	udp, err := lc.ListenPacket(context.Background(), "udp4", "0.0.0.0:"+strconv.Itoa(*disc)) // nhận broadcast
	if err != nil {
		fmt.Println("[server] Không mở được cổng khám phá:", err)
		os.Exit(1)
	}
	defer udp.Close()
	fmt.Printf("[server] %s: Caro %d×%d tại %s:%d, khám phá UDP cổng %d, mỗi nước tối đa %d ms\n",
		*name, N, N, host, port, *disc, *timeoutMs)
	events := make(chan event, 64)
	go acceptor(ln, events)
	go responder(udp, events)
	onInterrupt("[server] Dừng server")
	s := &server{name: *name, port: port, timeoutMs: *timeoutMs, udp: udp,
		clients: map[net.Conn]*client{}, rooms: map[string]*room{}}
	for {
		var timeout <-chan time.Time // nil: không phòng nào có hạn chót
		if r := s.expiredRoom(); r != nil {
			if !time.Now().Before(r.deadline) { // có phòng hết giờ: xử lý trước
				s.onTimeout(r)
				continue
			}
			timeout = time.After(time.Until(r.deadline))
		}
		select {
		case <-timeout: // vòng sau sẽ thấy phòng hết giờ
		case ev := <-events:
			switch ev.kind {
			case "open":
				s.clients[ev.conn] = &client{conn: ev.conn, addr: ev.addr}
			case "msg":
				if c := s.clients[ev.conn]; c != nil {
					s.onMessage(c, ev.msg)
				}
			case "closed":
				s.onClosed(ev.conn)
			case "discover":
				s.onDiscover(ev.udp, ev.msg)
			}
		}
	}
}
