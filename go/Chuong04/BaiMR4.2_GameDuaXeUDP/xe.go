// Bài mở rộng MR4.2 – Client đua xe UDP: người lái (bàn phím) hoặc bot.
//
// Mỗi khi nhận ảnh chụp tick k (chỉ nhận ảnh MỚI HƠN ảnh đang có), client:
//  1. đối chiếu: so vị trí xe mình mà client đã dự đoán với vị trí server gửi (lệch khi đầu vào bị mất);
//  2. dự đoán phía client: từ ảnh chụp k, chạy lại vật lý với các đầu vào của chính mình để biết xe
//     sẽ ở đâu ở tick k+1 (xe khác thì ngoại suy theo vận tốc cuối);
//  3. chọn đầu vào cho tick k+2 và gửi kèm 2 đầu vào trước đó (dư thừa).
//
// Khi một ảnh chụp bị mất, khung hình của tick đó được vẽ bằng dự đoán – người chơi không thấy xe "đứng hình".
package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"net"
	"os"
	"strings"
	"sync/atomic"
	"time"
)

type carState struct{ id, x, v, done int }

type snapshot struct {
	race, tick int
	phase      string
	names      []string
	cars       []carState
	rank       []string // "1. An 58 tick" ...
}

// parseSnap đọc ảnh chụp từ JSON; ok = false nếu thiếu trường.
func parseSnap(m Msg) (*snapshot, bool) {
	s := &snapshot{race: num(m, "race"), tick: num(m, "tick"), phase: str(m, "phase")}
	names, _ := m["names"].([]any)
	for _, n := range names {
		name, _ := n.(string)
		s.names = append(s.names, name)
	}
	cars, _ := m["cars"].([]any)
	for _, c := range cars {
		f, _ := c.([]any)
		if len(f) != 4 {
			return nil, false
		}
		var v [4]int
		for i := range v {
			v[i], _ = asInt(f[i])
		}
		s.cars = append(s.cars, carState{v[0], v[1], v[2], v[3]})
	}
	rank, _ := m["rank"].([]any)
	for i, r := range rank {
		f, _ := r.([]any)
		if len(f) == 2 {
			name, _ := f[0].(string)
			d, _ := asInt(f[1])
			s.rank = append(s.rank, fmt.Sprintf("%d. %s %s tick", i+1, name, doneText(d)))
		}
	}
	return s, len(s.cars) == len(s.names) && len(s.cars) > 0
}

// raceView – những gì client biết về cuộc đua: ảnh chụp mới nhất, đầu vào của mình, thống kê.
type raceView struct {
	me                        string
	myID                      int
	lastTick                  int
	snap                      *snapshot
	myInputs                  map[int]int // tick -> đầu vào mình đã chọn
	got, stale, missed, maxGp int
	errN, errMax              int // dự đoán xe mình sai (server đã dùng đầu vào khác)
	oerrN, oerrMax            int // ngoại suy xe khác sai sau một quãng hụt ảnh chụp
}

func newView(me string) *raceView {
	return &raceView{me: me, myID: -1, lastTick: -1, myInputs: map[int]int{}}
}

// inputAt – đầu vào mình dùng ở tick t (nếu chưa chọn thì giữ đầu vào gần nhất – giống server).
func (w *raceView) inputAt(t int) int {
	for ; t >= 0; t-- {
		if v, ok := w.myInputs[t]; ok {
			return v
		}
	}
	return 0
}

func (s *snapshot) car(id int) carState {
	for _, c := range s.cars {
		if c.id == id {
			return c
		}
	}
	return carState{}
}

// predictOwn – dự đoán phía client: chạy lại vật lý từ ảnh chụp với chính các đầu vào của mình.
func (w *raceView) predictOwn(s *snapshot, T int) carState {
	c := s.car(w.myID)
	for t := s.tick + 1; t <= T; t++ {
		if t < start || c.done >= 0 {
			continue
		}
		c.x, c.v, _ = step(c.x, c.v, w.inputAt(t))
		if c.x >= track {
			c.x, c.v, c.done = track, 0, t-start+1
		}
	}
	return c
}

// predictOther – xe khác: ngoại suy tuyến tính (dead reckoning) theo vận tốc trong ảnh chụp.
func (w *raceView) predictOther(s *snapshot, id, T int) carState {
	c := s.car(id)
	ticks := T - max(s.tick, start-1)
	if c.done >= 0 || ticks <= 0 {
		return c
	}
	c.x = min(track, c.x+c.v*ticks)
	return c
}

func (w *raceView) frame(T int, predicted bool) []string {
	head := fmt.Sprintf("[%s] Khung t=%d", w.me, T-start)
	if predicted {
		head += " (dự đoán: thiếu ảnh chụp)"
	}
	lines := []string{head}
	s := w.snap
	for id, name := range s.names {
		var c carState
		switch {
		case s.tick == T:
			c = s.car(id)
		case id == w.myID:
			c = w.predictOwn(s, T)
		default:
			c = w.predictOther(s, id, T)
		}
		tail := fmt.Sprintf(" x=%s v=%s", fmt2(c.x), fmt2(c.v))
		if c.done >= 0 {
			tail = fmt.Sprintf(" về đích (%d tick)", c.done)
		}
		letter := strings.ToUpper(name[:1])
		lines = append(lines, "  "+letter+"|"+trackLine(letter, c.x)+tail)
	}
	return lines
}

// accept – nhận một ảnh chụp; false nếu là ảnh cũ (đến muộn) – bị bỏ, ảnh mới nhất thắng.
func (w *raceView) accept(s *snapshot) bool {
	k := s.tick
	if k <= w.lastTick {
		w.stale++
		return false
	}
	if w.myID < 0 {
		for i, n := range s.names {
			if n == w.me {
				w.myID = i
			}
		}
	}
	if w.lastTick >= 0 && k-w.lastTick > 1 {
		gap := k - w.lastTick - 1
		w.missed += gap
		w.maxGp = max(w.maxGp, gap)
		if k >= start { // xe khác: ngoại suy trong quãng hụt lệch bao nhiêu?
			for id := range s.names {
				if id != w.myID {
					if e := abs(w.predictOther(w.snap, id, k).x - s.car(id).x); e > 0 {
						w.oerrN++
						w.oerrMax = max(w.oerrMax, e)
					}
				}
			}
		}
	}
	if w.snap != nil && k >= start { // đối chiếu dự đoán với sự thật của server
		if e := abs(w.predictOwn(w.snap, k).x - s.car(w.myID).x); e > 0 {
			w.errN++
			w.errMax = max(w.errMax, e)
		}
	}
	w.got++
	return true
}

// decide – chọn đầu vào cho tick k+lead, trả gói đầu vào cần gửi (mang kèm window-1 đầu vào trước đó).
func (w *raceView) decide(choose func(c carState) int) Msg {
	T := w.snap.tick + lead
	inp := choose(w.predictOwn(w.snap, T-1))
	from := T
	if len(w.myInputs) > 0 {
		last := -1
		for t := range w.myInputs {
			last = max(last, t)
		}
		from = last + 1
	}
	for t := from; t < T; t++ { // tick bị bỏ qua (do mất ảnh chụp): giữ đầu vào cũ
		w.myInputs[t] = w.inputAt(t)
	}
	w.myInputs[T] = inp
	in := make([]int, window)
	for j := range in {
		in[j] = w.inputAt(T - j)
	}
	return Msg{"t": "in", "seq": T, "in": in}
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

func sendTo(pc net.PacketConn, addr net.Addr, obj Msg) {
	data, _ := json.Marshal(obj)
	pc.WriteTo(data, addr)
}

// join – gửi join (lặp lại mỗi 250 ms vì UDP có thể mất) tới khi nhận welcome.
func join(pc net.PacketConn, addr net.Addr, me string) Msg {
	buf := make([]byte, 4096)
	for {
		sendTo(pc, addr, Msg{"t": "join", "name": me})
		pc.SetReadDeadline(time.Now().Add(250 * time.Millisecond))
		for {
			n, _, err := pc.ReadFrom(buf)
			if err != nil {
				break // hết 250 ms: gửi lại join
			}
			if p := decodeJSON(buf[:n]); p != nil && str(p, "t") == "welcome" {
				return p
			}
		}
	}
}

type carOpts struct {
	bot          string
	frames, runs int
}

// human – trạng thái của người lái: đầu vào hiện tại (đọc/ghi từ hai goroutine nên dùng atomic).
type human struct {
	inp   atomic.Int32
	tLast time.Time
}

func raceOnce(pc net.PacketConn, addr net.Addr, me string, o carOpts, h *human) {
	wel := join(pc, addr, me)
	race := num(wel, "race")
	fmt.Printf("[%s] Vào cuộc đua %d (cần %d xe; mất gói giả lập %d %%, đảo thứ tự %d %%)\n",
		me, race, num(wel, "need"), num(wel, "mat"), num(wel, "dao"))
	w := newView(me)
	nextFrame := start
	wait := 3 * time.Second
	if h != nil {
		wait = 50 * time.Millisecond
	}
	var lastDraw time.Time
	buf := make([]byte, 4096)
	for {
		pc.SetReadDeadline(time.Now().Add(wait))
		n, _, err := pc.ReadFrom(buf)
		var p Msg
		if err != nil {
			var ne net.Error
			if h == nil || !(errors.As(err, &ne) && ne.Timeout()) {
				fmt.Printf("[%s] Mất liên lạc với server\n", me)
				return
			}
		} else {
			p = decodeJSON(buf[:n])
		}
		var s *snapshot
		ok := false
		if p != nil && str(p, "t") == "snap" && num(p, "race") == race {
			s, ok = parseSnap(p)
		}
		if ok && w.accept(s) {
			k := s.tick
			if h != nil {
				h.tLast = time.Now()
			}
			if h == nil && o.frames > 0 && s.phase == "DUA" && w.snap != nil {
				for nextFrame <= k { // khung cho mọi tick cần vẽ đến hết tick k
					var out []string
					if nextFrame == k {
						w.snap = s
						out = w.frame(k, false)
					} else {
						out = w.frame(nextFrame, true) // dùng ảnh chụp cũ + dự đoán
					}
					fmt.Println(strings.Join(out, "\n"))
					nextFrame += o.frames
				}
			}
			w.snap, w.lastTick = s, k
			if s.phase == "XONG" {
				fmt.Printf("[%s] Kết quả: %s\n", me, strings.Join(s.rank, ", "))
				fmt.Printf("[%s] Ảnh chụp: nhận %d, hụt %d tick (dài nhất %d), bỏ %d ảnh cũ\n",
					me, w.got, w.missed, w.maxGp, w.stale)
				fmt.Printf("[%s] Dự đoán xe mình: lệch %d lần (lớn nhất %s ô); ngoại suy xe khác: lệch %d lần (lớn nhất %s ô)\n",
					me, w.errN, fmt2(w.errMax), w.oerrN, fmt2(w.oerrMax))
				return
			}
			choose := func(c carState) int {
				if c.done >= 0 {
					return 0
				}
				return botInput(c.x, c.v, o.bot)
			}
			if h != nil {
				choose = func(carState) int { return int(h.inp.Load()) }
			}
			sendTo(pc, addr, w.decide(choose))
		}
		if h != nil && w.snap != nil && time.Since(lastDraw) >= 100*time.Millisecond { // vẽ lại 10 lần/giây
			lastDraw = time.Now()
			est := w.lastTick + int(time.Since(h.tLast)/(tickMs*time.Millisecond))
			if est >= start {
				fmt.Print("\033[H\033[2J" + strings.Join(w.frame(est, est != w.lastTick), "\n") + "\n")
				fmt.Printf("Đầu vào: %s  (g = ga, n = nhả, p = phanh, Enter)\n", inputText[int(h.inp.Load())])
			} else {
				fmt.Printf("\033[H\033[2JXuất phát sau %.1f s\n", float64((start-est)*tickMs)/1000)
			}
		}
	}
}

func runCar(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("xe", flag.ExitOnError)
	var o carOpts
	fs.StringVar(&o.bot, "bot", "", "lieu | can_than (bỏ trống: tự lái)")
	fs.IntVar(&o.frames, "khung", 10, "bot in một khung mỗi bao nhiêu tick (0 = không in)")
	fs.IntVar(&o.runs, "so-van", 1, "số cuộc đua tham gia")
	fs.Parse(flags)
	if len(pos) < 1 {
		usage()
	}
	if _, ok := styles[o.bot]; o.bot != "" && !ok {
		fmt.Println("Kiểu bot không hợp lệ:", o.bot)
		os.Exit(2)
	}
	me := pos[0]
	addr, err := net.ResolveUDPAddr("udp4", net.JoinHostPort(argOr(pos, 1, "127.0.0.1"), argOr(pos, 2, "9200")))
	if err != nil {
		fmt.Println("Địa chỉ không hợp lệ:", err)
		os.Exit(2)
	}
	pc, err := net.ListenPacket("udp4", "0.0.0.0:0")
	if err != nil {
		fmt.Println("Không mở được socket UDP:", err)
		os.Exit(1)
	}
	defer pc.Close()
	var h *human
	if o.bot == "" {
		h = &human{tLast: time.Now()}
		go func() { // goroutine đọc bàn phím: g/n/p + Enter đổi đầu vào hiện tại
			sc := bufio.NewScanner(os.Stdin)
			for sc.Scan() {
				switch strings.ToLower(strings.TrimSpace(sc.Text()) + " ")[0] {
				case 'g':
					h.inp.Store(1)
				case 'n':
					h.inp.Store(0)
				case 'p':
					h.inp.Store(-1)
				}
			}
		}()
	}
	for i := 0; i < o.runs; i++ {
		raceOnce(pc, addr, me, o, h)
	}
}
