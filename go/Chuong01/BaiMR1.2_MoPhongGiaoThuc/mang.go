package main

// Lõi bộ mô phỏng mạng theo sự kiện rời rạc: topo, hàng đợi sự kiện, định tuyến theo chi phí
// (Dijkstra) và quy tắc chuyển tiếp của từng loại nút. Không dùng socket thật: mỗi gói tin là một
// giá trị Go, "thời gian" là đồng hồ mô phỏng tính bằng micro giây (µs).
//
//	H1 ─┐                                   ┌─ H4
//	H2 ─┼─ SW1 ── R1 ── R2 ── R3 ── SW2 ─┼─ H5      (R1 ── R3: đường dự phòng, chi phí 3)
//	H3 ─┘   LAN A 192.168.1.0/24   LAN B 192.168.2.0/24  └─ H6
//
// Quy tắc chuyển tiếp:
//   - host: nhận gói nếu gói dành cho mình (unicast đúng tên, quảng bá trong LAN, nhóm đã tham gia,
//     địa chỉ anycast mình phục vụ); host không chuyển tiếp gói của người khác;
//   - switch: quảng bá → gửi tràn mọi cổng trừ cổng vào; multicast → chỉ cổng có thành viên
//     (IGMP snooping) và cổng router; unicast → đúng một cổng theo bảng định tuyến;
//   - router: chặn quảng bá; unicast/anycast → giảm TTL, chọn chặng kế theo chi phí nhỏ nhất;
//     multicast → chỉ chuyển tới các LAN có thành viên.

import (
	"container/heap"
	"fmt"
	"slices"
)

// fmtT – định dạng thời gian mô phỏng (µs) thành mili giây, một chữ số thập phân.
func fmtT(us int64) string { return fmt.Sprintf("%d.%d", us/1000, us%1000/100) }

// Rng – bộ sinh số giả ngẫu nhiên xorshift32: cùng seed → cùng dãy số ở Python và Go.
type Rng struct{ x uint32 }

func NewRng(seed int) *Rng {
	x := uint32(seed)*2654435761 + 1
	if x == 0 {
		x = 1
	}
	return &Rng{x}
}

func (r *Rng) Next() uint32 {
	x := r.x
	x ^= x << 13
	x ^= x >> 17
	x ^= x << 5
	r.x = x
	return x
}

// Below – số nguyên ngẫu nhiên trong [0, n).
func (r *Rng) Below(n int) int { return int(r.Next() % uint32(n)) }

type Node struct {
	Name, Kind, IP, Lan          string
	X, Y                         int // tọa độ để vẽ hoạt hình HTML
	Up                           bool
	Sent, Recv, Deliv, Fwd, Drop int
}

type Link struct {
	A, B   string
	Cost   int              // chi phí định tuyến
	Delay  int64            // trễ lan truyền (µs)
	BW     int64            // băng thông: byte mỗi mili giây
	Up     bool             //
	Loss   int              // xác suất mất gói, phần nghìn
	Jitter int64            // trễ ngẫu nhiên thêm tối đa (µs)
	Free   map[string]int64 // thời điểm mỗi chiều rảnh (hàng đợi FIFO)
}

type Packet struct {
	Cast         string // uni | bc | mc | igmp | any
	Kind         string // nhãn hiển thị: TCP, UDP, UNI, BC, MC, IGMP, ANY
	Src, Dst     string
	Label        string
	Size, TTL    int
	DropA, DropB string // kịch bản ép mất gói trên liên kết DropA→DropB
	// dữ liệu của kịch bản
	Flags       string
	Seq, Ack, N int
	T           int64
	Msg         string
	Reply, Join bool
}

func NewPacket(cast, kind, src, dst, label string, size int) *Packet {
	return &Packet{Cast: cast, Kind: kind, Src: src, Dst: dst, Label: label, Size: size, TTL: 64}
}

type nodeDef struct {
	name, kind, ip, lan string
	x, y                int
}
type linkDef struct {
	a, b      string
	cost      int
	delay, bw int64
}

const (
	lanUS, wanUS = 500, 5000   // trễ lan truyền: trong LAN 0,5 ms; giữa router 5 ms
	lanBW, wanBW = 12500, 1250 // 100 Mbit/s và 10 Mbit/s (byte/ms)
)

var nodeDefs = []nodeDef{
	{"H1", "host", "192.168.1.11", "A", 60, 80}, {"H2", "host", "192.168.1.12", "A", 60, 210},
	{"H3", "host", "192.168.1.13", "A", 60, 340}, {"SW1", "switch", "-", "A", 190, 210},
	{"R1", "router", "192.168.1.1", "A", 330, 210}, {"R2", "router", "10.0.0.2", "-", 450, 80},
	{"R3", "router", "192.168.2.1", "B", 570, 210}, {"SW2", "switch", "-", "B", 710, 210},
	{"H4", "host", "192.168.2.14", "B", 840, 80}, {"H5", "host", "192.168.2.15", "B", 840, 210},
	{"H6", "host", "192.168.2.16", "B", 840, 340},
}

var linkDefs = []linkDef{
	{"H1", "SW1", 1, lanUS, lanBW}, {"H2", "SW1", 1, lanUS, lanBW},
	{"H3", "SW1", 1, lanUS, lanBW}, {"SW1", "R1", 1, lanUS, lanBW},
	{"R1", "R2", 1, wanUS, wanBW}, {"R2", "R3", 1, wanUS, wanBW},
	{"R1", "R3", 3, 8000, wanBW}, {"R3", "SW2", 1, lanUS, lanBW},
	{"SW2", "H4", 1, lanUS, lanBW}, {"SW2", "H5", 1, lanUS, lanBW},
	{"SW2", "H6", 1, lanUS, lanBW},
}

type nbr struct {
	name string
	l    *Link
}

// ---------- hàng đợi sự kiện (heap theo thời điểm, cùng thời điểm thì theo thứ tự thêm) ----------
type event struct {
	t   int64
	seq int
	fn  func()
}
type eventQ []event

func (q eventQ) Len() int { return len(q) }
func (q eventQ) Less(i, j int) bool {
	return q[i].t < q[j].t || q[i].t == q[j].t && q[i].seq < q[j].seq
}
func (q eventQ) Swap(i, j int) { q[i], q[j] = q[j], q[i] }
func (q *eventQ) Push(x any)   { *q = append(*q, x.(event)) }
func (q *eventQ) Pop() any {
	old := *q
	e := old[len(old)-1]
	*q = old[:len(old)-1]
	return e
}

type Net struct {
	rng       *Rng
	now       int64
	q         eventQ
	seq       int
	order     []string
	nodes     map[string]*Node
	adj       map[string][]nbr
	links     map[[2]string]*Link
	anycast   map[string][]string // địa chỉ anycast -> host phục vụ
	snoop     map[[2]string][]string
	mroute    map[string][]string // nhóm -> các switch (LAN) có thành viên
	member    map[string][]string // host -> các nhóm đã tham gia
	dist      map[string]map[string]int
	nh        map[string]map[string]string
	hopTrace  bool // in từng bước chuyển (tắt với TCP/UDP cho gọn)
	quiet     bool // không in nhật ký (dùng khi chỉ cần chỉ số, ví dụ lệnh so_sanh)
	onDeliver func(node string, p *Packet)
	onLoss    func(p *Packet, a, b string)
	hops      [][]any // dữ liệu cho hoạt hình HTML
	notes     [][]any
	changes   [][]any
}

func NewNet(seed int) *Net {
	n := &Net{rng: NewRng(seed), nodes: map[string]*Node{}, adj: map[string][]nbr{},
		links: map[[2]string]*Link{}, anycast: map[string][]string{}, snoop: map[[2]string][]string{},
		mroute: map[string][]string{}, member: map[string][]string{}, hopTrace: true,
		hops: [][]any{}, notes: [][]any{}, changes: [][]any{}}
	for _, d := range nodeDefs {
		n.order = append(n.order, d.name)
		n.nodes[d.name] = &Node{Name: d.name, Kind: d.kind, IP: d.ip, Lan: d.lan, X: d.x, Y: d.y, Up: true}
	}
	for _, d := range linkDefs {
		l := &Link{A: d.a, B: d.b, Cost: d.cost, Delay: d.delay, BW: d.bw, Up: true,
			Free: map[string]int64{d.a: 0, d.b: 0}}
		n.links[[2]string{d.a, d.b}], n.links[[2]string{d.b, d.a}] = l, l
		n.adj[d.a] = append(n.adj[d.a], nbr{d.b, l})
		n.adj[d.b] = append(n.adj[d.b], nbr{d.a, l})
	}
	n.computeRoutes()
	return n
}

func (n *Net) At(t int64, fn func()) {
	n.seq++
	heap.Push(&n.q, event{t, n.seq, fn})
}

func (n *Net) Run() {
	for n.q.Len() > 0 {
		e := heap.Pop(&n.q).(event)
		n.now = e.t
		e.fn()
	}
}

func (n *Net) Log(text, node string) {
	if !n.quiet {
		fmt.Printf("%7s ms  %s\n", fmtT(n.now), text)
	}
	n.notes = append(n.notes, []any{n.now, node, text})
}

// ---------- định tuyến: Dijkstra từ mọi nút ----------
func (n *Net) computeRoutes() {
	n.dist, n.nh = map[string]map[string]int{}, map[string]map[string]string{}
	for _, s := range n.order {
		if !n.nodes[s].Up {
			continue
		}
		dist, first, done := map[string]int{s: 0}, map[string]string{s: ""}, map[string]bool{}
		for {
			u := ""
			for _, x := range n.order { // chọn nút chưa xét có khoảng cách nhỏ nhất
				if _, ok := dist[x]; ok && !done[x] && (u == "" || dist[x] < dist[u]) {
					u = x
				}
			}
			if u == "" {
				break
			}
			done[u] = true
			if u != s && n.nodes[u].Kind == "host" {
				continue // host không chuyển tiếp cho nút khác
			}
			for _, e := range n.adj[u] {
				if !e.l.Up || !n.nodes[e.name].Up {
					continue
				}
				d := dist[u] + e.l.Cost
				if old, ok := dist[e.name]; !ok || d < old {
					dist[e.name] = d
					if u == s {
						first[e.name] = e.name
					} else {
						first[e.name] = first[u]
					}
				}
			}
		}
		n.dist[s], n.nh[s] = dist, first
	}
}

func (n *Net) Path(a, b string) []string {
	p := []string{a}
	for p[len(p)-1] != b && n.nh[p[len(p)-1]][b] != "" {
		p = append(p, n.nh[p[len(p)-1]][b])
	}
	return p
}

// target – đích thật của gói: với anycast là máy phục vụ gần nhất còn hoạt động.
func (n *Net) target(node string, p *Packet) string {
	if p.Cast != "any" {
		return p.Dst
	}
	best, bestD := "", 0
	for _, h := range n.anycast[p.Dst] {
		d, ok := n.dist[node][h]
		if n.nodes[h].Up && ok && (best == "" || d < bestD) {
			best, bestD = h, d
		}
	}
	return best
}

// ---------- gửi, truyền trên liên kết, nhận ----------

// Send – host `node` phát gói vào mạng (host chỉ có một liên kết, tới switch).
func (n *Net) Send(node string, p *Packet) {
	n.nodes[node].Sent++
	n.transmit(node, n.adj[node][0].name, p)
}

func (n *Net) transmit(a, b string, pkt *Packet) {
	l := n.links[[2]string{a, b}]
	if !l.Up || !n.nodes[b].Up {
		n.nodes[a].Drop++
		if n.hopTrace {
			n.Log(fmt.Sprintf("%s: liên kết tới %s không hoạt động, bỏ gói", a, b), a)
		}
		return
	}
	c := *pkt // bản sao riêng cho liên kết này
	p := &c
	start := max(n.now, l.Free[a])
	tx := int64(p.Size) * 1000 / l.BW // thời gian đẩy gói lên đường truyền
	l.Free[a] = start + tx
	arr := start + tx + l.Delay
	if l.Jitter > 0 {
		arr += int64(n.rng.Below(int(l.Jitter) + 1))
	}
	lost := false
	if p.DropA == a && p.DropB == b {
		lost, p.DropA, p.DropB = true, "", ""
	} else if l.Loss > 0 && n.rng.Below(1000) < l.Loss {
		lost = true
	}
	n.hops = append(n.hops, []any{start, arr, a, b, p.Kind, p.Label, lost})
	if lost {
		n.At((start+arr)/2, func() { n.lost(p, a, b) })
	} else {
		n.At(arr, func() { n.arrive(b, p, a) })
	}
}

func (n *Net) lost(p *Packet, a, b string) {
	n.nodes[a].Drop++
	if n.onLoss != nil {
		n.onLoss(p, a, b)
	} else {
		n.Log(fmt.Sprintf("× %s: mất trên %s→%s", p.Label, a, b), a)
	}
}

func (n *Net) arrive(node string, p *Packet, frm string) {
	N := n.nodes[node]
	N.Recv++
	if n.hopTrace {
		n.Log(fmt.Sprintf("%3s → %-3s %s", frm, node, p.Label), node)
	}
	switch N.Kind {
	case "host":
		n.hostIn(N, p)
	case "switch":
		n.switchIn(N, p, frm)
	default:
		n.routerIn(N, p, frm)
	}
}

func (n *Net) hostIn(N *Node, p *Packet) {
	mine := false
	switch p.Cast {
	case "uni":
		mine = p.Dst == N.Name
	case "any":
		mine = slices.Contains(n.anycast[p.Dst], N.Name)
	case "bc":
		mine = true // khung quảng bá tới được thì thuộc LAN này
	case "mc":
		mine = slices.Contains(n.member[N.Name], p.Dst)
	}
	if mine {
		N.Deliv++
		if n.onDeliver != nil {
			n.onDeliver(N.Name, p)
		}
	} else {
		N.Drop++
		if n.hopTrace {
			n.Log(N.Name+" bỏ qua: gói không dành cho mình", N.Name)
		}
	}
}

func (n *Net) switchIn(N *Node, p *Packet, frm string) {
	var outs []string
	switch p.Cast {
	case "bc":
		for _, e := range n.adj[N.Name] { // gửi tràn
			if e.name != frm {
				outs = append(outs, e.name)
			}
		}
	case "igmp":
		key := [2]string{N.Name, p.Dst}
		mem := n.snoop[key]
		if p.Join && !slices.Contains(mem, frm) {
			mem = append(mem, frm)
		}
		if !p.Join {
			if i := slices.Index(mem, frm); i >= 0 {
				mem = slices.Delete(mem, i, i+1)
			}
		}
		n.snoop[key] = mem
		for _, e := range n.adj[N.Name] {
			if n.nodes[e.name].Kind == "router" {
				outs = append(outs, e.name)
			}
		}
	case "mc":
		mem := n.snoop[[2]string{N.Name, p.Dst}]
		for _, e := range n.adj[N.Name] {
			if e.name != frm && (slices.Contains(mem, e.name) || n.nodes[e.name].Kind == "router") {
				outs = append(outs, e.name)
			}
		}
	default:
		if t := n.target(N.Name, p); t != "" && n.nh[N.Name][t] != "" {
			outs = []string{n.nh[N.Name][t]}
		}
	}
	for _, v := range outs {
		N.Fwd++
		n.transmit(N.Name, v, p)
	}
}

func (n *Net) routerIn(N *Node, p *Packet, frm string) {
	if p.Cast == "bc" {
		N.Drop++
		n.Log(fmt.Sprintf("%s chặn quảng bá: không chuyển ra ngoài LAN %s", N.Name, n.nodes[frm].Lan), N.Name)
		return
	}
	if p.Cast == "igmp" {
		N.Deliv++
		lans := n.mroute[p.Dst]
		if p.Join && !slices.Contains(lans, frm) {
			n.mroute[p.Dst] = append(lans, frm)
			n.Log(fmt.Sprintf("%s ghi nhận: LAN %s có thành viên %s", N.Name, n.nodes[frm].Lan, p.Dst), N.Name)
		} else if i := slices.Index(lans, frm); !p.Join && len(n.snoop[[2]string{frm, p.Dst}]) == 0 && i >= 0 {
			n.mroute[p.Dst] = slices.Delete(lans, i, i+1)
			n.Log(fmt.Sprintf("%s ghi nhận: LAN %s hết thành viên %s", N.Name, n.nodes[frm].Lan, p.Dst), N.Name)
		}
		return
	}
	p.TTL--
	if p.TTL <= 0 {
		N.Drop++
		n.Log(N.Name+" bỏ gói: TTL = 0", N.Name)
		return
	}
	var outs []string
	if p.Cast == "mc" {
		for _, sw := range n.mroute[p.Dst] {
			if nh := n.nh[N.Name][sw]; nh != "" && nh != frm && !slices.Contains(outs, nh) {
				outs = append(outs, nh)
			}
		}
		if len(outs) == 0 {
			n.Log(N.Name+" không chuyển tiếp: phía sau không có thành viên", N.Name)
		}
	} else {
		if t := n.target(N.Name, p); t != "" && n.nh[N.Name][t] != "" {
			outs = []string{n.nh[N.Name][t]}
		} else {
			N.Drop++
			n.Log(fmt.Sprintf("%s bỏ gói: không có đường tới %s", N.Name, p.Dst), N.Name)
		}
	}
	for _, v := range outs {
		N.Fwd++
		n.transmit(N.Name, v, p)
	}
}

// ---------- tiện ích cho kịch bản ----------
func (n *Net) SetLink(a, b string, up bool) {
	n.links[[2]string{a, b}].Up = up
	n.changes = append(n.changes, []any{n.now, "link", a + "-" + b, up})
	n.computeRoutes()
}

func (n *Net) SetNode(name string, up bool) {
	n.nodes[name].Up = up
	n.changes = append(n.changes, []any{n.now, "node", name, up})
	n.computeRoutes()
}

func (n *Net) Summary() {
	fmt.Println("Tổng kết theo nút (Gửi = phát ra, Nhận = gói tới, Giao = đưa lên")
	fmt.Println("ứng dụng, Ch.tiếp = số bản sao chuyển đi, Bỏ = bỏ qua/chặn/mất):")
	fmt.Printf("  %-4s %-13s %4s %5s %5s %8s %4s\n", "Nút", "Địa chỉ", "Gửi", "Nhận", "Giao", "Ch.tiếp", "Bỏ")
	for _, name := range n.order {
		N := n.nodes[name]
		fmt.Printf("  %-4s %-13s %4d %5d %5d %8d %4d\n", name, N.IP, N.Sent, N.Recv, N.Deliv, N.Fwd, N.Drop)
	}
}
