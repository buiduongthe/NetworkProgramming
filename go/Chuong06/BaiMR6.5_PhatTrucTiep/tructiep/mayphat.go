package tructiep

// Lõi máy phát (camera + micro -> máy chủ), dạng đối tượng, không chạm giao diện.
//
// Dùng chung cho bản dòng lệnh (phat.go ở thư mục cha) và gui/phat_gui.go. Báo sự kiện bằng Bao(text);
// giao diện đọc số liệu (xem trước, mức micro, thống kê, danh sách người xem) qua TrangThai().
//
// Đồng hồ phương tiện chung: t = 0 khi bấm Phát (đồng hồ đơn điệu).
//   - Âm thanh: goroutine 20 ms; gói thứ n được gửi lúc t0 + n·20 ms (bám đồng hồ, không trôi), nhãn
//     RTP ts = n·320 (= số mẫu đã gửi, 16 kHz).
//   - Video: goroutine theo fps; nhãn = ms phương tiện lúc chụp khung. Khung 0 là I; I lặp lại mỗi
//     gop khung hoặc NGAY khung kế tiếp sau khi nhận xin_khung_khoa (đếm GOP lại từ khung I đó).
//   - Mỗi 1 s gửi {"loai":"dong_ho","ts":ms phương tiện,"gio":ms Unix} (như RTCP Sender Report –
//     RFC 3550 mục 6.4.1): người xem ánh xạ nhãn thời gian <-> giờ thực để đo độ trễ đầu-cuối.

import (
	"bufio"
	"errors"
	"fmt"
	"math"
	"net"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
)

type MayPhat struct {
	Ten              string
	Bao              func(string)
	mu, khoaTCP      sync.Mutex
	tcp              net.Conn
	udp              *net.UDPConn
	daKetNoi         bool
	Ma               uint16
	dangPhat         bool
	camera           Camera
	mic              Micro
	fps, gop         int
	nguoiXem         []string
	xemTruoc         []byte // khung vừa gửi
	tsXemTruoc       int64
	mucMic           float64
	matGoi           float64
	rng              *NgauNhien
	BoGiaLap         int
	t0               float64
	so, seq, nAm     int
	soICuoi          int // -1: chưa có I
	truoc            []byte
	canI             bool
	lyDoI            string
	soI, soP         int
	byteI, byteP     int
	goiAm, pliNhan   int
	pMin, pMax       int
	demI, demP       int
	demByteAm        int
	demByteHinh      int
	kbpsAm, kbpsHinh float64
	song             atomic.Bool
}

func MoiMayPhat(ten string, bao func(string), matGoi float64) *MayPhat {
	if bao == nil {
		bao = func(string) {}
	}
	m := &MayPhat{Ten: ten, Bao: bao, fps: FpsMacDinh, gop: GopMacDinh, mucMic: math.Inf(-1),
		matGoi: matGoi, rng: MoiNgauNhien(1), soICuoi: -1, tsXemTruoc: -1}
	m.song.Store(true)
	go m.vongAm()
	go m.vongHinh()
	return m
}

func (m *MayPhat) lamMoiThongKe() {
	m.soI, m.soP, m.byteI, m.byteP, m.goiAm, m.pliNhan = 0, 0, 0, 0, 0, 0
	m.pMin, m.pMax = -1, -1
	m.demI, m.demP, m.demByteAm, m.demByteHinh = 0, 0, 0, 0
	m.kbpsAm, m.kbpsHinh = 0, 0
}

// ------------------------------------------------------------ kết nối

// KetNoi – kết nối TCP, chào với vai trò "phat". Lỗi (kể cả server từ chối) -> error.
func (m *MayPhat) KetNoi(host string, cong int) (uint16, error) {
	tcp, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), 5*time.Second)
	if err != nil {
		return 0, err
	}
	if t, ok := tcp.(*net.TCPConn); ok {
		t.SetNoDelay(true)
	}
	r := bufio.NewReaderSize(tcp, 4096)
	tcp.SetDeadline(time.Now().Add(5 * time.Second))
	if err := GuiJSON(tcp, J("loai", "chao", "vai_tro", "phat", "ten", m.Ten)); err != nil {
		tcp.Close()
		return 0, err
	}
	tl, err := DocDong(r)
	if err != nil {
		tcp.Close()
		return 0, err
	}
	if tl.Chuoi("loai") == "loi" {
		tcp.Close()
		return 0, errors.New("server từ chối: " + tl.Chuoi("ly_do"))
	}
	if tl.Chuoi("loai") != "chao" {
		tcp.Close()
		return 0, errors.New("server không trả lời 'chao'")
	}
	tcp.SetDeadline(time.Time{})
	ua, err := net.ResolveUDPAddr("udp", net.JoinHostPort(host, strconv.Itoa(int(tl.So("cong_udp", 0)))))
	if err != nil {
		tcp.Close()
		return 0, err
	}
	udp, err := net.DialUDP("udp", nil, ua)
	if err != nil {
		tcp.Close()
		return 0, err
	}
	udp.SetWriteBuffer(1 << 20)
	m.mu.Lock()
	m.tcp, m.udp, m.Ma, m.daKetNoi = tcp, udp, uint16(tl.So("ma", 0)), true
	m.mu.Unlock()
	udp.Write(GoiGiu(m.Ma)) // đăng ký địa chỉ UDP ngay
	go m.docTCP(r)
	return m.Ma, nil
}

func (m *MayPhat) gui(v any) {
	m.mu.Lock()
	tcp := m.tcp
	m.mu.Unlock()
	if tcp == nil {
		return
	}
	m.khoaTCP.Lock()
	defer m.khoaTCP.Unlock()
	GuiJSON(tcp, v)
}

func (m *MayPhat) docTCP(r *bufio.Reader) {
	for {
		tin, err := DocDong(r)
		if err != nil {
			break
		}
		switch tin.Chuoi("loai") {
		case "xin_khung_khoa":
			tu := "?"
			if s, ok := tin["tu"].(string); ok {
				tu = s
			}
			m.mu.Lock()
			m.pliNhan++
			m.lyDoI = "PLI từ " + tu
			m.canI = true
			m.mu.Unlock()
		case "nguoi_xem":
			var ds []string
			if a, ok := tin["ds"].([]any); ok {
				for _, x := range a {
					ds = append(ds, fmt.Sprint(x))
				}
			}
			m.mu.Lock()
			m.nguoiXem = ds
			m.mu.Unlock()
		case "su_kien":
			m.Bao("Sự kiện: " + tin.Chuoi("noi_dung"))
		case "loi":
			m.Bao("Lỗi từ server: " + tin.Chuoi("ly_do"))
		}
	}
	m.mu.Lock()
	da := m.daKetNoi
	m.mu.Unlock()
	if da {
		m.Bao("Mất kết nối server")
		m.Ngat()
	}
}

func (m *MayPhat) DaKetNoi() bool { m.mu.Lock(); defer m.mu.Unlock(); return m.daKetNoi }
func (m *MayPhat) DangPhat() bool { m.mu.Lock(); defer m.mu.Unlock(); return m.dangPhat }

func (m *MayPhat) Ngat() {
	m.mu.Lock()
	tcp, udp := m.tcp, m.udp
	m.tcp, m.udp = nil, nil
	m.daKetNoi, m.dangPhat = false, false
	m.nguoiXem = nil
	m.mu.Unlock()
	if tcp != nil {
		dongHaiChieu(tcp)
		tcp.Close()
	}
	if udp != nil {
		udp.Close()
	}
}

// ------------------------------------------------------------ phát / dừng

func (m *MayPhat) thongTin() DoiTuong {
	return J("loai", "thong_tin", "rong", Rong, "cao", Cao, "fps", m.fps, "gop", m.gop, "tan_so_mau", TanSoMau)
}

func gioiHan(x, lo, hi int) int { return max(lo, min(hi, x)) }

// BatDauPhat – fps/gop = 0: giữ giá trị hiện tại.
func (m *MayPhat) BatDauPhat(camera Camera, mic Micro, fps, gop int) {
	m.mu.Lock()
	m.camera, m.mic = camera, mic
	if fps > 0 {
		m.fps = fps
	}
	if gop > 0 {
		m.gop = gop
	}
	m.fps, m.gop = gioiHan(m.fps, 1, 30), gioiHan(m.gop, 1, 300)
	camera.DatFps(m.fps)
	tt := m.thongTin()
	fps = m.fps
	gop = m.gop
	m.mu.Unlock()
	// thong_tin đi TCP, gói phương tiện đi UDP trên goroutine khác: gửi thong_tin trước và chờ một chút
	// để máy chủ kịp chuyển sang "đang phát" – nếu không, khung I #0 tới trước bị coi là sai nguồn, bộ
	// đệm GOP rỗng tới khung I kế tiếp.
	m.gui(tt)
	time.Sleep(20 * time.Millisecond)
	m.mu.Lock()
	m.t0 = BayGio()
	m.so, m.seq, m.nAm = 0, 0, 0
	m.soICuoi, m.truoc = -1, nil
	m.canI, m.lyDoI = false, ""
	m.lamMoiThongKe()
	m.dangPhat = true
	m.mu.Unlock()
	m.Bao(fmt.Sprintf("Bắt đầu phát %dx%d %d fps GOP %d", Rong, Cao, fps, gop))
}

func (m *MayPhat) DatThamSo(fps, gop int) {
	m.mu.Lock()
	m.fps, m.gop = gioiHan(fps, 1, 30), gioiHan(gop, 1, 300)
	if m.camera != nil {
		m.camera.DatFps(m.fps)
	}
	phat, tt := m.dangPhat, m.thongTin()
	fps, gop = m.fps, m.gop
	m.mu.Unlock()
	if phat {
		m.gui(tt)
	}
	m.Bao(fmt.Sprintf("Tham số: %d fps, GOP %d", fps, gop))
}

func (m *MayPhat) KhungKhoaNgay(lyDo string) {
	m.mu.Lock()
	m.canI, m.lyDoI = true, lyDo
	m.mu.Unlock()
}

func (m *MayPhat) DungPhat() {
	m.mu.Lock()
	if !m.dangPhat {
		m.mu.Unlock()
		return
	}
	m.dangPhat = false
	cam, mic := m.camera, m.mic
	m.camera, m.mic = nil, nil
	m.mu.Unlock()
	m.gui(J("loai", "dung_phat"))
	if cam != nil {
		cam.Dong()
	}
	if mic != nil {
		mic.Dong()
	}
	m.Bao("Dừng phát")
}

// guiUDP – gửi một datagram; MatGoi giả lập mạng làm rơi gói ở chiều lên.
func (m *MayPhat) guiUDP(goi []byte) {
	m.mu.Lock()
	if m.matGoi > 0 && m.rng.Float64() < m.matGoi {
		m.BoGiaLap++
		m.mu.Unlock()
		return
	}
	udp := m.udp
	m.mu.Unlock()
	if udp != nil {
		udp.Write(goi)
	}
}

// ------------------------------------------------------------ goroutine âm thanh 20 ms

func (m *MayPhat) vongAm() {
	hanGiu := BayGio()
	for m.song.Load() {
		bayGio := BayGio()
		m.mu.Lock()
		phat, mic, ma, udp, t0, nAm := m.dangPhat, m.mic, m.Ma, m.udp, m.t0, m.nAm
		m.mu.Unlock()
		if udp != nil && bayGio >= hanGiu {
			m.guiUDP(GoiGiu(ma)) // giữ kết nối mỗi 1 s
			hanGiu = bayGio + 1.0
		}
		if !(phat && mic != nil && udp != nil) {
			m.mu.Lock()
			m.mucMic = math.Inf(-1)
			m.mu.Unlock()
			time.Sleep(20 * time.Millisecond)
			continue
		}
		han := t0 + float64(nAm)*KhungMs/1000
		if bayGio < han {
			ngu(min(han-bayGio, 0.02))
			continue
		}
		if nAm%50 == 0 { // báo cáo đồng hồ mỗi 1 s
			m.gui(J("loai", "dong_ho", "ts", int64((bayGio-t0)*1000), "gio", int64(GioUnixMs())))
		}
		khung := mic.LayKhung()
		muc := MucDbfs(khung)
		m.mu.Lock()
		seq := m.seq
		m.mucMic = muc
		m.mu.Unlock()
		goi := DongGoiRtp(uint16(seq), uint32(nAm*MauMoiKhung), uint32(ma), PcmSangByte(khung))
		m.guiUDP(goi)
		m.mu.Lock()
		m.seq++
		m.nAm++
		m.goiAm++
		m.demByteAm += len(goi)
		m.mu.Unlock()
	}
}

// ------------------------------------------------------------ goroutine video theo fps

func (m *MayPhat) vongHinh() {
	han, coHan := 0.0, false
	for m.song.Load() {
		bayGio := BayGio()
		m.mu.Lock()
		phat, cam, ma, udp, t0, fps := m.dangPhat, m.camera, m.Ma, m.udp, m.t0, m.fps
		m.mu.Unlock()
		if !(phat && cam != nil && udp != nil) {
			coHan = false
			time.Sleep(10 * time.Millisecond)
			continue
		}
		if !coHan || han < t0 {
			han, coHan = t0, true
		}
		if bayGio < han {
			ngu(min(han-bayGio, 0.01))
			continue
		}
		han += 1 / float64(fps)
		if han < bayGio-0.2 { // tụt quá xa (máy bận): bắt nhịp lại
			han = bayGio
		}
		if loi := cam.Loi(); loi != "" { // ffmpeg dừng/lỗi: lùi về giả lập
			m.Bao("Camera lỗi, dùng giả lập: " + catRune(loi, 28))
			cam.Dong()
			cam = MoiCameraGiaLap(m.Ten, max(1, int(ma)), fps)
			m.mu.Lock()
			m.camera = cam
			m.mu.Unlock()
		}
		tsMs := int64((bayGio - t0) * 1000)
		m.mu.Lock()
		so := m.so
		m.mu.Unlock()
		anh := cam.LayKhung(so, tsMs)
		if len(anh) != Rong*Cao*3 {
			continue // camera chưa có khung
		}
		m.mu.Lock()
		gop := m.gop
		laI := m.soICuoi < 0 || so-m.soICuoi >= gop || m.canI || m.truoc == nil
		lyDo := ""
		if m.canI {
			lyDo = m.lyDoI
		}
		m.canI, m.lyDoI = false, ""
		truoc := m.truoc
		m.mu.Unlock()
		loai, duLieu := byte(LoaiP), []byte(nil)
		if laI {
			loai, duLieu = LoaiI, MaHoaI(anh)
		} else {
			duLieu = MaHoaP(anh, truoc)
		}
		manh, err := ChiaManh(loai, ma, uint32(so), Rong, Cao, uint32(tsMs), duLieu)
		if err != nil {
			continue
		}
		for _, g := range manh {
			m.guiUDP(g)
		}
		if lyDo != "" {
			m.Bao(fmt.Sprintf("%s -> gửi khung I #%d", lyDo, so))
		}
		m.mu.Lock()
		m.truoc = anh
		m.so++
		if laI {
			m.soICuoi = so
			m.soI++
			m.byteI += len(duLieu)
			m.demI++
		} else {
			m.soP++
			m.byteP += len(duLieu)
			if m.pMin < 0 || len(duLieu) < m.pMin {
				m.pMin = len(duLieu)
			}
			m.pMax = max(m.pMax, len(duLieu))
			m.demP++
		}
		for _, g := range manh {
			m.demByteHinh += len(g)
		}
		m.xemTruoc, m.tsXemTruoc = anh, tsMs
		m.mu.Unlock()
	}
}

// ------------------------------------------------------------ số liệu

// ThongKeGiay – số liệu của giây vừa qua.
type ThongKeGiay struct {
	I, P             int
	KbpsAm, KbpsHinh float64
}

// ThongKeMoiGiay – gọi mỗi 1 s (dòng lệnh hoặc giao diện): trả về và xóa bộ đếm của giây vừa qua.
func (m *MayPhat) ThongKeMoiGiay() ThongKeGiay {
	m.mu.Lock()
	defer m.mu.Unlock()
	r := ThongKeGiay{m.demI, m.demP, float64(m.demByteAm) * 8 / 1000, float64(m.demByteHinh) * 8 / 1000}
	m.kbpsAm, m.kbpsHinh = r.KbpsAm, r.KbpsHinh
	m.demI, m.demP, m.demByteAm, m.demByteHinh = 0, 0, 0, 0
	return r
}

// TrangThaiPhat – ảnh chụp cho dòng lệnh và giao diện.
type TrangThaiPhat struct {
	DangPhat, DaKetNoi bool
	Ma                 uint16
	SoI, SoP           int
	TbI, TbP           float64
	PMin, PMax         int
	GoiAm, PliNhan     int
	KbpsAm, KbpsHinh   float64
	NguoiXem           []string
	MucMic             float64
	XemTruoc           []byte
	TsXemTruoc         int64
	Fps, Gop           int
	Camera, Mic        string
	CamThat, MicThat   bool
}

func (m *MayPhat) TrangThai() TrangThaiPhat {
	m.mu.Lock()
	defer m.mu.Unlock()
	t := TrangThaiPhat{DangPhat: m.dangPhat, DaKetNoi: m.daKetNoi, Ma: m.Ma, SoI: m.soI, SoP: m.soP,
		PMin: max(0, m.pMin), PMax: max(0, m.pMax), GoiAm: m.goiAm, PliNhan: m.pliNhan, KbpsAm: m.kbpsAm,
		KbpsHinh: m.kbpsHinh, NguoiXem: append([]string(nil), m.nguoiXem...), MucMic: m.mucMic,
		XemTruoc: m.xemTruoc, TsXemTruoc: m.tsXemTruoc, Fps: m.fps, Gop: m.gop}
	if m.soI > 0 {
		t.TbI = float64(m.byteI) / float64(m.soI)
	}
	if m.soP > 0 {
		t.TbP = float64(m.byteP) / float64(m.soP)
	}
	if m.camera != nil {
		t.Camera, t.CamThat = m.camera.MoTa(), m.camera.That()
	}
	if m.mic != nil {
		t.Mic, t.MicThat = m.mic.MoTa(), m.mic.That()
	}
	return t
}

// Dong – dừng phát, dừng các goroutine nền, ngắt kết nối.
func (m *MayPhat) Dong() {
	m.DungPhat()
	m.song.Store(false)
	m.Ngat()
}

// DongTongKetPhat – các dòng tổng kết của máy phát (dùng cho dòng lệnh).
func DongTongKetPhat(t TrangThaiPhat, giay float64) []string {
	tiLe := 0.0
	if t.TbI > 0 {
		tiLe = t.TbP / t.TbI * 100
	}
	return []string{
		fmt.Sprintf("===== TỔNG KẾT sau %.1f s =====", giay),
		fmt.Sprintf("khung I %d, P %d; TB I %.0f B, P %.0f B", t.SoI, t.SoP, t.TbI, t.TbP),
		fmt.Sprintf("P nhỏ nhất %d B, lớn nhất %d B (khung chớp)", t.PMin, t.PMax),
		fmt.Sprintf("I/thô %.1f %%, P TB/I TB %.1f %%", t.TbI/(Rong*Cao*3)*100, tiLe),
		fmt.Sprintf("tiếng %d gói; PLI nhận %d", t.GoiAm, t.PliNhan),
	}
}
