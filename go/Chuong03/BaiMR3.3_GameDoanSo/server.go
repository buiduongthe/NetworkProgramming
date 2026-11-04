// Bài mở rộng MR3.3 – Server trò chơi đoán số nhiều người chơi trên TCP.
//
// Thiết kế:
//   - Mỗi kết nối có một goroutine đọc chỉ làm một việc: nhận khung JSON và gửi sự kiện vào kênh.
//   - MỘT goroutine trò chơi (vòng lặp sự kiện) giữ toàn bộ trạng thái, xử lý tuần tự từng sự kiện,
//     nên không cần mutex. Hết giờ mỗi lượt = nhánh time.After trong select.
//   - Máy trạng thái của server: CHO (chờ đủ người) -> DANG_CHOI -> (kết thúc ván) -> CHO.
//   - Server là bên quyết định: kiểm tra mọi thông điệp (đúng lượt, kiểu số, khoảng hợp lệ), không tin client.
package main

import (
	"flag"
	"fmt"
	"net"
	"os"
	"slices"
	"strings"
	"time"
)

const (
	stateWaiting = "CHO"       // chờ đủ người
	statePlaying = "DANG_CHOI" // đang chơi
)

var hintText = map[string]string{"THAP": "thấp quá", "CAO": "cao quá", "DUNG": "ĐÚNG"}

var errorText = map[string]string{
	"BAD_JSON":         "Khung không phải đối tượng JSON",
	"NEED_HELLO":       "Phải gửi hello trước",
	"BAD_NAME":         "Tên phải là chuỗi 1–16 ký tự",
	"NAME_TAKEN":       "Tên đã có người dùng",
	"GAME_IN_PROGRESS": "Ván đang diễn ra, hãy vào lại sau",
	"GAME_NOT_STARTED": "Ván chưa bắt đầu",
	"NOT_YOUR_TURN":    "Chưa đến lượt bạn",
	"BAD_VALUE":        "Giá trị đoán phải là số nguyên",
	"OUT_OF_RANGE":     "Số đoán nằm ngoài khoảng cho phép",
	"UNKNOWN_TYPE":     "Loại thông điệp không hỗ trợ",
}

// event – sự kiện gửi tới vòng trò chơi: "open" (kết nối mới), "msg" (một khung), "closed".
type event struct {
	kind string
	conn net.Conn
	addr string
	msg  Msg // nil nghĩa là khung hỏng
}

type config struct {
	need, timeoutMs, min, max, rounds int
	seed                              int64
}

// reader – goroutine đọc của một kết nối: chuyển mỗi khung thành sự kiện cho vòng trò chơi.
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

// acceptor – goroutine chấp nhận kết nối: mỗi client mới có một goroutine đọc riêng.
func acceptor(ln net.Listener, events chan<- event) {
	for {
		conn, err := ln.Accept()
		if err != nil {
			return // listener đã đóng
		}
		events <- event{kind: "open", conn: conn, addr: conn.RemoteAddr().String()}
		go reader(conn, events)
	}
}

// game – toàn bộ trạng thái trò chơi; chỉ goroutine trò chơi được đọc/ghi.
type game struct {
	cfg      config
	rng      *lcg
	state    string
	names    map[net.Conn]string // người đã gửi hello hợp lệ
	joined   []net.Conn          // thứ tự vào phòng (map của Go không giữ thứ tự)
	order    []net.Conn          // thứ tự lượt
	turn     int
	turnNo   int
	deadline time.Time // hạn chót của lượt hiện tại (zero = không có)
	secret   int
	lo, hi   int
	round    int
}

// ----- gửi -----
func (g *game) send(conn net.Conn, obj Msg) {
	sendJSON(conn, obj) // lỗi bỏ qua: goroutine đọc sẽ báo "closed"
}

func (g *game) broadcast(obj Msg) {
	for _, c := range g.joined {
		g.send(c, obj)
	}
}

func (g *game) fail(conn net.Conn, code, who, extra string) {
	fmt.Printf("[server] Từ chối %s: %s%s\n", who, code, extra)
	g.send(conn, Msg{"type": "error", "code": code, "msg": errorText[code]})
}

func (g *game) forget(conn net.Conn) {
	delete(g.names, conn)
	g.joined = slices.DeleteFunc(g.joined, func(c net.Conn) bool { return c == conn })
}

// drop – đóng hẳn kết nối (goroutine đọc sẽ thoát).
func (g *game) drop(conn net.Conn) {
	g.forget(conn)
	conn.Close()
}

// ----- máy trạng thái -----
func (g *game) startRound() {
	g.round++
	g.state = statePlaying
	g.secret = g.rng.nextInt(g.cfg.min, g.cfg.max)
	g.lo, g.hi = g.cfg.min, g.cfg.max
	g.order = slices.Clone(g.joined)
	g.turn, g.turnNo = 0, 0
	names := make([]any, len(g.order))
	for i, c := range g.order {
		names[i] = g.names[c]
	}
	fmt.Printf("[server] Bắt đầu ván %d: số bí mật = %d (chỉ server biết), thứ tự: %s\n",
		g.round, g.secret, joinNames(names))
	g.broadcast(Msg{"type": "start", "round": g.round, "order": names, "min": g.cfg.min, "max": g.cfg.max})
	g.nextTurn()
}

// nextTurn – giao lượt cho người kế tiếp và đặt hạn chót.
func (g *game) nextTurn() {
	g.turnNo++
	name := g.names[g.order[g.turn]]
	g.deadline = time.Now().Add(time.Duration(g.cfg.timeoutMs) * time.Millisecond)
	g.broadcast(Msg{"type": "turn", "player": name, "lo": g.lo, "hi": g.hi,
		"turn_no": g.turnNo, "timeout_ms": g.cfg.timeoutMs})
}

func (g *game) advance() {
	g.turn = (g.turn + 1) % len(g.order)
	g.nextTurn()
}

func (g *game) endRound(winner string) {
	var w any
	if winner != "" {
		w = winner
		fmt.Printf("[server] Kết thúc ván %d: %s thắng sau %d lượt\n", g.round, winner, g.turnNo)
	} else {
		fmt.Printf("[server] Kết thúc ván %d: không còn người chơi\n", g.round)
	}
	g.broadcast(Msg{"type": "game_over", "winner": w, "secret": g.secret, "turns": g.turnNo})
	for _, c := range slices.Clone(g.joined) { // mỗi ván một phiên: đóng mọi kết nối
		g.send(c, Msg{"type": "bye"})
		g.drop(c)
	}
	g.state, g.order, g.deadline = stateWaiting, nil, time.Time{}
}

func (g *game) removePlayer(conn net.Conn) {
	name := g.names[conn]
	g.forget(conn)
	fmt.Printf("[server] %s rời trò chơi (còn %d người)\n", name, len(g.names))
	g.broadcast(Msg{"type": "left", "player": name})
	if g.state != statePlaying {
		return
	}
	i := slices.Index(g.order, conn)
	wasCurrent := i == g.turn
	g.order = slices.Delete(g.order, i, i+1)
	if len(g.order) == 0 {
		g.endRound("")
		return
	}
	if i < g.turn {
		g.turn-- // giữ nguyên người đang có lượt
	}
	if wasCurrent {
		g.turn %= len(g.order)
		g.nextTurn()
	}
}

func (g *game) onTimeout() {
	name := g.names[g.order[g.turn]]
	fmt.Printf("[server] %s hết giờ ở lượt %d\n", name, g.turnNo)
	g.broadcast(Msg{"type": "timeout", "player": name})
	g.advance()
}

// ----- xử lý thông điệp -----
func (g *game) onMessage(conn net.Conn, addr string, msg Msg) {
	who, joined := g.names[conn]
	if !joined {
		who = addr
	}
	if msg == nil {
		g.fail(conn, "BAD_JSON", who, "")
		return
	}
	kind := msg["type"]
	if !joined { // chưa vào phòng: chỉ chấp nhận hello
		if kind != "hello" {
			g.fail(conn, "NEED_HELLO", who, "")
			return
		}
		g.onHello(conn, addr, msg)
		return
	}
	switch kind {
	case "guess":
		g.onGuess(conn, who, msg["value"])
	case "bye":
		g.removePlayer(conn)
		g.drop(conn)
	default:
		g.fail(conn, "UNKNOWN_TYPE", who, " ("+repr(kind)+")")
	}
}

func (g *game) onHello(conn net.Conn, addr string, msg Msg) {
	name, ok := msg["name"].(string)
	name = strings.TrimSpace(name)
	if n := len([]rune(name)); !ok || n < 1 || n > 16 {
		g.fail(conn, "BAD_NAME", addr, "")
		return
	}
	for _, other := range g.names {
		if other == name {
			g.fail(conn, "NAME_TAKEN", addr, " ("+name+")")
			return
		}
	}
	if g.state == statePlaying {
		g.fail(conn, "GAME_IN_PROGRESS", name, "")
		g.send(conn, Msg{"type": "bye"})
		g.drop(conn)
		return
	}
	g.names[conn] = name
	g.joined = append(g.joined, conn)
	fmt.Printf("[server] %s vào phòng chờ (%d/%d)\n", name, len(g.names), g.cfg.need)
	g.send(conn, Msg{"type": "welcome", "name": name, "waiting": len(g.names),
		"need": g.cfg.need, "min": g.cfg.min, "max": g.cfg.max})
	if len(g.names) >= g.cfg.need {
		g.startRound()
	}
}

func (g *game) onGuess(conn net.Conn, who string, value any) {
	if g.state != statePlaying {
		g.fail(conn, "GAME_NOT_STARTED", who, "")
		return
	}
	if g.order[g.turn] != conn {
		g.fail(conn, "NOT_YOUR_TURN", who, "")
		return
	}
	v, ok := asInt(value) // true/false, chuỗi, 50.5 không phải số nguyên
	if !ok {
		g.fail(conn, "BAD_VALUE", who, " ("+repr(value)+")")
		return
	}
	if v < g.lo || v > g.hi {
		g.fail(conn, "OUT_OF_RANGE", who, fmt.Sprintf(" (%d ngoài %d–%d)", v, g.lo, g.hi))
		return
	}
	hint := "CAO"
	if v == g.secret {
		hint = "DUNG"
	} else if v < g.secret {
		hint = "THAP"
	}
	fmt.Printf("[server] Lượt %d: %s đoán %d -> %s\n", g.turnNo, who, v, hintText[hint])
	g.broadcast(Msg{"type": "result", "player": who, "value": v, "hint": hint})
	switch hint {
	case "DUNG":
		g.endRound(who)
		return
	case "THAP":
		g.lo = v + 1
	default:
		g.hi = v - 1
	}
	g.advance()
}

func runServer(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("server", flag.ExitOnError)
	var cfg config
	fs.IntVar(&cfg.need, "so-nguoi", 2, "số người cần để bắt đầu ván")
	fs.IntVar(&cfg.timeoutMs, "het-gio-ms", 30000, "thời gian tối đa của một lượt")
	fs.Int64Var(&cfg.seed, "seed", 0, "hạt giống (0 = theo thời gian)")
	fs.IntVar(&cfg.min, "min", 1, "số nhỏ nhất")
	fs.IntVar(&cfg.max, "max", 100, "số lớn nhất")
	fs.IntVar(&cfg.rounds, "so-van", 0, "dừng sau số ván này (0 = chơi mãi)")
	fs.Parse(flags)
	host, port := argOr(pos, 0, "0.0.0.0"), argOr(pos, 1, "9000")

	ln, err := net.Listen("tcp4", net.JoinHostPort(host, port))
	if err != nil {
		fmt.Println("[server] Không mở được cổng:", err)
		os.Exit(1)
	}
	defer ln.Close()
	fmt.Printf("[server] Trò chơi đoán số tại %s:%s; cần %d người; mỗi lượt tối đa %d ms\n",
		host, port, cfg.need, cfg.timeoutMs)
	events := make(chan event, 64)
	go acceptor(ln, events)
	onInterrupt("[server] Dừng server")
	seed := cfg.seed
	if seed == 0 {
		seed = time.Now().Unix()
	}
	g := &game{cfg: cfg, rng: newLcg(seed), state: stateWaiting, names: map[net.Conn]string{}}
	addrs := map[net.Conn]string{}
	for {
		if !g.deadline.IsZero() && !time.Now().Before(g.deadline) {
			g.onTimeout() // kiểm tra trước: thông điệp đến liên tục không "hoãn" được hết giờ
		}
		var timeout <-chan time.Time // nil: không có hạn chót, nhánh này không bao giờ chạy
		if !g.deadline.IsZero() {
			timeout = time.After(time.Until(g.deadline))
		}
		select {
		case <-timeout: // hết giờ của lượt hiện tại
			g.onTimeout()
			continue
		case ev := <-events:
			switch ev.kind {
			case "open":
				addrs[ev.conn] = ev.addr
				fmt.Printf("[server] %s kết nối\n", ev.addr)
			case "msg":
				g.onMessage(ev.conn, addrs[ev.conn], ev.msg)
			case "closed":
				delete(addrs, ev.conn)
				if _, ok := g.names[ev.conn]; ok { // mất kết nối giữa chừng
					g.removePlayer(ev.conn)
				}
				ev.conn.Close()
			}
		}
		if cfg.rounds > 0 && g.round >= cfg.rounds && g.state == stateWaiting {
			fmt.Printf("[server] Đã chơi %d ván, dừng server\n", g.round)
			return
		}
	}
}
