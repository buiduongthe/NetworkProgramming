// Bài mở rộng MR4.2 – Server đua xe thời gian thực trên UDP (server có thẩm quyền, vòng tick 20 Hz).
//
// Mô hình:
//   - Client gửi ĐẦU VÀO (ga/nhả/phanh) kèm số thứ tự = tick mà đầu vào áp dụng; mỗi gói mang kèm 3 đầu vào
//     gần nhất nên mất một gói vẫn còn bản sao ở gói sau.
//   - Server chạy vòng lặp tick cố định 50 ms: lấy đầu vào của tick hiện tại cho từng xe; nếu không có
//     (mọi bản sao đều mất) thì GIỮ đầu vào cũ; mô phỏng vật lý; gửi ẢNH CHỤP trạng thái tới mọi client.
//   - Ảnh chụp mang số tick: client chỉ giữ ảnh mới nhất, bỏ ảnh đến muộn (cũ hơn).
//   - --mat, --dao giả lập mất gói và đảo thứ tự (theo hàm băm có hạt giống nên tái lập được).
//     Danh sách "0,20" nghĩa là cuộc đua 1 mất 0 %, cuộc đua 2 mất 20 %.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"net"
	"os"
	"sort"
	"strconv"
	"strings"
	"time"
)

const (
	dirIn = iota
	dirSnap
	dirReorder
)

type car struct {
	name                                string
	addr                                net.Addr
	id, x, v                            int
	done                                int         // số tick từ lúc xuất phát đến khi về đích (-1: chưa)
	inputs                              map[int]int // tick -> đầu vào đã nhận (bộ đệm đầu vào)
	lastIn                              int         // đầu vào áp dụng gần nhất (dùng lại khi thiếu)
	recv, dropped, held, crashes        int
	snapSent, snapDropped, snapReorders int
	delayed                             []byte // ảnh chụp đang bị giữ lại để gửi muộn (giả lập đảo thứ tự)
}

type packet struct {
	data []byte
	addr net.Addr
}

type raceServer struct {
	need, rounds int
	seed         int64
	mat, dao     []int
	pc           net.PacketConn
	race         int
	cars         map[string]*car // khóa: địa chỉ "ip:port"
	state        string          // CHO -> DUA (gồm đếm ngược) -> XONG
	tick         int
	endTick      int
	nextTick     time.Time // zero: chưa chạy vòng tick
}

func pctList(text string) []int {
	var list []int
	for _, p := range strings.Split(text, ",") {
		n, err := strconv.Atoi(strings.TrimSpace(p))
		if err != nil {
			fmt.Println("Tỉ lệ không hợp lệ:", text)
			os.Exit(2)
		}
		list = append(list, n)
	}
	return list
}

func (s *raceServer) reset() {
	s.cars = map[string]*car{}
	s.state, s.tick, s.nextTick = "CHO", -1, time.Time{}
}

func (s *raceServer) send(addr net.Addr, obj Msg) {
	data, _ := json.Marshal(obj)
	s.pc.WriteTo(data, addr)
}

func (s *raceServer) loss() int    { return s.mat[min(s.race, len(s.mat)-1)] }
func (s *raceServer) reorder() int { return s.dao[min(s.race, len(s.dao)-1)] }

func (s *raceServer) ordered() []*car {
	list := make([]*car, 0, len(s.cars))
	for _, c := range s.cars {
		list = append(list, c)
	}
	sort.Slice(list, func(i, j int) bool { return list[i].id < list[j].id })
	return list
}

// ----- nhận gói -----
func (s *raceServer) onPacket(data []byte, addr net.Addr) {
	p := decodeJSON(data)
	if p == nil {
		return // datagram hỏng: bỏ qua
	}
	switch str(p, "t") {
	case "join":
		s.onJoin(p, addr)
	case "in":
		s.onInput(p, addr)
	}
}

func (s *raceServer) onJoin(p Msg, addr net.Addr) {
	name, ok := p["name"].(string)
	if n := len([]rune(name)); !ok || n < 1 || n > 12 {
		return
	}
	c := s.cars[addr.String()]
	if c == nil || s.state == "XONG" {
		taken := false
		for _, o := range s.cars {
			taken = taken || o.name == name
		}
		if s.state != "CHO" || taken {
			s.send(addr, Msg{"t": "busy"}) // đang đua hoặc trùng tên: client thử lại sau
			return
		}
		c = &car{name: name, addr: addr, id: -1, done: -1, inputs: map[int]int{}}
		s.cars[addr.String()] = c
		fmt.Printf("[server] Thêm một xe từ %s (%d/%d)\n", addr.(*net.UDPAddr).IP, len(s.cars), s.need)
	}
	// trả lời cả khi client gửi lại join (welcome trước đó có thể đã mất)
	s.send(addr, Msg{"t": "welcome", "name": c.name, "need": s.need, "race": s.race + 1,
		"mat": s.loss(), "dao": s.reorder()})
	if s.state == "CHO" && len(s.cars) == s.need {
		s.startRace()
	}
}

func (s *raceServer) onInput(p Msg, addr net.Addr) {
	c := s.cars[addr.String()]
	seq, ok := asInt(p["seq"])
	inputs, okList := p["in"].([]any)
	if c == nil || s.state != "DUA" || !ok || !okList {
		return
	}
	if lossy(s.seed, dirIn, c.id, seq, s.loss()) {
		c.dropped++ // giả lập: gói này "mất trên đường"
		return
	}
	c.recv++
	for j, v := range inputs[:min(3, len(inputs))] { // inputs[j] là đầu vào của tick seq - j
		t := seq - j
		inp, ok := asInt(v)
		if _, have := c.inputs[t]; ok && !have && t > s.tick && inp >= -1 && inp <= 1 {
			c.inputs[t] = inp // chỉ nhận đầu vào cho tick chưa mô phỏng
		}
	}
}

// ----- cuộc đua -----
func (s *raceServer) startRace() {
	list := make([]*car, 0, len(s.cars))
	for _, c := range s.cars {
		list = append(list, c)
	}
	sort.Slice(list, func(i, j int) bool { return list[i].name < list[j].name })
	names := make([]string, len(list))
	for i, c := range list {
		c.id = i // mã xe theo thứ tự tên: không phụ thuộc ai vào trước
		names[i] = c.name
	}
	s.state, s.tick = "DUA", -1
	s.nextTick = time.Now().Add(tickMs * time.Millisecond)
	fmt.Printf("[server] Cuộc đua %d: %s; mất gói %d %%, đảo thứ tự %d %%; xuất phát sau %d tick\n",
		s.race+1, strings.Join(names, ", "), s.loss(), s.reorder(), start)
}

func (s *raceServer) ranking() []*car {
	list := s.ordered()
	sort.SliceStable(list, func(i, j int) bool {
		a, b := list[i], list[j]
		if (a.done < 0) != (b.done < 0) {
			return a.done >= 0
		}
		if a.done != b.done {
			return a.done < b.done
		}
		if a.x != b.x {
			return a.x > b.x
		}
		return a.id < b.id
	})
	return list
}

func doneText(d int) string {
	if d >= 0 {
		return strconv.Itoa(d)
	}
	return "-"
}

func (s *raceServer) doTick() {
	s.tick++
	k := s.tick
	racing := s.state == "DUA" && k >= start
	for _, c := range s.ordered() {
		inp, ok := c.inputs[k]
		for t := range c.inputs { // dọn đầu vào đã dùng hoặc quá hạn
			if t <= k {
				delete(c.inputs, t)
			}
		}
		if !ok {
			inp = c.lastIn // thiếu đầu vào: giữ đầu vào cũ
			if racing && c.done < 0 {
				c.held++
			}
		}
		c.lastIn = inp
		if !racing || c.done >= 0 {
			continue
		}
		var crashed bool
		c.x, c.v, crashed = step(c.x, c.v, inp)
		if crashed {
			c.crashes++
			fmt.Printf("[server] t=%d: %s văng ở khúc cua (quá %s ô/tick)\n", k-start, c.name, fmt2(vCua))
		}
		if c.x >= track {
			c.x, c.v, c.done = track, 0, k-start+1
		}
	}
	if s.state == "DUA" {
		all := true
		for _, c := range s.cars {
			all = all && c.done >= 0
		}
		if all || k >= start+maxRace {
			s.state, s.endTick = "XONG", k
			parts := []string{}
			for i, c := range s.ranking() {
				parts = append(parts, fmt.Sprintf("%d. %s %s tick", i+1, c.name, doneText(c.done)))
			}
			fmt.Printf("[server] Cuộc đua %d kết thúc: %s\n", s.race+1, strings.Join(parts, "; "))
		}
	}
	s.broadcast(k)
	if s.state == "XONG" && k >= s.endTick+endTicks {
		s.finishRace()
	}
}

func (s *raceServer) broadcast(k int) {
	names, cars := []any{}, []any{}
	for _, c := range s.ordered() {
		names = append(names, c.name)
		cars = append(cars, []int{c.id, c.x, c.v, c.done})
	}
	snap := Msg{"t": "snap", "race": s.race + 1, "tick": k, "phase": s.state, "names": names, "cars": cars}
	if s.state == "XONG" {
		rank := []any{}
		for _, c := range s.ranking() {
			rank = append(rank, []any{c.name, c.done})
		}
		snap["rank"] = rank
	}
	data, _ := json.Marshal(snap)
	for _, c := range s.ordered() {
		prev := c.delayed
		c.delayed = nil
		switch {
		case lossy(s.seed, dirSnap, c.id, k, s.loss()):
			c.snapDropped++
		case lossy(s.seed, dirReorder, c.id, k, s.reorder()):
			c.delayed = data // giữ lại, gửi SAU ảnh chụp tick kế tiếp
			c.snapReorders++
		default:
			s.pc.WriteTo(data, c.addr)
			c.snapSent++
		}
		if prev != nil {
			s.pc.WriteTo(prev, c.addr) // ảnh chụp cũ đến sau ảnh mới: client phải bỏ
			c.snapSent++
		}
	}
}

func (s *raceServer) finishRace() {
	for _, c := range s.ordered() {
		fmt.Printf("[server] %s: nhận %d/%d gói đầu vào (giả lập mất %d), %d tick phải giữ đầu vào cũ, văng %d lần\n",
			c.name, c.recv, c.recv+c.dropped, c.dropped, c.held, c.crashes)
	}
	s.race++
	s.reset()
}

func runServer(args []string) {
	pos, flags := splitArgs(args)
	fs := flag.NewFlagSet("server", flag.ExitOnError)
	need := fs.Int("so-xe", 2, "số xe cần để bắt đầu")
	mat := fs.String("mat", "0", "tỉ lệ mất gói giả lập (%), ví dụ 0,20 cho hai cuộc đua")
	dao := fs.String("dao", "0", "tỉ lệ ảnh chụp bị đảo thứ tự (%)")
	seed := fs.Int64("seed", 1, "hạt giống giả lập mất gói")
	rounds := fs.Int("so-van", 0, "dừng sau số cuộc đua này (0 = mãi)")
	fs.Parse(flags)
	host, port := argOr(pos, 0, "0.0.0.0"), argOr(pos, 1, "9200")
	pc, err := net.ListenPacket("udp4", net.JoinHostPort(host, port))
	if err != nil {
		fmt.Println("[server] Không mở được cổng:", err)
		os.Exit(1)
	}
	defer pc.Close()
	fmt.Printf("[server] Đua xe UDP tại %s:%s, %d tick/s, cần %d xe, đường dài %d ô\n",
		host, port, 1000/tickMs, *need, track/100)
	onInterrupt("[server] Dừng server")
	s := &raceServer{need: *need, rounds: *rounds, seed: *seed, mat: pctList(*mat), dao: pctList(*dao), pc: pc}
	s.reset()
	packets := make(chan packet, 256)
	go func() { // goroutine nhận: mỗi datagram thành một phần tử của kênh
		for {
			buf := make([]byte, 2048)
			n, addr, err := pc.ReadFrom(buf)
			if err != nil {
				continue
			}
			packets <- packet{buf[:n], addr}
		}
	}()
	for {
		var timer <-chan time.Time // nil: chưa đua, chỉ chờ gói
		if !s.nextTick.IsZero() {
			timer = time.After(time.Until(s.nextTick))
		}
		select {
		case p := <-packets:
			s.onPacket(p.data, p.addr)
		case <-timer:
			s.nextTick = s.nextTick.Add(tickMs * time.Millisecond) // lịch cố định: không trôi theo thời gian xử lý
			s.doTick()
			if s.state == "CHO" && s.rounds > 0 && s.race >= s.rounds {
				fmt.Printf("[server] Đã đua %d cuộc, dừng server\n", s.race)
				return
			}
		}
	}
}
