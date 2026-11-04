// Bài mở rộng MR3.3 – Client trò chơi đoán số: người chơi thật (bàn phím) hoặc bot.
//
// Chế độ người chơi: một goroutine nhận thông điệp của server và in ra; goroutine chính chỉ hỏi số
// khi đến lượt. Client cũng kiểm tra đầu vào (phải là số nguyên) để báo lỗi sớm, nhưng server vẫn
// kiểm tra lại mọi thứ – server không bao giờ tin client.
//
// Các bot (dùng cho demo tất định):
//
//	chia_doi   – tìm kiếm nhị phân: đoán điểm giữa khoảng [lo, hi] server gửi kèm lượt.
//	ngau_nhien – đoán ngẫu nhiên trong [lo, hi] (bộ sinh LCG, hạt giống --seed).
//	nghich     – cố tình gửi thông điệp sai để xem server kiểm tra.
//	im_lang    – không bao giờ trả lời: bị server tính hết giờ, rồi tự ngắt kết nối.
package main

import (
	"bufio"
	"flag"
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

// describe chuyển thông điệp của server thành dòng chữ cho người chơi; "" nếu không cần in.
func describe(me string, m Msg) string {
	switch str(m, "type") {
	case "welcome":
		return fmt.Sprintf("[%s] Vào phòng chờ: %d/%d người, đoán số trong %d–%d",
			me, num(m, "waiting"), num(m, "need"), num(m, "min"), num(m, "max"))
	case "start":
		order, _ := m["order"].([]any)
		return fmt.Sprintf("[%s] Ván %d bắt đầu, thứ tự: %s", me, num(m, "round"), joinNames(order))
	case "turn":
		if str(m, "player") == me {
			return fmt.Sprintf("[%s] Đến lượt bạn (lượt %d), khoảng %d–%d",
				me, num(m, "turn_no"), num(m, "lo"), num(m, "hi"))
		}
	case "result":
		h := str(m, "hint")
		if t, ok := hintText[h]; ok {
			h = t
		}
		return fmt.Sprintf("[%s] %s đoán %d: %s", me, str(m, "player"), num(m, "value"), h)
	case "timeout":
		return fmt.Sprintf("[%s] %s hết giờ, mất lượt", me, str(m, "player"))
	case "left":
		return fmt.Sprintf("[%s] %s đã rời trò chơi", me, str(m, "player"))
	case "error":
		return fmt.Sprintf("[%s] Server báo lỗi %s: %s", me, str(m, "code"), str(m, "msg"))
	case "game_over":
		who := str(m, "winner")
		if who == "" {
			who = "không ai"
		}
		return fmt.Sprintf("[%s] Kết thúc: %s thắng, số bí mật là %d (%d lượt)",
			me, who, num(m, "secret"), num(m, "turns"))
	case "bye":
		return fmt.Sprintf("[%s] Server kết thúc phiên", me)
	}
	return ""
}

func say(line string) {
	if line != "" {
		fmt.Println(line)
	}
}

// ---------------------------------------------------------------- bot
func runBot(conn net.Conn, me, kind string, seed int64, thinkMs int) error {
	rng := newLcg(seed)
	tricks := map[string]bool{} // các "trò nghịch" đã làm (mỗi trò một lần)
	for {
		m, err := recvJSON(conn)
		if err != nil {
			return err
		}
		if m == nil {
			continue
		}
		say(describe(me, m))
		t := str(m, "type")
		if t == "game_over" || t == "bye" {
			return nil
		}
		if kind == "nghich" && t == "welcome" {
			fmt.Printf("[%s] (nghịch) đoán khi ván chưa bắt đầu, rồi gửi khung không phải JSON\n", me)
			sendJSON(conn, Msg{"type": "guess", "value": 1})
			sendFrame(conn, []byte("xin chào"))
		}
		if t == "timeout" && str(m, "player") == me && kind == "im_lang" {
			fmt.Printf("[%s] (im lặng) bị hết giờ, ngắt kết nối\n", me)
			return nil
		}
		if t != "turn" {
			continue
		}
		lo, hi := num(m, "lo"), num(m, "hi")
		if str(m, "player") != me {
			if kind == "nghich" && !tricks["out_of_turn"] {
				tricks["out_of_turn"] = true
				fmt.Printf("[%s] (nghịch) đoán khi đang là lượt của %s\n", me, str(m, "player"))
				sendJSON(conn, Msg{"type": "guess", "value": lo})
			}
			continue
		}
		if kind == "im_lang" {
			continue
		}
		time.Sleep(time.Duration(thinkMs) * time.Millisecond) // "suy nghĩ" để người xem kịp theo dõi
		if kind == "nghich" && !tricks["bad_values"] {
			tricks["bad_values"] = true
			fmt.Printf("[%s] (nghịch) gửi giá trị chuỗi và số ngoài khoảng\n", me)
			sendJSON(conn, Msg{"type": "guess", "value": "năm mươi"})
			sendJSON(conn, Msg{"type": "guess", "value": hi + 50})
		}
		guess := (lo + hi) / 2
		if kind == "ngau_nhien" {
			guess = rng.nextInt(lo, hi)
		}
		fmt.Printf("[%s] Đoán %d\n", me, guess)
		sendJSON(conn, Msg{"type": "guess", "value": guess})
	}
}

// ---------------------------------------------------------------- người chơi thật
func runHuman(conn net.Conn, me string) {
	turns := make(chan string, 8) // goroutine nhận báo cho goroutine chính: "turn" hoặc "end"
	go func() {
		for {
			m, err := recvJSON(conn)
			if err != nil {
				break
			}
			if m == nil {
				continue
			}
			say(describe(me, m))
			t := str(m, "type")
			if t == "turn" && str(m, "player") == me {
				turns <- "turn"
			}
			if t == "game_over" || t == "bye" {
				break
			}
		}
		turns <- "end"
	}()
	st, _ := os.Stdin.Stat()
	echo := st.Mode()&os.ModeCharDevice == 0 // đầu vào từ tệp/ống: in lại để nhật ký dễ đọc
	in := bufio.NewReader(os.Stdin)
	for <-turns == "turn" {
		for {
			fmt.Printf("[%s] Nhập số: ", me)
			line, err := in.ReadString('\n')
			if err != nil && line == "" { // hết đầu vào (Ctrl+D / Ctrl+Z): rời trò chơi
				fmt.Println()
				sendJSON(conn, Msg{"type": "bye"})
				return
			}
			line = strings.TrimSpace(line)
			if echo {
				fmt.Println(line)
			}
			v, err := strconv.Atoi(line) // kiểm tra phía client: chỉ để báo lỗi sớm
			if err != nil {
				fmt.Printf("[%s] Vui lòng nhập một số nguyên\n", me)
				continue
			}
			sendJSON(conn, Msg{"type": "guess", "value": v})
			break
		}
	}
}

func runPlayer(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("choi", flag.ExitOnError)
	bot := fs.String("bot", "", "chia_doi | ngau_nhien | nghich | im_lang (bỏ trống: người chơi)")
	seed := fs.Int64("seed", 1, "hạt giống của bot ngau_nhien")
	think := fs.Int("nghi-ms", 300, "thời gian bot 'suy nghĩ' mỗi lượt")
	fs.Parse(flags)
	if len(pos) < 1 {
		usage()
	}
	me, host, port := pos[0], argOr(pos, 1, "127.0.0.1"), argOr(pos, 2, "9000")
	switch *bot {
	case "", "chia_doi", "ngau_nhien", "nghich", "im_lang":
	default:
		fmt.Println("Kiểu bot không hợp lệ:", *bot)
		os.Exit(2)
	}
	conn, err := net.DialTimeout("tcp4", net.JoinHostPort(host, port), 3*time.Second)
	if err != nil {
		fmt.Printf("[%s] Không kết nối được %s:%s: %v\n", me, host, port, err)
		os.Exit(1)
	}
	defer conn.Close()
	fmt.Printf("[%s] Đã kết nối %s:%s\n", me, host, port)
	sendJSON(conn, Msg{"type": "hello", "name": me})
	if *bot == "" {
		runHuman(conn, me)
		return
	}
	if err := runBot(conn, me, *bot, *seed, *think); err != nil {
		fmt.Printf("[%s] Server đóng kết nối\n", me)
	}
}
