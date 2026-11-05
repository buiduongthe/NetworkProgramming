package amthanh

// Phong – lõi server hội thoại âm thanh dạng đối tượng (không dùng biến toàn cục), cho ứng dụng
// giao diện. Giao thức và quy tắc giống hệt bản dòng lệnh (server.go ở thư mục cha):
//  1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
//  2. Server khóa thắng client: đang bị khóa thì client xin bật mic bị từ chối ("tu_choi").
//  3. Server gỡ khóa KHÔNG bật hộ mic mà người dùng đã tự tắt (quyền riêng tư).
//  4. Độ lợi từng client và âm lượng chung do server đặt, áp dụng khi trộn; mỗi lần đổi độ lợi,
//     server báo client {"loai":"do_loi","gia_tri":g} để client hiện "Server Vol" (thông điệp thêm,
//     client cũ bỏ qua được).
//  5. Âm lượng nghe do client tự đặt, áp dụng tại client; server chỉ hiển thị.
//  6. Server tự thực thi: bỏ gói từ client đang tắt mic, gói sai địa chỉ nguồn.
//
// Phần thêm cho giao diện: server có thể NÓI (micro của máy server là một nguồn trong bộ trộn với
// SSRC_SERVER = 0, mọi client đều nghe, không đi qua mạng tới server) và NGHE (phát ra loa của máy
// server tổng tiếng các client, không gồm tiếng chính server).
//
// Mọi trạng thái được bảo vệ bằng một khóa (mu); các goroutine mạng và vòng trộn 20 ms chạy nền.
// Giao diện chỉ gọi các phương thức công khai, đọc ảnh chụp trạng thái bằng TrangThai(); hàm
// nhatKy được gọi từ goroutine nền – phía giao diện phải tự chuyển về luồng giao diện (fyne.Do).

import (
	"bufio"
	"encoding/json"
	"fmt"
	"math"
	"net"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

type thanhVien struct {
	ma          uint32
	ten, ip     string
	conn        net.Conn
	khoaGui     sync.Mutex
	udp         *net.UDPAddr
	tuTat       bool
	serverKhoa  bool
	doLoi       float64
	amLuongNghe float64
	boDem       *BoDemJitter
	seqRa       uint16
	tsRa        uint32
	mucDinh     float64 // mức lớn nhất kể từ lần giao diện đọc gần nhất (giữ đỉnh cho thanh đo)
	biChan      int
	saiNguon    int
}

func (k *thanhVien) micMo() bool { return !k.tuTat && !k.serverKhoa }

func (k *thanhVien) lyDo() string {
	switch {
	case k.micMo():
		return "mở"
	case k.tuTat && k.serverKhoa:
		return "tắt (cả hai)"
	case k.serverKhoa:
		return "tắt (server)"
	}
	return "tắt (tự tắt)"
}

func (k *thanhVien) gui(v any) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	k.conn.SetWriteDeadline(time.Now().Add(2 * time.Second)) // client treo không làm treo server
	GuiJSON(k.conn, v)
}

// TrangThaiThanhVien – ảnh chụp trạng thái một client để giao diện hiển thị.
type TrangThaiThanhVien struct {
	Ma                 uint32
	Ten, DiaChi, Mic   string
	MicMo, TuTat, Khoa bool
	DoLoi, AmLuongNghe float64
	MucDB              float64 // dBFS tiếng vào (đỉnh trong ~100 ms gần nhất)
	GoiNhan, Mat       int
	JitterMs           float64
}

type Phong struct {
	mu           sync.Mutex
	ds           map[uint32]*thanhVien
	maMoi        uint32
	amLuongChung float64
	congUDP      int
	ln           net.Listener
	udp          *net.UDPConn
	t0           time.Time
	dung         chan struct{}
	nhatKy       func(string)
	// tiếng của chính server
	nguonServer   NguonAm
	heSoMicServer float64
	mucServer     float64
	nghe          bool // server đang "nghe" (đo mức tổng tiếng client; phát ra loa nếu có)
	loa           *Loa
	mucNghe       float64
	DiaChi        string // "host:tcp/udp" để hiển thị
}

// MoPhong – mở cổng TCP (điều khiển) và UDP (RTP), chạy các goroutine nền.
func MoPhong(host string, congTCP, congUDP int, nhatKy func(string)) (*Phong, error) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(congTCP)))
	if err != nil {
		return nil, err
	}
	udp, err := net.ListenUDP("udp", &net.UDPAddr{IP: net.ParseIP(host), Port: congUDP})
	if err != nil {
		ln.Close()
		return nil, err
	}
	p := &Phong{ds: map[uint32]*thanhVien{}, amLuongChung: 1, congUDP: congUDP, ln: ln, udp: udp,
		t0: time.Now(), dung: make(chan struct{}), nhatKy: nhatKy, heSoMicServer: 1,
		mucServer: math.Inf(-1), mucNghe: math.Inf(-1),
		DiaChi: fmt.Sprintf("%s:%d (UDP %d)", host, congTCP, congUDP)}
	p.log("Server chạy: TCP %s:%d điều khiển, UDP %d âm thanh RTP", host, congTCP, congUDP)
	go p.chapNhan()
	go p.nhanUDP()
	go p.vongTron()
	return p, nil
}

func (p *Phong) log(format string, a ...any) {
	if p.nhatKy != nil {
		p.nhatKy(fmt.Sprintf(format, a...))
	}
}

// Dong – dừng server: đóng cổng, đóng mọi kết nối client (client thấy "server đóng kết nối").
func (p *Phong) Dong() {
	p.mu.Lock()
	select {
	case <-p.dung:
		p.mu.Unlock()
		return
	default:
	}
	close(p.dung)
	for _, k := range p.ds {
		k.conn.Close()
	}
	if p.loa != nil {
		p.loa.Dong()
		p.loa = nil
	}
	p.mu.Unlock()
	p.ln.Close()
	p.udp.Close()
	p.log("Server đã dừng")
}

func (p *Phong) phatSuKien(text string) {
	p.log("=> %s", text)
	for _, k := range p.ds {
		k.gui(map[string]any{"loai": "su_kien", "noi_dung": text})
	}
}

func (k *thanhVien) guiMic() {
	k.gui(map[string]any{"loai": "mic", "mo": k.micMo(), "ly_do": k.lyDo()})
	if !k.micMo() {
		k.boDem.Xoa()
	}
}

// ---------------------------------------------------------------- Kênh điều khiển TCP

func (p *Phong) chapNhan() {
	for {
		c, err := p.ln.Accept()
		if err != nil {
			return
		}
		c.(*net.TCPConn).SetNoDelay(true)
		go p.phucVu(c)
	}
}

func (p *Phong) phucVu(c net.Conn) {
	defer c.Close()
	r := bufio.NewScanner(c)
	if !r.Scan() {
		return
	}
	var tin map[string]any
	if json.Unmarshal(r.Bytes(), &tin) != nil || tin["loai"] != "vao" {
		return
	}
	ten, _ := tin["ten"].(string)
	if ten == "" {
		return
	}
	if len(ten) > 8 {
		ten = ten[:8]
	}
	addr := c.RemoteAddr().(*net.TCPAddr)
	p.mu.Lock()
	p.maMoi++
	k := &thanhVien{ma: p.maMoi, ten: ten, ip: addr.IP.String(), conn: c, doLoi: 1, amLuongNghe: 1,
		boDem: MoiBoDem(3), mucDinh: math.Inf(-1)}
	p.ds[k.ma] = k
	k.gui(map[string]any{"loai": "chao", "ma": k.ma, "cong_udp": p.congUDP, "tan_so_mau": TanSoMau,
		"khung_ms": KhungMs})
	k.gui(map[string]any{"loai": "do_loi", "gia_tri": k.doLoi})
	p.log("#%d %s vào phòng từ %s", k.ma, k.ten, addr)
	k.guiMic()
	p.phatSuKien(k.ten + " vào phòng")
	p.mu.Unlock()
	for r.Scan() {
		var tin map[string]any
		if json.Unmarshal(r.Bytes(), &tin) != nil {
			break
		}
		p.mu.Lock()
		p.xuLyTin(k, tin)
		p.mu.Unlock()
	}
	p.mu.Lock()
	delete(p.ds, k.ma)
	select {
	case <-p.dung:
	default:
		p.log("#%d %s rời phòng", k.ma, k.ten)
		p.phatSuKien(k.ten + " rời phòng")
	}
	p.mu.Unlock()
}

func (p *Phong) xuLyTin(k *thanhVien, tin map[string]any) {
	switch tin["loai"] {
	case "tu_tat_mic":
		k.tuTat = true
		k.guiMic()
		p.phatSuKien(k.ten + " tự tắt mic")
	case "tu_bat_mic":
		k.tuTat = false
		if k.serverKhoa { // quy tắc 2
			p.log("%s xin bật mic -> từ chối (server đang khóa)", k.ten)
			k.gui(map[string]any{"loai": "tu_choi", "ly_do": "server đang khóa mic của bạn"})
		} else {
			k.guiMic()
			p.phatSuKien(k.ten + " tự bật mic")
		}
	case "am_luong_nghe":
		if v, ok := tin["gia_tri"].(float64); ok {
			k.amLuongNghe = v
		}
		p.log("%s tự đặt âm lượng nghe %.0f%%", k.ten, k.amLuongNghe*100)
	}
}

// ---------------------------------------------------------------- Kênh dữ liệu UDP

func (p *Phong) nhanUDP() {
	buf := make([]byte, 2048)
	for {
		n, addr, err := p.udp.ReadFromUDP(buf)
		if err != nil {
			return
		}
		g, ok := TachRTP(buf[:n])
		if !ok {
			continue
		}
		p.mu.Lock()
		k := p.ds[g.Ssrc]
		switch {
		case k == nil:
		case k.udp == nil && addr.IP.String() != k.ip:
		default:
			if k.udp == nil {
				k.udp = addr
				p.log("#%d %s đăng ký UDP %s", k.ma, k.ten, addr)
			}
			if addr.String() != k.udp.String() {
				k.saiNguon++
			} else if g.Pt == PtAmThanh && len(g.Payload) == ByteMoiKhung {
				if !k.micMo() {
					k.biChan++
				} else {
					k.boDem.NhanGoi(g.Seq, g.Ts, g.Payload, time.Since(p.t0).Seconds())
				}
			}
		}
		p.mu.Unlock()
	}
}

// vongTron – mỗi 20 ms: tổng T = Σ (độ lợi × tiếng client) + tiếng server; client k nhận T trừ
// tiếng của chính k (mix-minus) × âm lượng chung. Loa của server phát Σ tiếng client × âm lượng chung.
func (p *Phong) vongTron() {
	tick := time.NewTicker(KhungMs * time.Millisecond)
	defer tick.Stop()
	for {
		select {
		case <-p.dung:
			return
		case <-tick.C:
		}
		p.mu.Lock()
		vao := map[uint32][]float64{}
		tongKhach := make([]float64, MauMoiKhung)
		for _, k := range p.ds {
			var goi []byte
			if k.micMo() {
				goi = k.boDem.Lay()
			}
			if goi == nil {
				continue
			}
			mau := ByteSangPCM(goi)
			k.mucDinh = math.Max(k.mucDinh, MucDBFS(mau))
			v := make([]float64, MauMoiKhung)
			for i, x := range mau {
				v[i] = k.doLoi * float64(x)
				tongKhach[i] += v[i]
			}
			vao[k.ma] = v
		}
		tong := append([]float64(nil), tongKhach...)
		if p.nguonServer != nil { // server nói: một nguồn nữa trong bộ trộn (SSRC 0)
			if mau := p.nguonServer.LayKhung(); mau != nil {
				for i := range mau {
					mau[i], _ = CatNgon(float64(mau[i]) * p.heSoMicServer)
					tong[i] += float64(mau[i])
				}
				p.mucServer = math.Max(p.mucServer, MucDBFS(mau))
			}
		}
		for _, k := range p.ds {
			if k.udp == nil {
				continue
			}
			rieng := vao[k.ma]
			ra := make([]int16, MauMoiKhung)
			for i := range ra {
				v := tong[i]
				if rieng != nil {
					v -= rieng[i]
				}
				ra[i], _ = CatNgon(v * p.amLuongChung)
			}
			p.udp.WriteToUDP(DongGoiRTP(PtAmThanh, k.seqRa, k.tsRa, SsrcServer, PcmSangByte(ra)), k.udp)
			k.seqRa++
			k.tsRa += MauMoiKhung
		}
		if p.nghe { // server nghe: chỉ tiếng các client
			ra := make([]int16, MauMoiKhung)
			for i := range ra {
				ra[i], _ = CatNgon(tongKhach[i] * p.amLuongChung)
			}
			p.mucNghe = math.Max(p.mucNghe, MucDBFS(ra))
			if p.loa != nil {
				p.loa.Ghi(ra)
			}
		}
		p.mu.Unlock()
	}
}

// ---------------------------------------------------------------- Lệnh từ giao diện

func (p *Phong) chon(ma uint32) []*thanhVien {
	var kq []*thanhVien
	for _, k := range p.ds {
		if ma == 0 || k.ma == ma {
			kq = append(kq, k)
		}
	}
	sort.Slice(kq, func(i, j int) bool { return kq[i].ma < kq[j].ma })
	return kq
}

// KhoaMic – khoa=true: server tắt (khóa) mic; false: gỡ khóa. ma = 0: tất cả client.
func (p *Phong) KhoaMic(ma uint32, khoa bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	ds := p.chon(ma)
	if len(ds) == 0 {
		return
	}
	var vanTat []string
	for _, k := range ds {
		k.serverKhoa = khoa
		k.guiMic()
		if k.tuTat {
			vanTat = append(vanTat, k.ten)
		}
	}
	ai := "tất cả"
	if ma != 0 {
		ai = ds[0].ten
	}
	if khoa {
		p.phatSuKien("server tắt mic " + ai)
		return
	}
	them := ""
	if len(vanTat) > 0 { // quy tắc 3
		them = " (" + strings.Join(vanTat, ", ") + " vẫn tự tắt)"
	}
	p.phatSuKien("server bật mic " + ai + them)
}

// DatDoLoi – độ lợi đầu vào của một client (0…2 = 0…200 %), báo client bằng "do_loi".
func (p *Phong) DatDoLoi(ma uint32, g float64) {
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, k := range p.chon(ma) {
		if k.doLoi == g {
			continue
		}
		k.doLoi = g
		k.gui(map[string]any{"loai": "do_loi", "gia_tri": g})
		p.log("độ lợi %s = %.0f%%", k.ten, g*100)
	}
}

func (p *Phong) DatAmLuongChung(v float64) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.amLuongChung != v {
		p.amLuongChung = v
		p.log("âm lượng chung = %.0f%%", v*100)
	}
}

// DatNguonServer – micro của server (nil: server im lặng). Nguồn cũ do bên gọi tự đóng.
func (p *Phong) DatNguonServer(n NguonAm) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.nguonServer = n
}

func (p *Phong) DatHeSoMicServer(v float64) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.heSoMicServer = v
}

// DatNghe – bật/tắt việc nghe; l là loa (ffplay) hoặc nil nếu không phát được – khi đó vẫn đo mức.
func (p *Phong) DatNghe(bat bool, l *Loa) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.loa != nil && p.loa != l {
		p.loa.Dong()
	}
	p.nghe, p.loa = bat, l
}

// TrangThai – ảnh chụp để giao diện vẽ (khoảng 10 lần/giây). Đọc xong thì đặt lại mức đỉnh.
func (p *Phong) TrangThai() (ds []TrangThaiThanhVien, mucServer, mucNghe, amLuongChung float64) {
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, k := range p.chon(0) {
		diaChi := k.ip
		if k.udp != nil {
			diaChi = k.udp.String()
		}
		ds = append(ds, TrangThaiThanhVien{k.ma, k.ten, diaChi, k.lyDo(), k.micMo(), k.tuTat, k.serverKhoa,
			k.doLoi, k.amLuongNghe, k.mucDinh, k.boDem.Nhan, k.boDem.Mat(), k.boDem.JitterMs()})
		k.mucDinh = math.Inf(-1)
	}
	mucServer, mucNghe = p.mucServer, p.mucNghe
	p.mucServer, p.mucNghe = math.Inf(-1), math.Inf(-1)
	return ds, mucServer, mucNghe, p.amLuongChung
}
