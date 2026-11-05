package video

// Phong – lõi server hội thoại video dạng đối tượng (không biến toàn cục), cho ứng dụng giao diện.
// Giao thức và quy tắc giống hệt bản dòng lệnh (server.go ở thư mục cha), không thêm thông điệp:
//  1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
//  2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
//  3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
//  4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và fps chung do server đặt.
//  5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn, không vượt fps chung.
//  6. Server tự thực thi: bỏ khung của camera đang tắt, khung đến nhanh hơn fps cho phép, sai nguồn.
//
// Server vẫn là MCU: ghép lưới các ô, nén, gửi cho từng client. Thêm cho giao diện: TrangThai() trả
// về ảnh mới nhất của từng client để cửa sổ server vẽ lưới camera (mỗi client một ô).

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
	ma                uint16
	ten, ip           string
	conn              net.Conn
	khoaGui           sync.Mutex
	udp               *net.UDPAddr
	tuTat, serverTat  bool
	fpsToiDa, chia    int
	nhanFps, nhanChia int
	ghep              *GhepKhung
	o                 []byte // ô 160×120 có nhãn – dùng khi ghép lưới gửi cho client
	anh               []byte // ảnh mới nhất đã phóng về 160×120, không nhãn – cho cửa sổ server
	soAnh             int    // tăng mỗi khi có ảnh mới (giao diện chỉ vẽ lại khi đổi)
	lucNhanCuoi       time.Time
	boVuotFps, boTat  int
	demKhung, demByte int
	fpsVao, kbpsVao   float64
	soKhungRa         uint32
	hanGuiRa          time.Time
	demRa, byteRa     int
	fpsRa, kbpsRa     float64
}

func (k *thanhVien) camMo() bool { return !k.tuTat && !k.serverTat }

func (k *thanhVien) lyDo() string {
	switch {
	case k.camMo():
		return "mở"
	case k.tuTat && k.serverTat:
		return "tắt (cả hai)"
	case k.serverTat:
		return "tắt (server)"
	}
	return "tắt (tự tắt)"
}

func (k *thanhVien) gui(v any) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	k.conn.SetWriteDeadline(time.Now().Add(2 * time.Second))
	GuiJSON(k.conn, v)
}

// TrangThaiCamera – ảnh chụp trạng thái một client để giao diện vẽ.
type TrangThaiCamera struct {
	Ma                      uint16
	Ten, DiaChi, LyDo       string
	CamMo, TuTat, ServerTat bool
	FpsToiDa, Chia          int
	NhanFps, NhanChia       int
	FpsVao, KbpsVao         float64
	FpsRa, KbpsRa           float64
	Anh                     []byte // 160×120 RGB (nil: chưa có hình)
	SoAnh                   int
	KhungDu, KhungHong, Mat int
}

type Phong struct {
	mu       sync.Mutex
	ds       map[uint16]*thanhVien
	maMoi    uint16
	fpsChung int
	congUDP  int
	ln       net.Listener
	udp      *net.UDPConn
	dung     chan struct{}
	nhatKy   func(string)
}

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
	udp.SetReadBuffer(1 << 20)
	p := &Phong{ds: map[uint16]*thanhVien{}, fpsChung: FpsMacDinh, congUDP: congUDP, ln: ln, udp: udp,
		dung: make(chan struct{}), nhatKy: nhatKy}
	p.log("Server chạy: TCP %s:%d điều khiển, UDP %d video", host, congTCP, congUDP)
	go p.chapNhan()
	go p.nhanUDP()
	go p.vongGui()
	return p, nil
}

func (p *Phong) log(format string, a ...any) {
	if p.nhatKy != nil {
		p.nhatKy(fmt.Sprintf(format, a...))
	}
}

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

func (k *thanhVien) guiCamera() {
	k.gui(map[string]any{"loai": "camera", "mo": k.camMo(), "ly_do": k.lyDo(), "fps": k.fpsToiDa, "chia": k.chia})
	k.veO(nil, 0, 0)
}

func (p *Phong) guiNhan(k *thanhVien) {
	k.gui(map[string]any{"loai": "nhan", "fps": min(k.nhanFps, p.fpsChung), "chia": k.nhanChia})
}

// veO – giống bản dòng lệnh: ô của client trong lưới gửi đi = hình phóng về 160×120 + dải nhãn.
func (k *thanhVien) veO(anh []byte, w, h int) {
	var o []byte
	switch {
	case anh != nil:
		o = PhongTo(anh, w, h, Rong/w)
		k.anh = append([]byte(nil), o...)
		k.soAnh++
	case k.camMo() && k.o != nil:
		o = k.o
	default:
		o = make([]byte, Rong*Cao*3)
		ToMau(o, Rong, 0, 0, Rong, Cao, Mau{40, 40, 40})
		if !k.camMo() {
			VeChu(o, Rong, Cao, 20, 44, "TAT CAMERA", 2, Mau{255, 255, 255})
			lydo := "(TU TAT)"
			if k.serverTat {
				lydo = "(SERVER)"
			}
			VeChu(o, Rong, Cao, 44, 66, lydo, 2, Mau{255, 255, 255})
			k.anh = nil
			k.soAnh++
		}
	}
	ToMau(o, Rong, 0, Cao-11, Rong, 11, Mau{0, 0, 0})
	nhan := fmt.Sprintf("%s %dFPS", k.ten, k.fpsToiDa)
	if k.chia > 1 {
		nhan += fmt.Sprintf(" 1/%d", k.chia)
	}
	VeChu(o, Rong, Cao, 2, Cao-9, nhan, 1, Mau{255, 255, 0})
	k.o = o
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
	k := &thanhVien{ma: p.maMoi, ten: ten, ip: addr.IP.String(), conn: c, fpsToiDa: FpsMacDinh, chia: 1,
		nhanFps: FpsMacDinh, nhanChia: 1, ghep: MoiGhep()}
	p.ds[k.ma] = k
	k.gui(map[string]any{"loai": "chao", "ma": k.ma, "cong_udp": p.congUDP, "rong": Rong, "cao": Cao})
	p.log("#%d %s vào phòng từ %s", k.ma, k.ten, addr)
	k.guiCamera()
	p.guiNhan(k)
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
	case "tu_tat_camera":
		k.tuTat = true
		k.guiCamera()
		p.phatSuKien(k.ten + " tự tắt camera")
	case "tu_bat_camera":
		k.tuTat = false
		if k.serverTat {
			p.log("%s xin bật camera -> từ chối (server đang tắt)", k.ten)
			k.gui(map[string]any{"loai": "tu_choi", "ly_do": "server đang tắt camera của bạn"})
		} else {
			k.guiCamera()
			p.phatSuKien(k.ten + " tự bật camera")
		}
	case "nhan":
		k.nhanFps = max(1, min(30, SoNguyen(tin["fps"], k.nhanFps)))
		if SoNguyen(tin["chia"], k.nhanChia) == 2 {
			k.nhanChia = 2
		} else {
			k.nhanChia = 1
		}
		p.log("%s chọn nhận %d fps, độ phân giải 1/%d", k.ten, k.nhanFps, k.nhanChia)
		p.guiNhan(k)
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
		h, payload, ok := TachHeader(buf[:n])
		if !ok {
			continue
		}
		bayGio := time.Now()
		p.mu.Lock()
		p.xuLyManh(h, payload, addr, bayGio)
		p.mu.Unlock()
	}
}

func (p *Phong) xuLyManh(h Header, payload []byte, addr *net.UDPAddr, bayGio time.Time) {
	k := p.ds[h.Ma]
	if k == nil {
		return
	}
	if k.udp == nil && addr.IP.String() == k.ip {
		k.udp = addr
		p.log("#%d %s đăng ký UDP %s", k.ma, k.ten, addr)
	}
	if k.udp == nil || addr.String() != k.udp.String() || h.Loai != LoaiManh {
		return
	}
	nen := k.ghep.Them(h, payload, bayGio)
	if nen == nil {
		return
	}
	if !k.camMo() {
		k.boTat++
		return
	}
	if bayGio.Sub(k.lucNhanCuoi).Seconds() < 0.8/float64(k.fpsToiDa) {
		k.boVuotFps++
		return
	}
	anh, err := GiaiNen(nen)
	w, c := int(h.W), int(h.H)
	if err != nil || w == 0 || w*c*3 != len(anh) || Rong%w != 0 || w*Cao != c*Rong {
		return
	}
	k.lucNhanCuoi = bayGio
	k.demKhung++
	k.demByte += len(nen)
	k.veO(anh, w, c)
}

// ---------------------------------------------------------------- Ghép lưới và gửi

func ghepLuoi(ds []*thanhVien) ([]byte, int, int) {
	n := max(1, len(ds))
	cot := int(math.Ceil(math.Sqrt(float64(n))))
	hang := (n + cot - 1) / cot
	W, H := cot*Rong, hang*Cao
	ra := make([]byte, W*H*3)
	ToMau(ra, W, 0, 0, W, H, Mau{15, 15, 15})
	for i, k := range ds {
		x0, y0 := (i%cot)*Rong, (i/cot)*Cao
		for y := 0; y < Cao; y++ {
			copy(ra[((y0+y)*W+x0)*3:], k.o[y*Rong*3:(y+1)*Rong*3])
		}
	}
	return ra, W, H
}

func (p *Phong) danhSach() []*thanhVien {
	var ds []*thanhVien
	for _, k := range p.ds {
		ds = append(ds, k)
	}
	sort.Slice(ds, func(i, j int) bool { return ds[i].ma < ds[j].ma })
	return ds
}

// vongGui – mỗi 5 ms: người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ, nén, chia mảnh, gửi.
func (p *Phong) vongGui() {
	mocDo := time.Now()
	for {
		select {
		case <-p.dung:
			return
		default:
		}
		bayGio := time.Now()
		p.mu.Lock()
		var ds []*thanhVien
		for _, k := range p.danhSach() {
			if k.o != nil {
				ds = append(ds, k)
			}
		}
		for _, k := range p.ds {
			fps := min(k.nhanFps, p.fpsChung)
			if k.udp == nil || len(ds) == 0 || bayGio.Before(k.hanGuiRa) {
				continue
			}
			k.hanGuiRa = k.hanGuiRa.Add(time.Second / time.Duration(fps))
			if k.hanGuiRa.Before(bayGio) {
				k.hanGuiRa = bayGio
			}
			anh, w, h := ghepLuoi(ds)
			anh, w, h = ThuNho(anh, w, h, k.nhanChia)
			nen := NenZlib(anh)
			for _, g := range ChiaManh(MaServer, k.soKhungRa, w, h, nen) {
				p.udp.WriteToUDP(g, k.udp)
			}
			k.soKhungRa++
			k.demRa++
			k.byteRa += len(nen)
		}
		if bayGio.Sub(mocDo) >= time.Second {
			for _, k := range p.ds {
				k.fpsVao, k.kbpsVao = float64(k.demKhung), float64(k.demByte)*8/1000
				k.fpsRa, k.kbpsRa = float64(k.demRa), float64(k.byteRa)*8/1000
				k.demKhung, k.demByte, k.demRa, k.byteRa = 0, 0, 0, 0
			}
			mocDo = mocDo.Add(time.Second)
		}
		p.mu.Unlock()
		time.Sleep(5 * time.Millisecond)
	}
}

// ---------------------------------------------------------------- Lệnh từ giao diện

func (p *Phong) chon(ma uint16) []*thanhVien {
	var kq []*thanhVien
	for _, k := range p.danhSach() {
		if ma == 0 || k.ma == ma {
			kq = append(kq, k)
		}
	}
	return kq
}

func tenAi(ds []*thanhVien, ma uint16) string {
	if ma == 0 || len(ds) == 0 {
		return "tất cả"
	}
	return ds[0].ten
}

// TatCamera – tat=true: server tắt camera; false: gỡ lệnh tắt. ma = 0: tất cả.
func (p *Phong) TatCamera(ma uint16, tat bool) {
	p.mu.Lock()
	defer p.mu.Unlock()
	ds := p.chon(ma)
	var vanTat []string
	for _, k := range ds {
		k.serverTat = tat
		k.guiCamera()
		if k.tuTat {
			vanTat = append(vanTat, k.ten)
		}
	}
	if tat {
		p.phatSuKien("server tắt camera " + tenAi(ds, ma))
		return
	}
	them := ""
	if len(vanTat) > 0 {
		them = " (" + strings.Join(vanTat, ", ") + " vẫn tự tắt)"
	}
	p.phatSuKien("server bật camera " + tenAi(ds, ma) + them)
}

func (p *Phong) DatFpsToiDa(ma uint16, n int) {
	p.mu.Lock()
	defer p.mu.Unlock()
	ds := p.chon(ma)
	for _, k := range ds {
		k.fpsToiDa = max(1, min(30, n))
		k.guiCamera()
	}
	p.phatSuKien(fmt.Sprintf("server giới hạn %s gửi tối đa %d fps", tenAi(ds, ma), n))
}

func (p *Phong) DatDoPhanGiai(ma uint16, chia int) {
	if chia != 1 && chia != 2 && chia != 4 {
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	ds := p.chon(ma)
	for _, k := range ds {
		k.chia = chia
		k.guiCamera()
	}
	p.phatSuKien(fmt.Sprintf("server đặt độ phân giải %s = 1/%d", tenAi(ds, ma), chia))
}

func (p *Phong) DatFpsChung(n int) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.fpsChung = max(1, min(30, n))
	for _, k := range p.ds {
		p.guiNhan(k)
	}
	p.phatSuKien(fmt.Sprintf("server đặt fps chung tối đa %d", p.fpsChung))
}

// TrangThai – ảnh chụp cho giao diện; daCo[ma] = SoAnh giao diện đã vẽ, để khỏi chép ảnh không đổi.
func (p *Phong) TrangThai(daCo map[uint16]int) ([]TrangThaiCamera, int) {
	p.mu.Lock()
	defer p.mu.Unlock()
	var kq []TrangThaiCamera
	for _, k := range p.danhSach() {
		diaChi := k.ip
		if k.udp != nil {
			diaChi = k.udp.String()
		}
		t := TrangThaiCamera{k.ma, k.ten, diaChi, k.lyDo(), k.camMo(), k.tuTat, k.serverTat, k.fpsToiDa, k.chia,
			k.nhanFps, k.nhanChia, k.fpsVao, k.kbpsVao, k.fpsRa, k.kbpsRa, nil, k.soAnh,
			k.ghep.KhungDu, k.ghep.KhungHong, k.ghep.ManhMat}
		if daCo[k.ma] != k.soAnh && k.anh != nil {
			t.Anh = append([]byte(nil), k.anh...)
		}
		kq = append(kq, t)
	}
	return kq, p.fpsChung
}
