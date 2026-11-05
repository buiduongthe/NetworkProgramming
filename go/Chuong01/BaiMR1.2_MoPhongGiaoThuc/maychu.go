package main

// Bài mở rộng MR1.2 – Bên trong máy chủ: đồng bộ, đa luồng, nhóm luồng, bất đồng bộ và phối hợp.
//
// Sáu yêu cầu từ các client (H1, H2, H3 ở LAN A; H4, H6 ở LAN B) đi qua mạng mô phỏng tới máy chủ H5.
// Mỗi yêu cầu cần đọc CSDL 30 ms (chờ I/O: luồng chờ nhưng không tốn CPU) rồi xử lý 10 ms CPU;
// máy chủ có 2 lõi CPU. Cùng khối việc đó, máy chủ được tổ chức theo năm mô hình:
//
//	dong_bo      một luồng, xử lý lần lượt: yêu cầu sau phải chờ yêu cầu trước xong hẳn;
//	da_luong     mỗi yêu cầu một luồng (tạo luồng 1 ms); luồng nhiều hơn lõi thì phải chờ lõi;
//	             cập nhật bộ đếm chung trong đoạn găng có khóa (3 ms) – luồng khác phải chờ khóa;
//	nhom_luong   nhóm 2 luồng thợ + hàng đợi tối đa 2 yêu cầu; hàng đợi đầy thì từ chối ngay (503);
//	bat_dong_bo  một luồng chạy vòng lặp sự kiện: I/O không chặn (nhiều I/O chờ cùng lúc), nhưng phần
//	             CPU chạy ngay trên vòng lặp nên vòng lặp bị chặn – nhịp tim 10 ms chạy muộn;
//	phoi_hop     vòng lặp sự kiện lo I/O, đẩy phần CPU sang nhóm 2 luồng (trong Go: gửi việc qua kênh
//	             cho các goroutine thợ); thợ gửi kết quả về vòng lặp qua kênh – vòng lặp luôn rảnh,
//	             nhịp tim đúng giờ.
//
// Mô hình giảng dạy đã đơn giản hóa: lõi CPU cấp theo thứ tự đến, không chia lát thời gian (không
// preemption); đoạn găng chỉ tính thời gian giữ khóa; thời gian chuyển ngữ cảnh bằng 0.

import (
	"fmt"
	"strings"
)

const (
	ioUS      int64 = 30 * MS // đọc CSDL: chờ I/O
	cpuUS     int64 = 10 * MS // xử lý: dùng một lõi CPU
	khoaUS    int64 = 3 * MS  // đoạn găng (mô hình đa luồng)
	taoUS     int64 = 1 * MS  // tạo một luồng mới (mô hình đa luồng)
	nhipUS    int64 = 10 * MS // chu kỳ nhịp tim của vòng lặp sự kiện
	soLoi           = 2       // số lõi CPU của máy chủ
	soTho           = 2       // số luồng của nhóm luồng
	hangToiDa       = 2       // sức chứa hàng đợi của nhóm luồng
	server          = "H5"
)

// yeuCau – (mã yêu cầu, client, thời điểm gửi µs).
var yeuCau = []struct {
	label, client string
	t             int64
}{{"A", "H4", 0}, {"B", "H6", 0}, {"C", "H4", 2 * MS}, {"D", "H1", 0}, {"E", "H2", 0}, {"F", "H3", 0}}

var mcOrder = []string{"dong_bo", "da_luong", "nhom_luong", "bat_dong_bo", "phoi_hop"}
var mcTitles = map[string]string{
	"dong_bo":     "Đồng bộ, một luồng – xử lý lần lượt, yêu cầu sau phải chờ",
	"da_luong":    "Đa luồng – mỗi yêu cầu một luồng, chờ lõi CPU, khóa",
	"nhom_luong":  "Nhóm luồng – 2 luồng thợ + hàng đợi 2, đầy thì từ chối (503)",
	"bat_dong_bo": "Bất đồng bộ – một vòng lặp sự kiện, việc CPU chặn vòng lặp",
	"phoi_hop":    "Phối hợp – vòng lặp sự kiện lo I/O, nhóm luồng lo việc CPU",
}
var tenNgan = map[string]string{"dong_bo": "đồng bộ", "da_luong": "đa luồng", "nhom_luong": "nhóm luồng",
	"bat_dong_bo": "bất đồng bộ", "phoi_hop": "phối hợp"}
var nhanXet = map[string][]string{
	"dong_bo": {"Nhận xét: một luồng làm lần lượt; khi chờ CSDL luồng bị chặn,",
		"  CPU rảnh mà không phục vụ được ai; yêu cầu sau chờ cộng dồn."},
	"da_luong": {"Nhận xét: mỗi yêu cầu một luồng nên các lần chờ I/O chồng lên",
		"  nhau; nhưng 6 luồng chỉ có 2 lõi (-: chờ lõi), đoạn găng chỉ",
		"  cho một luồng vào mỗi lúc (~: chờ khóa); mỗi luồng tốn bộ",
		"  nhớ và thời gian tạo (+)."},
	"nhom_luong": {"Nhận xét: số luồng cố định nên tài nguyên được giới hạn; khi",
		"  2 thợ đều bận và hàng đợi đầy, yêu cầu bị từ chối ngay (503)",
		"  thay vì làm máy chủ quá tải."},
	"bat_dong_bo": {"Nhận xét: một luồng vẫn phục vụ nhiều yêu cầu vì I/O không",
		"  chặn (hàng I/O đang chờ có tới 6 lệnh cùng lúc); nhưng việc",
		"  CPU chạy trên vòng lặp làm mọi việc khác, kể cả nhịp tim,",
		"  phải chờ."},
	"phoi_hop": {"Nhận xét: vòng lặp chỉ điều phối (nhận, gửi lệnh I/O, trả lời);",
		"  việc CPU chạy trên nhóm luồng, song song trên 2 lõi, rồi báo",
		"  về vòng lặp: vòng lặp luôn rảnh, nhịp tim đúng giờ."},
}
var kyHieu = map[string]string{"khoa": "#", "cho_khoa": "~", "cho_cpu": "-", "tao": "+"}

// padR, padL – căn lề theo số ký tự (rune), không theo byte như %-12s của fmt.
func padR(s string, w int) string { return s + strings.Repeat(" ", max(0, w-runes(s))) }
func padL(s string, w int) string { return strings.Repeat(" ", max(0, w-runes(s))) + s }

// seg – một đoạn hoạt động của luồng: [t0, t1), trạng thái, mã yêu cầu.
type seg struct {
	t0, t1      int64
	kind, label string
}

// Lane – một luồng của máy chủ và các đoạn hoạt động của nó.
type Lane struct {
	name  string
	segs  []seg
	marks []int64 // thời điểm vòng lặp chạy một việc rất ngắn (nhận, chuyển giao, nhịp tim)
}

func (L *Lane) seg(t0, t1 int64, kind, label string) {
	if t1 > t0 {
		L.segs = append(L.segs, seg{t0, t1, kind, label})
	}
}

type choLoi struct { // một luồng đang chờ lõi CPU hoặc chờ khóa
	lane  *Lane
	label string
	done  func()
	tw    int64
}

type viec struct { // một việc trong hàng đợi của vòng lặp sự kiện
	fn    func()
	dur   int64
	label string
}

type yc struct{ client, gui, den, nhan, ma int64 }

type ioSeg struct {
	t0, t1 int64
	label  string
}

// chiSo – chỉ số so sánh của một mô hình.
type chiSo struct {
	hoanTat, tb, lon int64
	luong, tuChoi    int
	nhip             int64
}

type MayChu struct {
	net      *Net
	mode     string
	lanes    []*Lane
	freeCore int
	cpuWait  []choLoi // hàng chờ lõi (FIFO)
	lockBusy bool
	lockWait []choLoi
	dem      int
	io       []ioSeg    // I/O bất đồng bộ đang chờ
	beats    [][2]int64 // nhịp tim [dự kiến, thực tế]
	req      map[string]*yc
	clients  map[string]string
	choXuLy  int // số yêu cầu máy chủ chưa trả lời
	soLuong  int
	maxLuong int
	tuChoi   int
	// đồng bộ
	chinh     *Lane
	choKetNoi []string
	ban       bool
	// nhóm luồng / phối hợp
	tho   []*Lane
	ranh  []bool
	hang  []string
	hangT []int64
	// vòng lặp sự kiện
	vl    *Lane
	ready []viec
	vlBan bool
}

func NewMayChu(n *Net, mode string) *MayChu {
	return &MayChu{net: n, mode: mode, freeCore: soLoi, req: map[string]*yc{},
		clients: map[string]string{}, choXuLy: len(yeuCau)}
}

func (m *MayChu) log(text string) { m.net.Log(text, server) }

func (m *MayChu) lane(name string) *Lane {
	L := &Lane{name: name, segs: []seg{}, marks: []int64{}}
	m.lanes = append(m.lanes, L)
	return L
}

// ---------- tài nguyên dùng chung: lõi CPU và khóa ----------

// cpu – chạy 10 ms CPU trên `lane`; hết lõi thì chờ theo thứ tự đến.
func (m *MayChu) cpu(lane *Lane, label string, done func()) {
	if m.freeCore > 0 {
		m.freeCore--
		m.cpuStart(lane, label, done)
	} else {
		m.log(fmt.Sprintf("%s: %s chờ lõi CPU (cả %d lõi đang bận)", lane.name, label, soLoi))
		m.cpuWait = append(m.cpuWait, choLoi{lane, label, done, m.net.now})
	}
}

func (m *MayChu) cpuStart(lane *Lane, label string, done func()) {
	t := m.net.now
	lane.seg(t, t+cpuUS, "cpu", label)
	m.net.At(t+cpuUS, func() { m.cpuEnd(done) })
}

func (m *MayChu) cpuEnd(done func()) {
	if len(m.cpuWait) > 0 { // trao lõi vừa rảnh cho luồng chờ lâu nhất
		w := m.cpuWait[0]
		m.cpuWait = m.cpuWait[1:]
		w.lane.seg(w.tw, m.net.now, "cho_cpu", w.label)
		m.cpuStart(w.lane, w.label, w.done)
	} else {
		m.freeCore++
	}
	done()
}

// khoa – vào đoạn găng: giữ khóa 3 ms để cập nhật bộ đếm chung; khóa đang bị giữ thì chờ.
func (m *MayChu) khoa(lane *Lane, label string, done func()) {
	if !m.lockBusy {
		m.lockBusy = true
		m.khoaGiu(lane, label, done)
	} else {
		m.log(fmt.Sprintf("%s: %s chờ khóa (luồng khác đang trong đoạn găng)", lane.name, label))
		m.lockWait = append(m.lockWait, choLoi{lane, label, done, m.net.now})
	}
}

func (m *MayChu) khoaGiu(lane *Lane, label string, done func()) {
	t := m.net.now
	lane.seg(t, t+khoaUS, "khoa", label)
	tha := func() {
		m.dem++
		m.log(fmt.Sprintf("%s: %s vào đoạn găng, bộ đếm chung = %d, nhả khóa", lane.name, label, m.dem))
		if len(m.lockWait) > 0 {
			w := m.lockWait[0]
			m.lockWait = m.lockWait[1:]
			w.lane.seg(w.tw, m.net.now, "cho_khoa", w.label)
			m.khoaGiu(w.lane, w.label, w.done)
		} else {
			m.lockBusy = false
		}
		done()
	}
	m.net.At(t+khoaUS, tha)
}

// ---------- mạng: gửi yêu cầu, nhận yêu cầu, trả lời ----------
func (m *MayChu) batDau() {
	m.net.onDeliver = m.nhan
	for _, r := range yeuCau {
		m.req[r.label] = &yc{gui: r.t}
		m.clients[r.label] = r.client
		label, client := r.label, r.client
		m.net.At(r.t, func() {
			m.net.Log(fmt.Sprintf("%s gửi yêu cầu %s tới máy chủ %s", client, label, server), client)
			m.net.Send(client, msgPacket("uni", "YC", client, server,
				fmt.Sprintf("YC %s %s→%s", label, client, server), 200, label))
		})
	}
	switch m.mode {
	case "dong_bo":
		m.khoiTaoDongBo()
	case "da_luong":
	case "nhom_luong":
		m.khoiTaoNhomLuong()
	case "bat_dong_bo":
		m.khoiTaoBatDongBo()
	case "phoi_hop":
		m.khoiTaoPhoiHop()
	}
}

func (m *MayChu) nhan(node string, p *Packet) {
	label := p.Msg
	r := m.req[label]
	if node == server {
		r.den = m.net.now
		m.log(fmt.Sprintf("yêu cầu %s từ %s tới %s", label, m.clients[label], server))
		switch m.mode {
		case "dong_bo":
			m.denDongBo(label)
		case "da_luong":
			m.denDaLuong(label)
		case "nhom_luong":
			m.denNhomLuong(label)
		case "bat_dong_bo":
			m.vlNhan(label, m.batIOXong)
		case "phoi_hop":
			m.vlNhan(label, m.phoiIOXong)
		}
	} else {
		r.nhan, r.ma = m.net.now, int64(p.N)
		m.net.Log(fmt.Sprintf("%s nhận trả lời %s (%d) sau %s ms", node, label, p.N,
			fmtT(m.net.now-r.gui)), node)
	}
}

func (m *MayChu) traLoi(label string, code int) {
	c := m.clients[label]
	m.choXuLy--
	text, size := fmt.Sprintf("TL %s %s→%s", label, server, c), 1000
	if code != 200 {
		text, size = fmt.Sprintf("503 %s %s→%s", label, server, c), 100
	}
	p := msgPacket("uni", "TL", server, c, text, size, label)
	p.N = code
	m.net.Send(server, p)
}

// ---------- 1. đồng bộ, một luồng ----------
func (m *MayChu) khoiTaoDongBo() {
	m.chinh = m.lane("luồng chính")
	m.soLuong, m.maxLuong = 1, 1
}

func (m *MayChu) denDongBo(label string) {
	if m.ban {
		m.choKetNoi = append(m.choKetNoi, label)
		m.log(fmt.Sprintf("luồng chính bận → %s chờ trong hàng đợi kết nối (%d)", label, len(m.choKetNoi)))
	} else {
		m.dongBoXuLy(label)
	}
}

func (m *MayChu) dongBoXuLy(label string) {
	m.ban = true
	t := m.net.now
	m.log(fmt.Sprintf("luồng chính xử lý %s: đọc CSDL, luồng bị chặn %d ms", label, ioUS/MS))
	m.chinh.seg(t, t+ioUS, "io", label)
	m.net.At(t+ioUS, func() { m.cpu(m.chinh, label, func() { m.dongBoXong(label) }) })
}

func (m *MayChu) dongBoXong(label string) {
	m.log(fmt.Sprintf("luồng chính xong %s (CPU %d ms), gửi trả lời", label, cpuUS/MS))
	m.traLoi(label, 200)
	if len(m.choKetNoi) > 0 {
		next := m.choKetNoi[0]
		m.choKetNoi = m.choKetNoi[1:]
		m.dongBoXuLy(next)
	} else {
		m.ban = false
	}
}

// ---------- 2. đa luồng: mỗi yêu cầu một luồng ----------
func (m *MayChu) denDaLuong(label string) {
	L := m.lane(fmt.Sprintf("T%d", len(m.lanes)+1))
	m.soLuong++
	m.maxLuong = max(m.maxLuong, m.soLuong)
	m.log(fmt.Sprintf("tạo luồng %s cho %s (%d luồng đang chạy)", L.name, label, m.soLuong))
	t := m.net.now
	L.seg(t, t+taoUS, "tao", label)
	xong := func() {
		m.soLuong--
		m.log(fmt.Sprintf("%s: gửi trả lời %s, luồng kết thúc (%d luồng còn chạy)", L.name, label, m.soLuong))
		m.traLoi(label, 200)
	}
	doc := func() {
		u := m.net.now
		m.log(fmt.Sprintf("%s: %s đọc CSDL (%s bị chặn, các luồng khác vẫn chạy)", L.name, label, L.name))
		L.seg(u, u+ioUS, "io", label)
		m.net.At(u+ioUS, func() { m.cpu(L, label, func() { m.khoa(L, label, xong) }) })
	}
	m.net.At(t+taoUS, doc)
}

// ---------- 3. nhóm luồng + hàng đợi giới hạn ----------
func (m *MayChu) khoiTaoNhomLuong() {
	for i := range soTho {
		m.tho = append(m.tho, m.lane(fmt.Sprintf("W%d", i+1)))
		m.ranh = append(m.ranh, true)
	}
	m.soLuong, m.maxLuong = soTho, soTho
}

func (m *MayChu) denNhomLuong(label string) {
	for i := range soTho {
		if m.ranh[i] {
			m.nhomGiao(i, label)
			return
		}
	}
	if len(m.hang) < hangToiDa {
		m.hang = append(m.hang, label)
		m.hangT = append(m.hangT, m.net.now)
		m.log(fmt.Sprintf("W1, W2 đều bận → %s vào hàng đợi (%d/%d)", label, len(m.hang), hangToiDa))
	} else {
		m.tuChoi++
		m.log(fmt.Sprintf("hàng đợi đầy → từ chối %s ngay (503)", label))
		m.traLoi(label, 503)
	}
}

func (m *MayChu) nhomGiao(i int, label string) {
	m.ranh[i] = false
	L := m.tho[i]
	t := m.net.now
	m.log(fmt.Sprintf("%s nhận %s: đọc CSDL (%s bị chặn %d ms)", L.name, label, L.name, ioUS/MS))
	L.seg(t, t+ioUS, "io", label)
	m.net.At(t+ioUS, func() { m.cpu(L, label, func() { m.nhomXong(i, label) }) })
}

func (m *MayChu) nhomXong(i int, label string) {
	m.log(fmt.Sprintf("W%d xong %s, gửi trả lời", i+1, label))
	m.traLoi(label, 200)
	if len(m.hang) > 0 {
		lb, tw := m.hang[0], m.hangT[0]
		m.hang, m.hangT = m.hang[1:], m.hangT[1:]
		m.log(fmt.Sprintf("W%d lấy %s từ hàng đợi (đã chờ %s ms)", i+1, lb, fmtT(m.net.now-tw)))
		m.nhomGiao(i, lb)
	} else {
		m.ranh[i] = true
	}
}

// ---------- vòng lặp sự kiện (dùng cho 4 và 5) ----------
func (m *MayChu) khoiTaoVongLap() {
	m.vl = m.lane("vòng lặp")
	m.net.At(0, func() { m.nhip(0) })
}

// post – đưa một việc vào hàng đợi của vòng lặp (dur > 0: việc CPU chạy ngay trên vòng lặp).
func (m *MayChu) post(fn func(), dur int64, label string) {
	m.ready = append(m.ready, viec{fn, dur, label})
	if !m.vlBan {
		m.vlBan = true
		m.net.At(m.net.now, m.chay)
	}
}

// chay – vòng lặp lấy lần lượt từng việc; việc CPU giữ vòng lặp tới khi xong.
func (m *MayChu) chay() {
	for len(m.ready) > 0 {
		v := m.ready[0]
		m.ready = m.ready[1:]
		if v.dur > 0 {
			m.cpu(m.vl, v.label, func() { v.fn(); m.chay() })
			return
		}
		m.vl.marks = append(m.vl.marks, m.net.now)
		v.fn()
	}
	m.vlBan = false
}

// nhip – nhịp tim mỗi 10 ms: một việc rất ngắn trên vòng lặp; đo nó chạy muộn bao nhiêu.
func (m *MayChu) nhip(p int64) {
	if m.choXuLy == 0 {
		return
	}
	beat := func() {
		m.beats = append(m.beats, [2]int64{p, m.net.now})
		if m.net.now-p >= MS {
			m.log(fmt.Sprintf("nhịp tim %s ms chạy muộn %s ms: vòng lặp bận", fmtT(p), fmtT(m.net.now-p)))
		}
	}
	m.post(beat, 0, "")
	m.net.At(p+nhipUS, func() { m.nhip(p + nhipUS) })
}

func (m *MayChu) vlNhan(label string, xongIO func(string)) {
	m.post(func() {
		t := m.net.now
		m.log(fmt.Sprintf("vòng lặp nhận %s, gửi lệnh đọc CSDL không chặn", label))
		m.io = append(m.io, ioSeg{t, t + ioUS, label})
		m.net.At(t+ioUS, func() { xongIO(label) })
	}, 0, "")
}

// ---------- 4. bất đồng bộ: một vòng lặp, CPU trên vòng lặp ----------
func (m *MayChu) khoiTaoBatDongBo() {
	m.soLuong, m.maxLuong = 1, 1
	m.khoiTaoVongLap()
}

func (m *MayChu) batIOXong(label string) {
	m.log(fmt.Sprintf("CSDL xong %s → việc CPU %d ms vào hàng đợi vòng lặp", label, cpuUS/MS))
	m.post(func() {
		m.log(fmt.Sprintf("vòng lặp tính xong %s (CPU trên vòng lặp), trả lời", label))
		m.traLoi(label, 200)
	}, cpuUS, label)
}

// ---------- 5. phối hợp: vòng lặp + nhóm luồng cho việc CPU ----------
func (m *MayChu) khoiTaoPhoiHop() {
	m.soLuong, m.maxLuong = 1+soTho, 1+soTho
	m.khoiTaoVongLap()
	for i := range soTho {
		m.tho = append(m.tho, m.lane(fmt.Sprintf("P%d", i+1)))
		m.ranh = append(m.ranh, true)
	}
}

func (m *MayChu) phoiIOXong(label string) {
	m.post(func() {
		m.log(fmt.Sprintf("vòng lặp giao việc CPU của %s cho nhóm luồng", label))
		for i := range soTho {
			if m.ranh[i] {
				m.phoiChay(i, label)
				return
			}
		}
		m.hang = append(m.hang, label)
		m.log(fmt.Sprintf("P1, P2 đều bận → việc CPU của %s chờ trong hàng đợi", label))
	}, 0, "")
}

func (m *MayChu) phoiChay(i int, label string) {
	m.ranh[i] = false
	m.cpu(m.tho[i], label, func() { m.phoiXong(i, label) })
}

func (m *MayChu) phoiXong(i int, label string) {
	m.log(fmt.Sprintf("P%d tính xong %s → gửi kết quả về vòng lặp", i+1, label))
	m.post(func() {
		m.log(fmt.Sprintf("vòng lặp nhận kết quả %s từ nhóm luồng, trả lời", label))
		m.traLoi(label, 200)
	}, 0, "")
	if len(m.hang) > 0 {
		lb := m.hang[0]
		m.hang = m.hang[1:]
		m.phoiChay(i, lb)
	} else {
		m.ranh[i] = true
	}
}

// ---------- báo cáo ----------
func (m *MayChu) gantt() {
	var end int64
	for _, L := range m.lanes {
		for _, s := range L.segs {
			end = max(end, s.t1)
		}
	}
	for _, s := range m.io {
		end = max(end, s.t1)
	}
	step := max(int64(MS), (end+46*MS-1)/(46*MS)*MS) // tối đa 46 ô
	width := (end + step - 1) / step
	fmt.Printf("Bên trong máy chủ %s, 1 ô = %d ms:\n", server, step/MS)
	fmt.Println("  CHỮ HOA dùng CPU, chữ thường bị chặn chờ I/O, + tạo luồng,")
	fmt.Println("  - chờ lõi CPU, # giữ khóa, ~ chờ khóa, · việc rất ngắn")
	for _, L := range m.lanes {
		var row strings.Builder
		for i := range width {
			mid := i*step + step/2
			ch := " "
			for _, s := range L.segs {
				if s.t0 <= mid && mid < s.t1 {
					switch s.kind {
					case "cpu":
						ch = s.label
					case "io":
						ch = strings.ToLower(s.label)
					default:
						ch = kyHieu[s.kind]
					}
					break
				}
			}
			if ch == " " {
				for _, mk := range L.marks {
					if i*step <= mk && mk < (i+1)*step {
						ch = "·"
						break
					}
				}
			}
			row.WriteString(ch)
		}
		fmt.Printf("  %s|%s|\n", padR(L.name, 12), row.String())
	}
	if len(m.io) > 0 {
		var row strings.Builder
		for i := range width {
			mid := i*step + step/2
			n := 0
			for _, s := range m.io {
				if s.t0 <= mid && mid < s.t1 {
					n++
				}
			}
			if n > 0 {
				fmt.Fprintf(&row, "%d", min(n, 9))
			} else {
				row.WriteString(" ")
			}
		}
		fmt.Printf("  %s|%s|\n", padR("I/O đang chờ", 12), row.String())
	}
	right := fmt.Sprintf("%d ms", width*step/MS)
	fmt.Printf("  %s 0%s\n", padR("", 12), padL(right, int(width)-1))
}

// chiSo – chỉ số so sánh: hoàn tất, phản hồi trung bình/lớn nhất, số luồng, từ chối, nhịp tim trễ.
func (m *MayChu) chiSo() chiSo {
	var tong, lon, hoanTat int64
	for _, y := range yeuCau {
		r := m.req[y.label]
		ph := r.nhan - r.gui
		tong += ph
		lon = max(lon, ph)
		hoanTat = max(hoanTat, r.nhan)
	}
	nhip := int64(-1)
	for _, b := range m.beats {
		nhip = max(nhip, b[1]-b[0])
	}
	return chiSo{hoanTat, tong / int64(len(yeuCau)), lon, m.maxLuong, m.tuChoi, nhip}
}

func (m *MayChu) report() {
	fmt.Println()
	m.gantt()
	fmt.Println()
	fmt.Printf("  %s %s %s %s %s %s  Kết quả\n", padR("Mã", 3), padR("Client", 6), padL("Gửi", 6),
		padL("Tới H5", 7), padL("Nhận TL", 8), padL("Phản hồi", 9))
	for _, y := range yeuCau {
		r := m.req[y.label]
		kq := "200 OK"
		if r.ma != 200 {
			kq = "503 từ chối"
		}
		fmt.Printf("  %s %s %s %s %s %s  %s\n", padR(y.label, 3), padR(y.client, 6), padL(fmtT(r.gui), 6),
			padL(fmtT(r.den), 7), padL(fmtT(r.nhan), 8), padL(fmtT(r.nhan-r.gui), 9), kq)
	}
	s := m.chiSo()
	fmt.Printf("  Hoàn tất lúc %s ms; phản hồi TB %s ms, lớn nhất %s ms\n", fmtT(s.hoanTat), fmtT(s.tb), fmtT(s.lon))
	fmt.Printf("  Luồng tối đa %d; từ chối %d\n", s.luong, s.tuChoi)
	if len(m.beats) > 0 {
		var tong int64
		for _, b := range m.beats {
			tong += b[1] - b[0]
		}
		fmt.Printf("  Nhịp tim %d ms: %d nhịp, trễ lớn nhất %s ms, TB %s ms\n", nhipUS/MS, len(m.beats),
			fmtT(s.nhip), fmtT(tong/int64(len(m.beats))))
	}
	fmt.Println()
	for _, line := range nhanXet[m.mode] {
		fmt.Println(line)
	}
}

// data – dữ liệu cho hoạt hình HTML: các luồng, I/O đang chờ, nhịp tim.
func (m *MayChu) data(rd *runData) {
	rd.Lanes = [][]any{}
	for _, L := range m.lanes {
		segs := [][]any{}
		for _, s := range L.segs {
			segs = append(segs, []any{s.t0, s.t1, s.kind, s.label})
		}
		rd.Lanes = append(rd.Lanes, []any{L.name, segs, L.marks})
	}
	for _, s := range m.io {
		rd.IO = append(rd.IO, []any{s.t0, s.t1, s.label})
	}
	for _, b := range m.beats {
		rd.Beats = append(rd.Beats, []int64{b[0], b[1]})
	}
}

// chayMayChu – chạy một mô hình máy chủ trên `n`, in nhật ký và báo cáo; ghi dữ liệu hoạt hình vào rd.
func chayMayChu(n *Net, mode string, rd *runData) {
	n.hopTrace = false
	fmt.Printf("6 yêu cầu tới %s, mỗi yêu cầu: đọc CSDL %d ms (chờ I/O)\n", server, ioUS/MS)
	fmt.Printf("  + xử lý %d ms CPU; máy chủ có %d lõi CPU\n", cpuUS/MS, soLoi)
	m := NewMayChu(n, mode)
	m.batDau()
	n.Run()
	m.report()
	m.data(rd)
}

// doChiSo – chạy một mô hình máy chủ không in gì (n.quiet); trả về chỉ số so sánh.
func doChiSo(n *Net, mode string) chiSo {
	n.hopTrace, n.quiet = false, true
	m := NewMayChu(n, mode)
	m.batDau()
	n.Run()
	return m.chiSo()
}

// soSanh – in bảng so sánh năm mô hình từ chỉ số đã đo.
func soSanh(seed int) {
	fmt.Println("So sánh năm mô hình máy chủ: cùng 6 yêu cầu, mỗi yêu cầu")
	fmt.Printf("  %d ms I/O + %d ms CPU, máy chủ %d lõi. TB, Lớn nhất: thời gian\n", ioUS/MS, cpuUS/MS, soLoi)
	fmt.Println("  phản hồi; 503: số yêu cầu bị từ chối; Nhịp tim: trễ lớn nhất.")
	fmt.Printf("  %s %s %s %s %s %s %s\n", padR("Mô hình", 11), padL("Luồng", 5), padL("Hoàn tất", 9),
		padL("TB", 9), padL("Lớn nhất", 9), padL("503", 3), padL("Nhịp tim", 9))
	for _, mode := range mcOrder {
		s := doChiSo(NewNet(seed), mode)
		nhip := "–"
		if s.nhip >= 0 {
			nhip = fmtT(s.nhip) + " ms"
		}
		fmt.Printf("  %s %5d %s %s %s %3d %s\n", padR(tenNgan[mode], 11), s.luong, padL(fmtT(s.hoanTat)+" ms", 9),
			padL(fmtT(s.tb)+" ms", 9), padL(fmtT(s.lon)+" ms", 9), s.tuChoi, padL(nhip, 9))
	}
	fmt.Println()
	fmt.Println("Ghi nhớ: luồng giúp chồng thời gian chờ I/O nhưng tốn tài nguyên")
	fmt.Println("  và cần khóa khi dùng chung dữ liệu; bất đồng bộ phục vụ nhiều")
	fmt.Println("  I/O bằng một luồng nhưng không được làm việc CPU trên vòng")
	fmt.Println("  lặp; phối hợp vòng lặp + nhóm luồng có được cả hai ưu điểm.")
}
