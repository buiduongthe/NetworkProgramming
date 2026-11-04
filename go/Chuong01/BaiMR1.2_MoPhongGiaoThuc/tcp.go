package main

// Kịch bản TCP: bắt tay ba bước, cửa sổ trượt, điều khiển tắc nghẽn, đóng FIN.
//
// TCP được rút gọn để dễ theo dõi (dựa trên TCP Reno, RFC 9293 và RFC 5681):
//   - bắt tay SYN → SYN+ACK → ACK; đóng FIN → ACK, FIN → ACK;
//   - bên nhận xác nhận tích lũy từng đoạn, giữ lại đoạn đến sớm (ngoài thứ tự);
//   - bên gửi chỉ gửi khi số byte đang bay < cwnd (cửa sổ tắc nghẽn);
//   - khởi đầu chậm: cwnd < ssthresh thì mỗi ACK mới tăng cwnd thêm 1 MSS (gấp đôi mỗi RTT);
//   - tránh tắc nghẽn: mỗi ACK mới tăng MSS·MSS/cwnd (≈ +1 MSS mỗi RTT);
//   - 3 ACK trùng → gửi lại nhanh: ssthresh = max(đang bay/2, 2 MSS), cwnd = ssthresh;
//   - hết giờ (RTO) → ssthresh = max(đang bay/2, 2 MSS), cwnd = 1 MSS, gửi lại từ đoạn chưa
//     xác nhận, RTO tăng gấp đôi.
//
// Đơn giản hóa: RTO cố định 100 ms ban đầu (RFC 6298 tính từ RTT đo được), không thổi phồng cwnd
// trong khôi phục nhanh, không có ACK trễ, cửa sổ bên nhận đủ lớn.

import (
	"fmt"
	"slices"
	"sort"
	"strings"
	"unicode/utf8"
)

const (
	MSS = 1000
	HDR = 40 // byte tiêu đề TCP/IP
	W   = 34 // độ rộng phần mũi tên trong sơ đồ
)

func runes(s string) int { return utf8.RuneCountInString(s) }

// arrow – mũi tên giữa hai trục thời gian; mất gói thì dừng giữa đường bằng ×.
func arrow(label string, right, lost bool) string {
	if right {
		s := "─ " + label + " "
		if !lost {
			return s + strings.Repeat("─", W-1-runes(s)) + "►"
		}
		s += strings.Repeat("─", 4) + "×"
		return s + strings.Repeat(" ", max(0, W-runes(s)))
	}
	s := " " + label + " ─"
	if !lost {
		return "◄" + strings.Repeat("─", W-1-runes(s)) + s
	}
	s = "×" + strings.Repeat("─", 4) + s
	return strings.Repeat(" ", max(0, W-runes(s))) + s
}

func cw(c int) string { return fmt.Sprintf("%d.%d", c/MSS, c%MSS*10/MSS) }

type row struct {
	t    int64
	text string
}
type tpoint struct {
	t int64
	v int
}
type mark struct {
	t int64
	m string
}

type TcpScenario struct {
	net                      *Net
	c, s                     string
	total                    int
	drops                    []int // các đoạn bị ép mất (lặp số đoạn = mất nhiều lần)
	cwnd, ssthresh           int
	una, nxt                 int // byte đầu tiên chưa được xác nhận / sẽ gửi tiếp
	dup                      int
	recovering               bool
	rto                      int64
	gen                      int
	timerOn                  bool
	sent                     map[int]int // đoạn -> số lần đã gửi
	tx, retx, fast, timeouts int
	rnext                    int          // bên nhận: byte mong đợi tiếp theo
	rbuf                     map[int]bool // bên nhận: đoạn đến sớm
	rows                     []row
	cwndLog                  []tpoint
	events                   []mark
	state                    string
	tData0, tDone            int64
}

func NewTcp(net *Net, nseg int, drops []int) *TcpScenario {
	sc := &TcpScenario{net: net, c: "H1", s: "H5", total: nseg * MSS, drops: drops,
		cwnd: MSS, ssthresh: 8 * MSS, una: 1, nxt: 1, rto: 100_000, sent: map[int]int{},
		rnext: 1, rbuf: map[int]bool{}, cwndLog: []tpoint{{0, MSS}}, state: "CLOSED"}
	net.hopTrace = false
	net.onDeliver = sc.deliver
	net.onLoss = sc.loss
	return sc
}

func count(xs []int, k int) int {
	n := 0
	for _, x := range xs {
		if x == k {
			n++
		}
	}
	return n
}

// ---------- ghi sơ đồ ----------
func (sc *TcpScenario) addRow(t int64, text string) { sc.rows = append(sc.rows, row{t, text}) }

func (sc *TcpScenario) pktRow(p *Packet, tRecv int64, note string, lost bool) {
	right := p.Src == sc.c
	recv := fmtT(tRecv)
	if lost {
		recv = "×"
	}
	sc.addRow(p.T, fmt.Sprintf("%6s %6s  |%s|  %s", fmtT(p.T), recv, arrow(p.Label, right, lost), note))
	to := sc.c // ghi chú cho hoạt hình HTML
	if right {
		to = sc.s
	}
	what := to + " nhận " + p.Label + " " + note
	if lost {
		what = "× " + p.Label + " " + note
	}
	sc.net.notes = append(sc.net.notes, []any{sc.net.now, to, strings.TrimRight(what, " ")})
}

func (sc *TcpScenario) event(text string) {
	sc.addRow(sc.net.now, fmt.Sprintf("%6s %6s  => %s", fmtT(sc.net.now), "", text))
	sc.net.notes = append(sc.net.notes, []any{sc.net.now, sc.c, text})
}

func (sc *TcpScenario) setCwnd(v int) {
	sc.cwnd = v
	sc.cwndLog = append(sc.cwndLog, tpoint{sc.net.now, v})
}

// ---------- gửi ----------
func (sc *TcpScenario) out(frm, to, label, flags string, seq, ack, length int, drop bool) {
	p := NewPacket("uni", "TCP", frm, to, label, HDR+length)
	p.Flags, p.Seq, p.Ack, p.T = flags, seq, ack, sc.net.now
	if drop {
		if frm == sc.c {
			p.DropA, p.DropB = "R2", "R3"
		} else {
			p.DropA, p.DropB = "R2", "R1"
		}
	}
	sc.net.Send(frm, p)
}

func (sc *TcpScenario) Start() {
	sc.state = "SYN_SENT"
	sc.out(sc.c, sc.s, "SYN seq=0", "SYN", 0, 0, 0, false)
}

func (sc *TcpScenario) sendData() {
	for sc.nxt <= sc.total && sc.nxt-sc.una+MSS <= sc.cwnd {
		k := (sc.nxt-1)/MSS + 1
		n := sc.sent[k] + 1
		sc.sent[k] = n
		sc.tx++
		label := fmt.Sprintf("#%d seq=%d", k, sc.nxt)
		if n > 1 {
			sc.retx++
			label += " gửi lại"
		}
		sc.out(sc.c, sc.s, label, "DATA", sc.nxt, 0, MSS, count(sc.drops, k) >= n)
		sc.nxt += MSS
		if !sc.timerOn {
			sc.arm()
		}
	}
}

func (sc *TcpScenario) arm() {
	sc.gen++
	sc.timerOn = true
	g := sc.gen
	sc.net.At(sc.net.now+sc.rto, func() { sc.timeout(g) })
}

func (sc *TcpScenario) stopTimer() {
	sc.gen++
	sc.timerOn = false
}

func (sc *TcpScenario) timeout(g int) {
	if g != sc.gen {
		return // bộ hẹn giờ này đã bị hủy/khởi động lại
	}
	sc.timeouts++
	sc.ssthresh = max((sc.nxt-sc.una)/2, 2*MSS)
	sc.setCwnd(MSS)
	sc.events = append(sc.events, mark{sc.net.now, "T"})
	sc.event(fmt.Sprintf("HẾT GIỜ RTO=%d ms: ssthresh=%s, cwnd=1", sc.rto/1000, cw(sc.ssthresh)))
	sc.rto = min(sc.rto*2, 800_000)
	sc.nxt, sc.dup, sc.recovering = sc.una, 0, false
	sc.timerOn = false
	sc.sendData()
}

// ---------- nhận ----------
func (sc *TcpScenario) deliver(node string, p *Packet) {
	d := sc.net
	f := p.Flags
	if node == sc.s { // ---- phía máy chủ H5
		switch {
		case f == "SYN":
			sc.pktRow(p, d.now, "", false)
			sc.out(sc.s, sc.c, "SYN+ACK seq=0 ack=1", "SYNACK", 0, 1, 0, false)
		case f == "ACK" && sc.state == "SYN_SENT2":
			sc.pktRow(p, d.now, "ESTABLISHED", false)
			sc.state = "ESTABLISHED"
		case f == "DATA":
			if p.Seq == sc.rnext {
				sc.rnext += MSS
				for sc.rbuf[sc.rnext] { // đoạn đến sớm nay đã liền mạch
					delete(sc.rbuf, sc.rnext)
					sc.rnext += MSS
				}
			} else if p.Seq > sc.rnext {
				sc.rbuf[p.Seq] = true
			}
			sc.pktRow(p, d.now, "", false)
			sc.out(sc.s, sc.c, fmt.Sprintf("ACK %d", sc.rnext), "ACK", 0, sc.rnext, 0, false)
		case f == "FIN":
			sc.pktRow(p, d.now, "CLOSE_WAIT", false)
			sc.out(sc.s, sc.c, fmt.Sprintf("ACK %d", p.Seq+1), "FINACK", 0, p.Seq+1, 0, false)
			d.At(d.now+1000, func() { sc.out(sc.s, sc.c, "FIN seq=1", "FIN2", 1, 0, 0, false) })
		case f == "LASTACK":
			sc.pktRow(p, d.now, "CLOSED", false)
		}
		return
	}
	switch f { // ---- phía khách H1
	case "SYNACK":
		sc.pktRow(p, d.now, "", false)
		sc.state = "SYN_SENT2"
		sc.out(sc.c, sc.s, "ACK ack=1", "ACK", 0, 1, 0, false)
		sc.tData0 = d.now
		sc.sendData()
	case "ACK":
		sc.onAck(p)
	case "FINACK":
		sc.pktRow(p, d.now, "FIN_WAIT_2", false)
	case "FIN2":
		sc.pktRow(p, d.now, "TIME_WAIT", false)
		sc.out(sc.c, sc.s, "ACK ack=2", "LASTACK", 0, 2, 0, false)
	}
}

func (sc *TcpScenario) onAck(p *Packet) {
	ack := p.Ack
	if ack > sc.una { // ACK mới
		sc.una = ack
		sc.nxt = max(sc.nxt, ack)
		sc.dup = 0
		switch {
		case sc.recovering:
			sc.recovering = false
			sc.setCwnd(sc.ssthresh)
		case sc.cwnd < sc.ssthresh:
			sc.setCwnd(sc.cwnd + MSS) // khởi đầu chậm
		default:
			sc.setCwnd(sc.cwnd + MSS*MSS/sc.cwnd) // tránh tắc nghẽn
		}
		sc.rto = 100_000
		sc.pktRow(p, sc.net.now, "cwnd="+cw(sc.cwnd), false)
		if sc.una > sc.total {
			sc.stopTimer()
			if sc.state == "ESTABLISHED" || sc.state == "SYN_SENT2" {
				sc.state = "FIN_WAIT_1"
				sc.tDone = sc.net.now
				sc.out(sc.c, sc.s, fmt.Sprintf("FIN seq=%d", sc.total+1), "FIN", sc.total+1, 0, 0, false)
			}
			return
		}
		sc.arm()
		sc.sendData()
	} else if ack == sc.una { // ACK trùng
		sc.dup++
		sc.pktRow(p, sc.net.now, fmt.Sprintf("trùng %d", sc.dup), false)
		if sc.dup == 3 {
			sc.fast++
			sc.ssthresh = max((sc.nxt-sc.una)/2, 2*MSS)
			sc.setCwnd(sc.ssthresh)
			sc.recovering = true
			sc.events = append(sc.events, mark{sc.net.now, "F"})
			sc.event("3 ACK TRÙNG: gửi lại nhanh, ssthresh=cwnd=" + cw(sc.cwnd))
			k := (sc.una-1)/MSS + 1
			sc.sent[k]++
			sc.tx++
			sc.retx++
			sc.out(sc.c, sc.s, fmt.Sprintf("#%d seq=%d gửi lại", k, sc.una), "DATA", sc.una, 0, MSS,
				count(sc.drops, k) >= sc.sent[k])
		}
	}
}

func (sc *TcpScenario) loss(p *Packet, a, b string) {
	sc.pktRow(p, 0, fmt.Sprintf("MẤT %s→%s", a, b), true)
}

// ---------- kết quả ----------
func (sc *TcpScenario) Report() {
	fmt.Printf("Sơ đồ thời gian – trình tự (%s khách, %s máy chủ; thời gian ms):\n", sc.c, sc.s)
	fmt.Printf("%6s %6s  %-*s%s\n", "gửi", "nhận", W+1, sc.c, sc.s)
	sort.SliceStable(sc.rows, func(i, j int) bool { return sc.rows[i].t < sc.rows[j].t })
	for _, r := range sc.rows {
		fmt.Println(strings.TrimRight(r.text, " "))
	}
	fmt.Println()
	sc.chart()
	dur := sc.tDone - sc.tData0
	fmt.Println()
	fmt.Printf("Tóm tắt TCP: %d đoạn (%d byte), truyền %d lần, gửi lại %d\n", sc.total/MSS, sc.total, sc.tx, sc.retx)
	fmt.Printf("  (%d lần gửi lại nhanh, %d lần hết giờ); xong sau %s ms,\n", sc.fast, sc.timeouts, fmtT(dur))
	fmt.Printf("  thông lượng ≈ %d kbit/s (liên kết chậm nhất 10 Mbit/s,\n", int64(sc.total)*8*1000/dur)
	fmt.Println("  RTT ≈ 26 ms): cửa sổ nhỏ và mất gói làm thông lượng thấp hơn")
	fmt.Println("  nhiều so với băng thông.")
}

func (sc *TcpScenario) chart() {
	const cols = 52
	tEnd := sc.cwndLog[len(sc.cwndLog)-1].t
	vals := make([]int, cols)
	for c := range cols {
		t := tEnd * int64(c+1) / cols
		v := MSS
		for _, pt := range sc.cwndLog {
			if pt.t <= t {
				v = pt.v
			}
		}
		vals[c] = v / MSS
	}
	top := slices.Max(vals)
	fmt.Printf("Biểu đồ cwnd (đơn vị MSS = %d byte) theo thời gian:\n", MSS)
	for y := top; y > 0; y-- {
		var b strings.Builder
		for _, v := range vals {
			if v >= y {
				b.WriteString("█")
			} else {
				b.WriteString(" ")
			}
		}
		fmt.Printf("%4d │%s\n", y, b.String())
	}
	fmt.Println("     └" + strings.Repeat("─", cols))
	marks := []rune(strings.Repeat(" ", cols))
	for _, e := range sc.events {
		marks[min(cols-1, int(e.t*cols/max(tEnd, 1)))] = []rune(e.m)[0]
	}
	fmt.Println("      " + strings.TrimRight(string(marks), " "))
	right := fmtT(tEnd) + " ms"
	fmt.Printf("      0%*s\n", cols-1, right)
	fmt.Println("      F = gửi lại nhanh (3 ACK trùng), T = hết giờ RTO")
}
