// Bài mở rộng MR6.4 – Client Caro: người chơi thật (dòng lệnh), bot, hoặc người xem tự động.
//
// Kết nối: chỉ định host, port; hoặc --tim CỔNG để tìm server trong LAN bằng UDP broadcast (Bài 6.5).
//
//	Người chơi (không có --bot, --xem): gõ lệnh ds | tao <phòng> | vao <phòng> | xem <phòng> | h8 | lai | roi | thoat
//	Bot (--bot tan_cong|phong_thu|can_bang|tap_su|im_lang, kèm --tao hoặc --vao); --thu-loi: gửi nước sai
//	Người xem tự động (--xem <phòng>): in tiến trình và bàn cờ cuối mỗi ván, thoát khi phòng đóng.
package main

import (
	"bufio"
	"flag"
	"fmt"
	"net"
	"os"
	"strings"
	"sync/atomic"
	"time"
)

type options struct {
	tim                 int
	create, join, watch string
	bot                 string
	seed                int64
	tryErrors           bool
	games, delayMs      int
}

func overText(m Msg) string {
	reason := reasonText[str(m, "reason")]
	result := reason
	if w := str(m, "winner"); w != "" {
		result = fmt.Sprintf("%s (%s) thắng – %s", w, str(m, "mark"), reason)
	}
	return fmt.Sprintf("Ván %d kết thúc sau %d nước: %s", num(m, "game_no"), num(m, "moves"), result)
}

func lineText(m Msg) string {
	cells, _ := m["line"].([]any)
	parts := []string{}
	for _, p := range cells {
		rc, _ := p.([]any)
		if len(rc) == 2 {
			r, _ := asInt(rc[0])
			c, _ := asInt(rc[1])
			parts = append(parts, cellName(r, c))
		}
	}
	return strings.Join(parts, " ")
}

func orNone(m Msg, key string) string {
	if v := str(m, key); v != "" {
		return v
	}
	return "chưa có"
}

// view – bàn cờ phía client: chỉ là bản sao để hiển thị, được cập nhật từ thông điệp của server.
type view struct {
	b            *board
	lastR, lastC int
	mark, turn   string // turn = "" nghĩa là không ai có lượt (ván đã hết)
}

func newView() *view { return &view{b: newBoard(), lastR: -1, lastC: -1, turn: "X"} }

func (v *view) apply(m Msg, me string) {
	switch str(m, "type") {
	case "start":
		v.b, v.lastR, v.lastC, v.turn = newBoard(), -1, -1, "X"
		v.mark = ""
		if str(m, "x") == me {
			v.mark = "X"
		} else if str(m, "o") == me {
			v.mark = "O"
		}
	case "board": // ảnh chụp bàn cờ cho người xem đến muộn
		rows, _ := m["rows"].([]any)
		for r, row := range rows {
			text, _ := row.(string)
			for c := 0; c < len(text) && c < N && r < N; c++ {
				v.b[r][c] = text[c]
			}
		}
		v.turn = str(m, "turn")
	case "moved":
		r, c := num(m, "r"), num(m, "c")
		if inside(r, c) {
			v.b[r][c] = str(m, "mark")[0]
		}
		v.lastR, v.lastC = r, c
		v.turn = str(m, "next") // lượt kế do server quyết định ("" = null: ván đã hết)
	case "over":
		v.turn = ""
	}
}

func connect(o options, me, host, port string) (net.Conn, error) {
	if o.tim > 0 {
		fmt.Printf("[%s] Tìm server Caro bằng broadcast tới cổng %d...\n", me, o.tim)
		list, err := discover(o.tim, 800, "255.255.255.255")
		if err != nil || len(list) == 0 {
			fmt.Printf("[%s] Không tìm thấy server nào\n", me)
			os.Exit(1)
		}
		host, port = list[0].ip, fmt.Sprint(list[0].port)
		fmt.Printf("[%s] Thấy %s tại %s:%s, kết nối\n", me, list[0].name, host, port)
	}
	conn, err := net.DialTimeout("tcp4", net.JoinHostPort(host, port), 3*time.Second)
	if err == nil && o.tim == 0 {
		fmt.Printf("[%s] Đã kết nối %s:%s\n", me, host, port)
	}
	return conn, err
}

// ---------------------------------------------------------------- bot
func quitRoom(conn net.Conn, me string, games int) {
	fmt.Printf("[%s] Đã chơi %d ván, rời phòng\n", me, games)
	sendJSON(conn, Msg{"type": "bye"}) // bye = rời phòng + rời sảnh trong một thông điệp
}

// runBot – bot phản ứng theo từng thông điệp của server.
//
// Sau mỗi ván, người THUA (hoặc quân O nếu hòa) lên tiếng trước: xin chơi lại hoặc rời phòng; người thắng
// chờ quyết định đó rồi mới trả lời. Nhờ quy ước này hai bot không "nói cùng lúc", nhật ký có thứ tự cố định.
func runBot(conn net.Conn, me string, o options) error {
	v, games, tried := newView(), 0, map[string]bool{}
	rng := newLcg(o.seed)
	waiting := false // người thắng đang chờ đối thủ quyết định
	inRoom := false
	for {
		m, err := recvJSON(conn)
		if err != nil {
			return err
		}
		if m == nil {
			continue
		}
		t := str(m, "type")
		v.apply(m, me)
		wantMore := games < o.games && o.bot != "im_lang"
		switch t {
		case "welcome":
			rooms, _ := m["rooms"].([]any)
			fmt.Printf("[%s] Vào sảnh; số phòng hiện có: %d\n", me, len(rooms))
			if o.create != "" {
				sendJSON(conn, Msg{"type": "create", "room": o.create})
			} else {
				sendJSON(conn, Msg{"type": "join", "room": o.join})
			}
		case "joined":
			inRoom = true
			fmt.Printf("[%s] Vào phòng %s, cầm quân %s\n", me, str(m, "room"), str(m, "role"))
		case "error":
			fmt.Printf("[%s] Server báo lỗi %s: %s\n", me, str(m, "code"), str(m, "msg"))
			if !inRoom { // không tạo/vào được phòng: bot dừng
				sendJSON(conn, Msg{"type": "bye"})
				return nil
			}
		case "start":
			fmt.Printf("[%s] Ván %d bắt đầu: X = %s, O = %s (tôi cầm %s)\n",
				me, num(m, "game_no"), str(m, "x"), str(m, "o"), v.mark)
			if o.tryErrors && v.mark == "O" && !tried["early"] {
				tried["early"] = true
				fmt.Printf("[%s] (thử lỗi) đánh h8 khi chưa đến lượt\n", me)
				sendJSON(conn, Msg{"type": "move", "r": 7, "c": 7})
			}
		case "rematch_wait":
			if str(m, "by") != me && waiting {
				waiting = false
				if !wantMore {
					quitRoom(conn, me, games)
					return nil
				}
				fmt.Printf("[%s] %s xin chơi lại – đồng ý\n", me, str(m, "by"))
				sendJSON(conn, Msg{"type": "rematch"})
			}
		case "left":
			fmt.Printf("[%s] %s rời phòng\n", me, str(m, "player"))
			if waiting {
				quitRoom(conn, me, games)
				return nil
			}
		case "over":
			games++
			wantMore = games < o.games && o.bot != "im_lang"
			fmt.Printf("[%s] %s\n", me, overText(m))
			winner := str(m, "winner")
			leader := winner == me || (winner == "" && v.mark == "X")
			switch {
			case str(m, "reason") == "left": // đối thủ đã đi: không còn ai để chơi
				quitRoom(conn, me, games)
				return nil
			case leader:
				waiting = true
			case wantMore:
				fmt.Printf("[%s] Xin chơi lại\n", me)
				sendJSON(conn, Msg{"type": "rematch"})
			default:
				quitRoom(conn, me, games)
				return nil
			}
		}
		if (t == "start" || t == "moved") && v.mark != "" && v.turn == v.mark && o.bot != "im_lang" {
			time.Sleep(time.Duration(o.delayMs) * time.Millisecond) // "suy nghĩ" để người xem kịp theo dõi
			if o.tryErrors && v.lastR >= 0 && !tried["bad_cells"] {
				tried["bad_cells"] = true
				fmt.Printf("[%s] (thử lỗi) đánh vào ô đã có quân %s và ô ngoài bàn (15, 3)\n",
					me, cellName(v.lastR, v.lastC))
				sendJSON(conn, Msg{"type": "move", "r": v.lastR, "c": v.lastC})
				sendJSON(conn, Msg{"type": "move", "r": N, "c": 3})
			}
			r, c := chooseMove(v.b, v.mark[0], o.bot, rng)
			sendJSON(conn, Msg{"type": "move", "r": r, "c": c})
		}
	}
}

// ---------------------------------------------------------------- người xem tự động
func runWatcher(conn net.Conn, me string, o options) error {
	v := newView()
	for {
		m, err := recvJSON(conn)
		if err != nil {
			return err
		}
		if m == nil {
			continue
		}
		v.apply(m, me)
		switch str(m, "type") {
		case "welcome":
			sendJSON(conn, Msg{"type": "watch", "room": o.watch})
		case "joined":
			info, _ := m["info"].(Msg)
			fmt.Printf("[%s] Xem phòng %s (X = %s, O = %s)\n", me, str(m, "room"), orNone(info, "x"), orNone(info, "o"))
		case "error":
			fmt.Printf("[%s] Server báo lỗi %s: %s\n", me, str(m, "code"), str(m, "msg"))
			return nil
		case "board":
			fmt.Printf("[%s] Nhận ảnh chụp bàn cờ: ván %d, đã đi %d nước\n", me, num(m, "game_no"), num(m, "moves"))
		case "start":
			fmt.Printf("[%s] Ván %d: X = %s, O = %s\n", me, num(m, "game_no"), str(m, "x"), str(m, "o"))
		case "moved":
			if num(m, "move_no")%10 == 0 {
				fmt.Printf("[%s] Nước %d: %s (%s) %s\n", me, num(m, "move_no"), str(m, "by"), str(m, "mark"),
					cellName(num(m, "r"), num(m, "c")))
			}
		case "over":
			fmt.Printf("[%s] %s\n", me, overText(m))
			for _, line := range render(v.b, v.lastR, v.lastC) {
				fmt.Println(line)
			}
			if lt := lineText(m); lt != "" {
				fmt.Printf("[%s] Dãy thắng: %s\n", me, lt)
			}
		case "left":
			fmt.Printf("[%s] %s rời phòng\n", me, str(m, "player"))
		case "room_closed":
			fmt.Printf("[%s] Phòng %s đã đóng\n", me, str(m, "room"))
			sendJSON(conn, Msg{"type": "bye"})
			return nil
		}
	}
}

// ---------------------------------------------------------------- người chơi thật
const help = "Lệnh: ds | tao <phòng> | vao <phòng> | xem <phòng> | <ô> ví dụ h8 | lai | roi | thoat"

func runHuman(conn net.Conn, me string, o options) {
	v := newView()
	showBoard := func() {
		for _, line := range render(v.b, v.lastR, v.lastC) {
			fmt.Println(line)
		}
		if v.mark != "" {
			state := "chờ đối thủ"
			if v.turn == v.mark {
				state = "ĐẾN LƯỢT BẠN"
			}
			fmt.Printf("[%s] Bạn cầm %s; %s\n", me, v.mark, state)
		}
	}
	var quitting atomic.Bool // người dùng gõ thoat: lỗi đọc sau đó là bình thường, không báo
	go func() {
		for {
			m, err := recvJSON(conn)
			if err != nil {
				if !quitting.Load() {
					fmt.Printf("[%s] Mất kết nối tới server\n", me)
				}
				return
			}
			if m == nil {
				continue
			}
			v.apply(m, me)
			switch str(m, "type") {
			case "welcome", "rooms":
				rooms, _ := m["rooms"].([]any)
				colon := ""
				if len(rooms) > 0 {
					colon = ":"
				}
				fmt.Printf("[%s] Có %d phòng%s\n", me, len(rooms), colon)
				for _, x := range rooms {
					r, _ := x.(Msg)
					fmt.Printf("    %s: X = %s, O = %s, %d người xem, %s\n", str(r, "room"), orNone(r, "x"),
						orNone(r, "o"), num(r, "watchers"), str(r, "state"))
				}
			case "joined":
				fmt.Printf("[%s] Vào phòng %s với vai trò %s\n", me, str(m, "room"), str(m, "role"))
			case "start":
				fmt.Printf("[%s] Ván %d: X = %s, O = %s; mỗi nước tối đa %d s\n", me, num(m, "game_no"),
					str(m, "x"), str(m, "o"), num(m, "timeout_ms")/1000)
				showBoard()
			case "moved", "board":
				showBoard()
			case "over":
				extra := ""
				if lt := lineText(m); lt != "" {
					extra = "; dãy " + lt
				}
				fmt.Printf("[%s] %s%s\n", me, overText(m), extra)
				fmt.Printf("[%s] Gõ 'lai' để xin chơi lại hoặc 'roi' để rời phòng\n", me)
			case "rematch_wait":
				fmt.Printf("[%s] %s xin chơi lại\n", me, str(m, "by"))
			case "left":
				fmt.Printf("[%s] %s rời phòng\n", me, str(m, "player"))
			case "room_closed":
				fmt.Printf("[%s] Phòng %s đã đóng\n", me, str(m, "room"))
			case "error":
				fmt.Printf("[%s] Lỗi %s: %s\n", me, str(m, "code"), str(m, "msg"))
			}
		}
	}()
	fmt.Printf("[%s] %s\n", me, help)
	if o.create != "" {
		sendJSON(conn, Msg{"type": "create", "room": o.create})
	} else if o.join != "" {
		sendJSON(conn, Msg{"type": "join", "room": o.join})
	}
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		parts := strings.Fields(sc.Text())
		if len(parts) == 0 {
			continue
		}
		cmd, arg := strings.ToLower(parts[0]), ""
		if len(parts) > 1 {
			arg = parts[1]
		}
		if cmd == "thoat" {
			break
		}
		switch cmd {
		case "ds":
			sendJSON(conn, Msg{"type": "list"})
		case "tao":
			sendJSON(conn, Msg{"type": "create", "room": arg})
		case "vao":
			sendJSON(conn, Msg{"type": "join", "room": arg})
		case "xem":
			sendJSON(conn, Msg{"type": "watch", "room": arg})
		case "lai":
			sendJSON(conn, Msg{"type": "rematch"})
		case "roi":
			sendJSON(conn, Msg{"type": "leave"})
		default:
			r, c, ok := parseCell(cmd)
			if !ok {
				fmt.Printf("[%s] %s\n", me, help)
				continue
			}
			if v.b[r][c] != empty { // kiểm tra sớm phía client; server vẫn kiểm tra lại
				fmt.Printf("[%s] Ô %s đã có quân\n", me, cmd)
				continue
			}
			sendJSON(conn, Msg{"type": "move", "r": r, "c": c})
		}
	}
	quitting.Store(true)
	sendJSON(conn, Msg{"type": "bye"})
}

func runPlayer(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("choi", flag.ExitOnError)
	var o options
	fs.IntVar(&o.tim, "tim", 0, "tìm server bằng UDP broadcast tới cổng này")
	fs.StringVar(&o.create, "tao", "", "tạo phòng")
	fs.StringVar(&o.join, "vao", "", "vào phòng để chơi")
	fs.StringVar(&o.watch, "xem", "", "xem phòng (người xem tự động)")
	fs.StringVar(&o.bot, "bot", "", "tan_cong | phong_thu | can_bang | tap_su | im_lang")
	fs.Int64Var(&o.seed, "seed", 1, "hạt giống của bot tap_su")
	fs.BoolVar(&o.tryErrors, "thu-loi", false, "cố tình gửi nước sai")
	fs.IntVar(&o.games, "so-van", 1, "số ván bot chơi")
	fs.IntVar(&o.delayMs, "tre-ms", 300, "thời gian bot 'suy nghĩ' mỗi nước")
	fs.Parse(flags)
	if len(pos) < 1 {
		usage()
	}
	if _, ok := styles[o.bot]; o.bot != "" && !ok && o.bot != "im_lang" {
		fmt.Println("Kiểu bot không hợp lệ:", o.bot)
		os.Exit(2)
	}
	if o.bot != "" && o.create == "" && o.join == "" {
		fmt.Println("Bot cần --tao hoặc --vao")
		os.Exit(2)
	}
	me := pos[0]
	conn, err := connect(o, me, argOr(pos, 1, "127.0.0.1"), argOr(pos, 2, "9100"))
	if err != nil {
		fmt.Printf("[%s] Không kết nối được: %v\n", me, err)
		os.Exit(1)
	}
	defer conn.Close()
	sendJSON(conn, Msg{"type": "hello", "name": me})
	switch {
	case o.watch != "":
		err = runWatcher(conn, me, o)
	case o.bot != "":
		err = runBot(conn, me, o)
	default:
		runHuman(conn, me, o)
	}
	if err != nil {
		fmt.Printf("[%s] Server đóng kết nối\n", me)
	}
}
