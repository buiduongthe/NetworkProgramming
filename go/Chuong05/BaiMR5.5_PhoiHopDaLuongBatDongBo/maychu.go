package main

// Server: một goroutine "vòng-lặp" đóng vai vòng lặp sự kiện, phối hợp với nhóm worker tính và luồng nền.
//
// Go không có vòng lặp sự kiện dựng sẵn (và không cần: runtime tự lập lịch goroutine, tự đưa lời gọi chặn
// sang luồng HĐH khác). Để so sánh với asyncio/tokio, bài dựng kiểu "actor": goroutine vòng-lặp sở hữu
// toàn bộ trạng thái (không cần khóa) và select trên các kênh:
//
//	chYC     ← goroutine kết nối: đã đọc xong dòng lệnh + dữ liệu, chờ câu trả lời trên kênh riêng;
//	chSuKien ← goroutine tra cứu cũ xong, worker tính xong, hết hạn chót, cảm biến trả lời theo yêu cầu;
//	chMau    ← luồng cảm biến (goroutine ghim vào một luồng HĐH bằng runtime.LockOSThread) mỗi 50 ms;
//	nhip.C   ← time.Ticker 10 ms: nhịp tim đo độ trễ của vòng lặp.
//
// Nếu vòng lặp tự làm việc CPU/chặn (chế độ chan), mọi kênh khác phải chờ – giống chặn vòng lặp asyncio.
import (
	"bufio"
	"context"
	"fmt"
	"io"
	"net"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"
)

type yeuCau struct {
	p      []string
	data   []byte
	traLoi chan string
}

// dangXuLy – một yêu cầu BAM đang được phối hợp.
type dangXuLy struct {
	ma, cheDo  string
	data       []byte
	vong, han  int
	traLoi     chan string
	ctx        context.Context
	cancel     context.CancelFunc
	xong, ghi  bool
	giuCongVao bool
}

type suKienCuXong struct{ d *dangXuLy }
type suKienTinhXong struct {
	d       *dangXuLy
	hex     string
	dungO   int
	tenTinh string
}
type suKienHetGio struct{ d *dangXuLy }
type suKienCamBien struct {
	traLoi  chan string
	ma      string
	giaTri  int
	tenGiao string
}
type mau struct {
	k    int
	tTao time.Time
}

type mayChu struct {
	chYC      chan yeuCau
	chSuKien  chan any
	chMau     chan mau
	viecTinh  chan *dangXuLy
	ycCamBien chan suKienCamBien
	// trạng thái – chỉ goroutine vòng-lặp đọc/ghi
	treNhip, treMau  float64
	soMau, soYC      int
	oNhom, oNhomMax  int
	dangVao          int
	choVao           []*dangXuLy
	luongTraCuu      map[string]bool
	luongBam         map[string]bool
	soGoroutineTraCu int
}

func (m *mayChu) datLai() {
	m.treNhip, m.treMau = 0, 0
	m.soMau, m.soYC, m.oNhomMax = 0, 0, 0
	m.luongTraCuu, m.luongBam = map[string]bool{}, map[string]bool{}
	m.soGoroutineTraCu = 0
}

// camBien – luồng nền: goroutine ghim vào MỘT luồng HĐH riêng, chạy vòng lặp chặn của riêng nó.
func (m *mayChu) camBien() {
	runtime.LockOSThread() // từ đây goroutine này luôn chạy trên cùng một luồng HĐH
	k := 0
	for {
		select {
		case yc := <-m.ycCamBien: // vòng lặp nhờ đọc ngay → giao kết quả lại qua kênh sự kiện
			k++
			yc.giaTri, yc.tenGiao = (k*37)%100, "cảm-biến"
			m.chSuKien <- yc
		case <-time.After(camBienMs * time.Millisecond):
			k++
			m.chMau <- mau{k, time.Now()}
		}
	}
}

// worker tính: nhận việc từ kênh viecTinh, băm, báo kết quả về vòng lặp.
func (m *mayChu) worker(ten string) {
	for d := range m.viecTinh {
		hex, r := bam(d.ctx, d.data, d.vong)
		m.chSuKien <- suKienTinhXong{d, hex, r, ten}
	}
}

func (m *mayChu) batDau(d *dangXuLy) {
	if d.cheDo == "gioihan" {
		m.dangVao++
		d.giuCongVao = true
	}
	m.soGoroutineTraCu++
	go func() { // hàm chặn chạy trong goroutine riêng: runtime cấp luồng HĐH khác nếu cần
		traCuuCu()
		m.chSuKien <- suKienCuXong{d}
	}()
}

func (m *mayChu) traCongVao(d *dangXuLy) {
	if !d.giuCongVao {
		return
	}
	d.giuCongVao = false
	m.dangVao--
	if len(m.choVao) > 0 { // còn yêu cầu chờ ở cổng vào → cho yêu cầu kế tiếp vào
		next := m.choVao[0]
		m.choVao = m.choVao[1:]
		m.batDau(next)
	}
}

func (m *mayChu) xuLyBam(y yeuCau) {
	p := y.p
	n, _ := strconv.Atoi(p[3])
	vong, _ := strconv.Atoi(p[4])
	han, _ := strconv.Atoi(p[5])
	d := &dangXuLy{ma: p[1], cheDo: p[2], data: y.data, vong: vong, han: han, traLoi: y.traLoi}
	d.ghi = d.ma == "1" || d.ma == "101" || d.ma == "102"
	if d.ghi {
		logAi("vòng-lặp", "BAM %s (%s): nhận %d byte, %d vòng, hạn %d ms", d.ma, d.cheDo, n, vong, han)
	}
	m.soYC++
	if d.cheDo == "chan" { // A. SAI: làm ngay trên vòng lặp
		traCuuCu()
		m.luongTraCuu["vòng-lặp"] = true
		hex, _ := bam(context.Background(), d.data, d.vong)
		m.luongBam["vòng-lặp"] = true
		if d.ghi {
			logAi("vòng-lặp", "BAM %s: băm xong trên vòng-lặp, trả lời OK", d.ma)
		}
		d.traLoi <- "OK " + d.ma + " " + hex
		return
	}
	d.ctx, d.cancel = context.WithTimeout(context.Background(), time.Duration(han)*time.Millisecond)
	context.AfterFunc(d.ctx, func() { // khi ctx hết hạn → báo vòng lặp (chạy trên goroutine khác)
		if d.ctx.Err() == context.DeadlineExceeded {
			m.chSuKien <- suKienHetGio{d}
		}
	})
	if d.cheDo == "gioihan" && m.dangVao >= gioiHan {
		m.choVao = append(m.choVao, d) // áp lực ngược: chờ ở cổng vào, vòng lặp không bị chặn
		return
	}
	m.batDau(d)
}

func (m *mayChu) suKien(e any) {
	switch e := e.(type) {
	case suKienCuXong:
		d := e.d
		if d.xong { // đã quá hạn khi đang tra cứu → không gửi việc sang nhóm tính
			m.traCongVao(d)
			return
		}
		m.oNhom++
		m.oNhomMax = max(m.oNhomMax, m.oNhom)
		m.viecTinh <- d
	case suKienTinhXong:
		d := e.d
		m.oNhom--
		m.traCongVao(d)
		if e.dungO >= 0 {
			say("[server] BAM %s: luồng tính thấy cờ hủy → dừng ở vòng %d", d.ma, e.dungO)
			return
		}
		m.luongBam[e.tenTinh] = true
		if !d.xong {
			d.xong = true
			d.cancel()
			if d.ghi {
				logAi("vòng-lặp", "BAM %s: băm xong trên %s, trả lời OK", d.ma, e.tenTinh)
			}
			d.traLoi <- "OK " + d.ma + " " + e.hex
		}
	case suKienHetGio:
		d := e.d
		if !d.xong {
			d.xong = true
			logAi("vòng-lặp", "BAM %s: quá hạn %d ms → trả HETGIO, bật cờ hủy cho luồng tính", d.ma, d.han)
			d.traLoi <- "HETGIO " + d.ma
		}
	case suKienCamBien:
		logAi("vòng-lặp", "CAMBIEN %s: kết quả gửi qua kênh từ goroutine %s, giá trị %d", e.ma, e.tenGiao, e.giaTri)
		e.traLoi <- fmt.Sprintf("CAMBIEN %s %d %s", e.ma, e.giaTri, e.tenGiao)
	}
}

func tenTap(s map[string]bool) string {
	if len(s) == 0 {
		return "–"
	}
	var a []string
	for k := range s {
		a = append(a, k)
	}
	sort.Strings(a)
	return strings.Join(a, ", ")
}

func (m *mayChu) thongKe(nhan string) string {
	say("[server] đợt \"%s\": %d yêu cầu", nhan, m.soYC)
	say("  %-15sluồng đã chạy", "giai đoạn")
	say("  %-15s%s", "nhận/trả lời", "goroutine kết nối (đọc/ghi socket), vòng-lặp (điều phối)")
	traCuu := tenTap(m.luongTraCuu)
	if m.soGoroutineTraCu > 0 {
		traCuu = fmt.Sprintf("%d goroutine riêng (mỗi lời gọi một goroutine)", m.soGoroutineTraCu)
	}
	say("  %-15s%s", "tra cứu cũ", traCuu)
	say("  %-15s%s", "băm SHA-256", tenTap(m.luongBam))
	say("  trễ nhịp tối đa %.1f ms; nhóm tính tối đa %d việc", m.treNhip, m.oNhomMax)
	say("  cảm biến %d mẫu, trễ giao tối đa %.1f ms", m.soMau, m.treMau)
	return fmt.Sprintf("TK %.1f %d %.1f %d", m.treNhip, m.soMau, m.treMau, m.oNhomMax)
}

// vongLap – vòng lặp sự kiện: MỘT goroutine, select trên mọi nguồn sự kiện.
func (m *mayChu) vongLap() {
	nhip := time.NewTicker(nhipMs * time.Millisecond)
	truoc := time.Now()
	for {
		select {
		case y := <-m.chYC:
			switch y.p[0] {
			case "BAM":
				m.xuLyBam(y)
			case "CAMBIEN":
				m.ycCamBien <- suKienCamBien{traLoi: y.traLoi, ma: y.p[1]}
			case "DATLAI":
				m.datLai()
				y.traLoi <- "OK"
			case "THONGKE":
				y.traLoi <- m.thongKe(strings.Join(y.p[1:], " "))
			}
		case e := <-m.chSuKien:
			m.suKien(e)
		case mu := <-m.chMau:
			m.soMau++
			m.treMau = max(m.treMau, float64(time.Since(mu.tTao).Microseconds())/1000)
		case <-nhip.C:
			bay := time.Now()
			tre := float64(bay.Sub(truoc).Microseconds())/1000 - nhipMs // thức dậy muộn bao nhiêu ms
			truoc = bay
			m.treNhip = max(m.treNhip, tre)
		}
	}
}

// ketNoi – goroutine của một kết nối: đọc yêu cầu, chuyển cho vòng lặp, ghi câu trả lời.
func (m *mayChu) ketNoi(conn net.Conn) {
	defer conn.Close()
	r := bufio.NewReader(conn)
	line, err := r.ReadString('\n')
	p := strings.Fields(line)
	if err != nil || len(p) == 0 {
		return
	}
	y := yeuCau{p: p, traLoi: make(chan string, 1)}
	if p[0] == "BAM" && len(p) == 6 {
		n, _ := strconv.Atoi(p[3])
		y.data = make([]byte, n)
		if _, err := io.ReadFull(r, y.data); err != nil {
			return
		}
	}
	m.chYC <- y
	fmt.Fprintf(conn, "%s\n", <-y.traLoi)
}

func runServer(host, port string) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		say("Lỗi: %v", err)
		return
	}
	m := &mayChu{chYC: make(chan yeuCau, 256), chSuKien: make(chan any, 256), chMau: make(chan mau, 1024),
		viecTinh: make(chan *dangXuLy, 256), ycCamBien: make(chan suKienCamBien, 16)}
	m.datLai()
	for i := 1; i <= nhomTinh; i++ {
		go m.worker(fmt.Sprintf("tính-%d", i))
	}
	go m.camBien()
	go m.vongLap()
	say("[server] lắng nghe %s:%s; vòng lặp sự kiện trên goroutine \"vòng-lặp\", nhóm tính %d goroutine,", host, port, nhomTinh)
	say("  nhịp tim %d ms, luồng cảm biến mỗi %d ms, chế độ C giới hạn %d yêu cầu", nhipMs, camBienMs, gioiHan)
	say("Đặc thù Go: không có vòng lặp sự kiện dựng sẵn – goroutine vòng-lặp sở hữu trạng thái và select trên")
	say("  các kênh; runtime tự đưa lời gọi chặn sang luồng HĐH khác nên chỉ cần gọi hàm chặn trong goroutine riêng.")
	for {
		conn, err := ln.Accept()
		if err != nil {
			return
		}
		go m.ketNoi(conn)
	}
}
