package tructiep

// Lõi máy chủ trung chuyển phát trực tiếp 1→n (kiểu SFU), dạng đối tượng.
//
// Dùng chung cho bản dòng lệnh (server.go ở thư mục cha) và ứng dụng giao diện (gui/server_gui.go);
// không chạm giao diện, báo sự kiện bằng hàm Bao(text). BatDau/Dung được nhiều lần.
//
// Kiến trúc SFU (Selective Forwarding Unit – RFC 7667 mục 3.7): máy chủ KHÔNG giải nén, KHÔNG trộn;
// mỗi datagram của máy phát được NHÂN BẢN nguyên byte tới từng người xem (fan-out):
//
//	băng thông ra ≈ n × băng thông vào; CPU chỉ tốn cho WriteToUDP() – khác MCU ở MR6.3.
//
// Máy chủ chỉ đọc header (byte đầu, loại khung, số khung) để: đếm gói/byte, lọc theo đăng ký (âm
// thanh/video) và giữ BỘ ĐỆM GOP: khung I mới nhất + mọi khung P sau nó (như GOP cache của SRS,
// nginx-rtmp). Người vào muộn nhận ngay cả nhóm -> giải được hình tức thì, không phải chờ I kế tiếp.
//
// Goroutine: một nhận kết nối TCP, mỗi client một đọc TCP, MỘT nhận UDP chuyển tiếp ngay (mọi gói
// phương tiện đi ra từ chỉ goroutine này nên thứ tự gói được giữ). Trạng thái chung (danh sách client,
// bộ đệm GOP, thống kê) được bảo vệ bằng một khóa mu.

import (
	"bufio"
	"fmt"
	"net"
	"sort"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
)

// khach – một kết nối: máy phát (vai trò "phat") hoặc người xem ("xem").
type khach struct {
	ma              uint16
	ten, vaiTro, ip string
	conn            net.Conn
	khoaGui         sync.Mutex
	udp             *net.UDPAddr // địa chỉ UDP đã đăng ký
	amThanh, video  bool         // đăng ký nhận (người xem)
	lucChao         float64
	lucMoc          float64 // mốc đo khung I đầu: lúc vào, hoặc lúc bắt đầu phát nếu muộn hơn
	canBoDem        bool    // chờ goroutine UDP gửi bộ đệm GOP cho riêng người này
	goiRa, byteRa   int
	demByte         int // cửa sổ 1 giây
	kbps            float64
	khungDauMs      *float64 // từ lúc chào tới mảnh khung I đầu tiên gửi cho người này
	nguonDau        string   // "bộ đệm" hoặc "trực tiếp"
	pli             int
}

func (k *khach) gui(v any) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	GuiJSON(k.conn, v)
}

func (k *khach) guiTho(dong []byte) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	k.conn.Write(dong)
}

func (k *khach) nhan() string {
	switch {
	case k.amThanh && k.video:
		return "A+V"
	case k.amThanh:
		return "A"
	case k.video:
		return "V"
	}
	return "-"
}

// TomTatXem – một dòng của bảng người xem.
type TomTatXem struct {
	Ma             uint16
	Ten, Udp, Nhan string
	GoiRa, ByteRa  int
	Kbps           float64
	KhungDauMs     *float64
	NguonDau       string
	Pli            int
	Giay           float64
}

func (k *khach) tomTat() TomTatXem {
	udp := "–"
	if k.udp != nil {
		udp = k.udp.String()
	}
	var dau *float64
	if k.khungDauMs != nil {
		dau = conTro(*k.khungDauMs)
	}
	return TomTatXem{k.ma, k.ten, udp, k.nhan(), k.goiRa, k.byteRa, k.kbps, dau, k.nguonDau, k.pli,
		BayGio() - k.lucChao}
}

// AnhXemTruoc – khung server tự giải mã để giao diện hiện xem trước.
type AnhXemTruoc struct {
	W, H int
	Rgb  []byte
	Ts   uint32
	So   int64
}

// MayChuPhat – máy chủ trung chuyển.
type MayChuPhat struct {
	Bao      func(string)
	mu       sync.Mutex
	chay     atomic.Bool
	BoDemBat bool
	XemTruoc bool // true: tự giải mã một bản để giao diện hiện ảnh xem trước
	CongUDP  int
	DiaChi   string
	maMoi    uint16
	ln       net.Listener
	udp      *net.UDPConn
	wg       sync.WaitGroup

	ds                             map[uint16]*khach
	mayPhat                        *khach
	dangPhat                       bool
	thongTin, dongHo               []byte // nguyên văn dòng đã lưu cho người vào sau
	thongTinTin                    Tin
	boDem                          [][]byte
	boDemByte                      int
	boDemSoI                       int64 // -1: chưa có I
	byteVao, goiVao, byteRa, goiRa int
	demVaoAm, demVaoHinh, demRa    int
	kbpsVaoAm, kbpsVaoHinh, kbpsRa float64
	pliChuyen, pliBo               int
	lucPli                         float64
	saiNguon                       int
	daRoi                          []TomTatXem
	anhXemTruoc                    *AnhXemTruoc
	ghep                           *GhepKhung
	giaiMa                         *GiaiMaVideo
}

func MoiMayChu(bao func(string)) *MayChuPhat {
	if bao == nil {
		bao = func(string) {}
	}
	m := &MayChuPhat{Bao: bao, BoDemBat: true}
	m.lamMoi()
	return m
}

func (m *MayChuPhat) lamMoi() {
	m.ds = map[uint16]*khach{}
	m.mayPhat = nil
	m.dangPhat = false
	m.thongTin, m.dongHo, m.thongTinTin = nil, nil, nil
	m.boDem, m.boDemByte, m.boDemSoI = nil, 0, -1
	m.byteVao, m.goiVao, m.byteRa, m.goiRa = 0, 0, 0, 0
	m.demVaoAm, m.demVaoHinh, m.demRa = 0, 0, 0
	m.kbpsVaoAm, m.kbpsVaoHinh, m.kbpsRa = 0, 0, 0
	m.pliChuyen, m.pliBo = 0, 0
	m.lucPli = -1e9
	m.saiNguon = 0
	m.daRoi = nil
	m.anhXemTruoc = nil
	m.ghep, m.giaiMa = MoiGhep(), MoiGiaiMa()
}

// Chay – server đang chạy không.
func (m *MayChuPhat) Chay() bool { return m.chay.Load() }

// ------------------------------------------------------------ khởi động / dừng

func (m *MayChuPhat) BatDau(host string, congTCP, congUDP int) error {
	if congUDP == 0 {
		congUDP = congTCP + 1
	}
	ln, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(congTCP)))
	if err != nil {
		return err
	}
	ua, err := net.ResolveUDPAddr("udp", net.JoinHostPort(host, strconv.Itoa(congUDP)))
	if err != nil {
		ln.Close()
		return err
	}
	udp, err := net.ListenUDP("udp", ua)
	if err != nil {
		ln.Close()
		return err
	}
	udp.SetReadBuffer(1 << 20)
	udp.SetWriteBuffer(1 << 20)
	m.mu.Lock()
	m.lamMoi()
	m.mu.Unlock()
	m.CongUDP, m.maMoi, m.ln, m.udp = congUDP, 0, ln, udp
	m.DiaChi = fmt.Sprintf("%s:%d", host, congTCP)
	m.chay.Store(true)
	m.wg.Add(2)
	go m.nhanKetNoi(ln)
	go m.nhanUDP(udp)
	m.Bao(fmt.Sprintf("TCP %s:%d, UDP %d; bộ đệm GOP: %s", host, congTCP, congUDP, batTat(m.BoDemBat)))
	return nil
}

func batTat(b bool) string {
	if b {
		return "bật"
	}
	return "tắt"
}

func (m *MayChuPhat) Dung() {
	if !m.chay.Swap(false) {
		return
	}
	m.mu.Lock()
	var ds []*khach
	for _, k := range m.ds {
		ds = append(ds, k)
	}
	m.mu.Unlock()
	for _, k := range ds {
		dongHaiChieu(k.conn)
	}
	m.ln.Close()
	m.udp.Close()
	xong := make(chan struct{})
	go func() { m.wg.Wait(); close(xong) }()
	select {
	case <-xong:
	case <-time.After(2 * time.Second):
	}
	m.Bao("Server đã dừng")
}

// dongHaiChieu – như shutdown(SHUT_RDWR): gửi FIN (sau dữ liệu đã ghi) và đánh thức goroutine đang đọc.
func dongHaiChieu(c net.Conn) {
	if t, ok := c.(*net.TCPConn); ok {
		t.CloseWrite()
		t.CloseRead()
		return
	}
	c.Close()
}

// ------------------------------------------------------------ TCP điều khiển

// suKien – gửi sự kiện cho mọi client (gọi khi đang giữ khóa).
func (m *MayChuPhat) suKien(text string) {
	m.Bao("=> " + text)
	for _, k := range m.dsTheoMa(false) {
		k.gui(J("loai", "su_kien", "noi_dung", text))
	}
}

// dsTheoMa – các client theo mã tăng dần; chiXem = chỉ người xem.
func (m *MayChuPhat) dsTheoMa(chiXem bool) []*khach {
	var ds []*khach
	for _, k := range m.ds {
		if !chiXem || k.vaiTro == "xem" {
			ds = append(ds, k)
		}
	}
	sort.Slice(ds, func(i, j int) bool { return ds[i].ma < ds[j].ma })
	return ds
}

// baoNguoiXem – gửi máy phát danh sách người xem mỗi khi danh sách đổi (gọi khi giữ khóa).
func (m *MayChuPhat) baoNguoiXem() {
	if m.mayPhat != nil {
		ten := []string{}
		for _, k := range m.dsTheoMa(true) {
			ten = append(ten, k.ten)
		}
		m.mayPhat.gui(J("loai", "nguoi_xem", "so", len(ten), "ds", ten))
	}
}

func (m *MayChuPhat) nhanKetNoi(ln net.Listener) {
	defer m.wg.Done()
	for m.chay.Load() {
		conn, err := ln.Accept()
		if err != nil {
			if !m.chay.Load() {
				break
			}
			time.Sleep(10 * time.Millisecond)
			continue
		}
		if t, ok := conn.(*net.TCPConn); ok {
			t.SetNoDelay(true)
		}
		go m.phucVu(conn)
	}
}

func (m *MayChuPhat) phucVu(conn net.Conn) {
	r := bufio.NewReaderSize(conn, 4096)
	var k *khach
	defer func() {
		if k != nil {
			m.mu.Lock()
			m.roi(k, "rời")
			m.mu.Unlock()
		}
		conn.Close()
	}()
	tin, err := DocDong(r)
	if err != nil || tin.Chuoi("loai") != "chao" || (tin.Chuoi("vai_tro") != "phat" && tin.Chuoi("vai_tro") != "xem") {
		return
	}
	ten, _ := tin["ten"].(string)
	if !TenHopLe(ten) {
		GuiJSON(conn, J("loai", "loi", "ly_do", "tên không hợp lệ"))
		return
	}
	ip, cong, _ := net.SplitHostPort(conn.RemoteAddr().String())
	m.mu.Lock()
	if tin.Chuoi("vai_tro") == "phat" && m.mayPhat != nil {
		GuiJSON(conn, J("loai", "loi", "ly_do", "đã có máy phát"))
		m.Bao("Từ chối máy phát thứ hai " + ten)
		m.mu.Unlock()
		return
	}
	m.maMoi++
	k = &khach{ma: m.maMoi, ten: ten, vaiTro: tin.Chuoi("vai_tro"), conn: conn, ip: ip,
		amThanh: true, video: true, lucChao: BayGio()}
	k.lucMoc = k.lucChao
	m.ds[k.ma] = k
	k.gui(J("loai", "chao", "ma", k.ma, "cong_udp", m.CongUDP, "phat", m.dangPhat))
	if k.vaiTro == "phat" {
		m.mayPhat = k
		m.Bao(fmt.Sprintf("#%d %s (phát) vào từ %s:%s", k.ma, k.ten, ip, cong))
		m.suKien(k.ten + " kết nối (máy phát)")
		m.baoNguoiXem()
	} else {
		if m.thongTin != nil {
			k.guiTho(m.thongTin) // bản đã lưu cho người vào sau
		}
		if m.dongHo != nil {
			k.guiTho(m.dongHo)
		}
		m.Bao(fmt.Sprintf("#%d %s (xem) vào từ %s:%s", k.ma, k.ten, ip, cong))
		m.suKien(k.ten + " vào xem")
		m.baoNguoiXem()
	}
	m.mu.Unlock()
	for m.chay.Load() {
		tin, dong, err := DocDongTho(r)
		if err != nil {
			break
		}
		m.mu.Lock()
		m.xuLy(k, tin, dong)
		m.mu.Unlock()
	}
}

func (m *MayChuPhat) roi(k *khach, lyDo string) {
	if _, co := m.ds[k.ma]; !co {
		return
	}
	delete(m.ds, k.ma)
	if k == m.mayPhat {
		if m.dangPhat {
			m.ketThucPhat()
		}
		m.mayPhat = nil
		m.suKien(k.ten + " (máy phát) thoát")
	} else {
		m.daRoi = append(m.daRoi, k.tomTat())
		m.suKien(k.ten + " " + lyDo)
		m.baoNguoiXem()
	}
}

func (m *MayChuPhat) ketThucPhat() {
	m.dangPhat = false
	m.thongTin, m.dongHo, m.thongTinTin = nil, nil, nil
	m.boDem, m.boDemByte, m.boDemSoI = nil, 0, -1
	m.ghep, m.giaiMa = MoiGhep(), MoiGiaiMa()
	for _, x := range m.dsTheoMa(false) {
		x.gui(J("loai", "ket_thuc"))
	}
}

func (m *MayChuPhat) xuLy(k *khach, tin Tin, dong []byte) {
	loai := tin.Chuoi("loai")
	if k.vaiTro == "phat" {
		switch {
		case loai == "thong_tin":
			m.thongTin, m.thongTinTin = dong, tin
			for _, x := range m.dsTheoMa(true) {
				x.guiTho(dong) // chuyển tiếp nguyên văn
			}
			m.Bao(fmt.Sprintf("%s phát %sx%s %s fps GOP %s, %s Hz", k.ten, SoChu(tin["rong"]), SoChu(tin["cao"]),
				SoChu(tin["fps"]), SoChu(tin["gop"]), SoChu(tin["tan_so_mau"])))
			if !m.dangPhat {
				m.dangPhat = true
				bayGio := BayGio()
				for _, x := range m.dsTheoMa(true) { // người vào trước lúc phát: đo từ lúc phát
					if x.khungDauMs == nil {
						x.lucMoc = bayGio
					}
				}
				m.suKien(k.ten + " bắt đầu phát")
			}
		case loai == "dong_ho":
			m.dongHo = dong
			for _, x := range m.dsTheoMa(true) {
				x.guiTho(dong)
			}
		case loai == "dung_phat" && m.dangPhat:
			m.ketThucPhat()
			m.suKien(k.ten + " dừng phát")
		}
		return
	}
	switch loai {
	case "dang_ky":
		coVideo := k.video
		k.amThanh = tin.Bool("am_thanh", k.amThanh)
		k.video = tin.Bool("video", k.video)
		if k.video && !coVideo && m.BoDemBat && k.udp != nil {
			k.canBoDem = true // bật lại video: gửi bộ đệm GOP
		}
		m.Bao(fmt.Sprintf("%s đăng ký nhận: %s", k.ten, k.nhan()))
	case "xin_khung_khoa":
		k.pli++
		bayGio := BayGio()
		if m.mayPhat != nil && m.dangPhat && bayGio-m.lucPli >= PliServerS {
			m.lucPli = bayGio
			m.pliChuyen++
			m.mayPhat.gui(J("loai", "xin_khung_khoa", "tu", k.ten))
			m.Bao(fmt.Sprintf("PLI từ %s -> %s", k.ten, m.mayPhat.ten))
		} else {
			m.pliBo++
			m.Bao(fmt.Sprintf("PLI từ %s: bỏ (lọc 500 ms)", k.ten))
		}
	}
}

// ------------------------------------------------------------ UDP: nhận và nhân bản

type goiDich struct {
	goi  []byte
	dich *net.UDPAddr
}

func (m *MayChuPhat) nhanUDP(sock *net.UDPConn) {
	defer m.wg.Done()
	moc := BayGio()
	for m.chay.Load() {
		buf := make([]byte, 2048) // mỗi gói một mảng riêng: bộ đệm GOP giữ lại gói
		sock.SetReadDeadline(time.Now().Add(500 * time.Millisecond))
		n, addr, err := sock.ReadFromUDP(buf)
		var goi []byte
		if err == nil {
			goi = buf[:n:n]
		} else if ne, ok := err.(net.Error); !ok || !ne.Timeout() {
			if !m.chay.Load() {
				break
			}
		}
		bayGio := BayGio()
		m.guiBoDemCho(sock) // bộ đệm GOP đi TRƯỚC mọi gói trực tiếp
		if len(goi) > 0 {
			m.mu.Lock()
			ra := m.xuLyGoi(goi, addr, bayGio)
			m.mu.Unlock()
			for _, g := range ra {
				sock.WriteToUDP(g.goi, g.dich)
			}
		}
		if bayGio-moc >= 1.0 {
			moc += 1.0
			if bayGio-moc > 1.0 {
				moc = bayGio
			}
			m.mu.Lock()
			m.kbpsVaoAm, m.kbpsVaoHinh = float64(m.demVaoAm)*8/1000, float64(m.demVaoHinh)*8/1000
			m.kbpsRa = float64(m.demRa) * 8 / 1000
			m.demVaoAm, m.demVaoHinh, m.demRa = 0, 0, 0
			for _, k := range m.ds {
				k.kbps, k.demByte = float64(k.demByte)*8/1000, 0
			}
			m.mu.Unlock()
		}
	}
}

// guiBoDemCho – gửi toàn bộ bộ đệm GOP cho người vừa có địa chỉ UDP / vừa bật lại video.
// Nghỉ 1 ms mỗi 32 gói để không làm tràn bộ đệm nhận của người xem.
func (m *MayChuPhat) guiBoDemCho(sock *net.UDPConn) {
	m.mu.Lock()
	var ds []*khach
	for _, k := range m.dsTheoMa(true) {
		if k.canBoDem && k.udp != nil {
			ds = append(ds, k)
		}
	}
	if len(ds) == 0 {
		m.mu.Unlock()
		return
	}
	var goi [][]byte
	if m.BoDemBat {
		goi = append(goi, m.boDem...)
	}
	soI := m.boDemSoI
	for _, k := range ds {
		k.canBoDem = false
	}
	m.mu.Unlock()
	if len(goi) == 0 {
		return
	}
	for _, k := range ds {
		for n, g := range goi {
			sock.WriteToUDP(g, k.udp)
			if n%32 == 31 {
				time.Sleep(time.Millisecond)
			}
		}
		m.mu.Lock()
		b := 0
		khungP := map[uint32]bool{}
		for _, g := range goi {
			b += len(g)
			if g[1] == LoaiP {
				khungP[uint32(g[4])<<24|uint32(g[5])<<16|uint32(g[6])<<8|uint32(g[7])] = true
			}
		}
		k.goiRa += len(goi)
		k.byteRa += b
		k.demByte += b
		m.goiRa += len(goi)
		m.byteRa += b
		m.demRa += b
		m.Bao(fmt.Sprintf("%s: gửi bộ đệm GOP %d gói (I #%d + %d P)", k.ten, len(goi), soI, len(khungP)))
		if k.khungDauMs == nil {
			k.khungDauMs = conTro((BayGio() - k.lucMoc) * 1000)
			k.nguonDau = "bộ đệm"
			m.Bao(fmt.Sprintf("%s: khung I đầu sau %.0f ms (bộ đệm)", k.ten, *k.khungDauMs))
		}
		m.mu.Unlock()
	}
}

// xuLyGoi – xử lý một datagram (đang giữ khóa). Trả về danh sách (gói, đích) cần gửi.
func (m *MayChuPhat) xuLyGoi(goi []byte, addr *net.UDPAddr, bayGio float64) []goiDich {
	var h Header
	var payload []byte
	video := false
	switch goi[0] {
	case PhienBan:
		var ok bool
		h, payload, ok = TachHeader(goi)
		if !ok {
			return nil
		}
		if h.Loai == LoaiGiu {
			k := m.ds[h.Ma]
			if k != nil && addr.IP.String() == k.ip && (k.udp == nil || k.udp.String() != addr.String()) {
				lanDau := k.udp == nil
				k.udp = addr
				m.Bao(fmt.Sprintf("#%d %s đăng ký UDP %s", k.ma, k.ten, addr.String()))
				if lanDau && k.vaiTro == "xem" && k.video && m.BoDemBat {
					k.canBoDem = true
				}
			}
			return nil
		}
		video = true
	case DauRtp:
	default:
		return nil
	}
	if m.mayPhat == nil || m.mayPhat.udp == nil || addr.String() != m.mayPhat.udp.String() || !m.dangPhat {
		m.saiNguon++ // chỉ nhận phương tiện từ máy phát
		return nil
	}
	m.goiVao++
	m.byteVao += len(goi)
	if video {
		m.demVaoHinh += len(goi)
		m.capNhatBoDem(h.Loai, int64(h.SoKhung), goi)
		if m.XemTruoc {
			m.giaiMaXemTruoc(h, payload, bayGio)
		}
	} else {
		m.demVaoAm += len(goi)
	}
	var ra []goiDich
	for _, k := range m.dsTheoMa(true) {
		nhan := k.amThanh
		if video {
			nhan = k.video
		}
		if k.udp == nil || k.canBoDem || !nhan {
			continue
		}
		ra = append(ra, goiDich{goi, k.udp})
		k.goiRa++
		k.byteRa += len(goi)
		k.demByte += len(goi)
		if video && h.Loai == LoaiI && k.khungDauMs == nil {
			k.khungDauMs = conTro((bayGio - k.lucMoc) * 1000)
			k.nguonDau = "trực tiếp"
			m.Bao(fmt.Sprintf("%s: khung I đầu sau %.0f ms (trực tiếp)", k.ten, *k.khungDauMs))
		}
	}
	m.goiRa += len(ra)
	m.byteRa += len(goi) * len(ra)
	m.demRa += len(goi) * len(ra)
	return ra
}

// capNhatBoDem – bộ đệm GOP: mảnh đầu tiên của khung I mới -> xóa; lưu mảnh của I và các P sau nó.
func (m *MayChuPhat) capNhatBoDem(loai byte, so int64, goi []byte) {
	if !m.BoDemBat {
		return
	}
	if loai == LoaiI && so != m.boDemSoI {
		m.boDem, m.boDemByte, m.boDemSoI = nil, 0, so
	} else if loai != LoaiI && m.boDemSoI < 0 {
		return // chưa có I (hoặc vừa tràn): chờ I kế tiếp
	}
	m.boDem = append(m.boDem, goi)
	m.boDemByte += len(goi)
	if m.boDemByte > BoDemGopToiDa {
		m.boDem, m.boDemByte, m.boDemSoI = nil, 0, -1
		m.Bao("Bộ đệm GOP quá 4 MB: xóa, chờ khung I kế tiếp")
	}
}

// giaiMaXemTruoc – server giải mã MỘT bản chỉ để hiện ảnh xem trước (không ảnh hưởng việc chuyển tiếp).
func (m *MayChuPhat) giaiMaXemTruoc(h Header, payload []byte, bayGio float64) {
	k := m.ghep.Them(h, payload, bayGio)
	if k == nil {
		return
	}
	m.giaiMa.KiemTra(bayGio)
	for _, x := range m.giaiMa.Them(k, bayGio) {
		m.anhXemTruoc = &AnhXemTruoc{x.W, x.H, x.DuLieu, x.Ts, x.So}
	}
}

// ------------------------------------------------------------ lệnh của bảng điều khiển

func (m *MayChuPhat) DatBoDem(bat bool) {
	m.mu.Lock()
	m.BoDemBat = bat
	if !bat {
		m.boDem, m.boDemByte, m.boDemSoI = nil, 0, -1
	}
	m.mu.Unlock()
	m.Bao("Bộ đệm GOP: " + batTat(bat))
}

// Ngat – đuổi người xem theo tên (hoặc mã > 0). Trả về số người bị ngắt.
func (m *MayChuPhat) Ngat(ten string, ma uint16) int {
	m.mu.Lock()
	var ds []*khach
	for _, k := range m.dsTheoMa(true) {
		if (ma > 0 && k.ma == ma) || (ten != "" && k.ten == ten) {
			ds = append(ds, k)
		}
	}
	for _, k := range ds {
		k.gui(J("loai", "bi_ngat", "ly_do", "server ngắt"))
		m.roi(k, "bị server ngắt")
	}
	m.mu.Unlock()
	for _, k := range ds {
		dongHaiChieu(k.conn)
	}
	return len(ds)
}

// TrangThaiMayChu – ảnh chụp trạng thái cho dòng lệnh và giao diện.
type TrangThaiMayChu struct {
	DangPhat                        bool
	TenPhat                         string
	ThongTin                        Tin
	BoDemBat                        bool
	BoDemGoi, BoDemByte             int
	BoDemSoI                        int64
	KbpsVao, KbpsVaoAm, KbpsVaoHinh float64
	KbpsRa, HeSo                    float64
	ByteVao, ByteRa, GoiVao, GoiRa  int
	PliChuyen, PliBo                int
	NguoiXem, DaRoi                 []TomTatXem
	Anh                             *AnhXemTruoc
}

func (m *MayChuPhat) TrangThai() TrangThaiMayChu {
	m.mu.Lock()
	defer m.mu.Unlock()
	vao := m.kbpsVaoAm + m.kbpsVaoHinh
	t := TrangThaiMayChu{DangPhat: m.dangPhat, ThongTin: m.thongTinTin, BoDemBat: m.BoDemBat,
		BoDemGoi: len(m.boDem), BoDemByte: m.boDemByte, BoDemSoI: m.boDemSoI,
		KbpsVao: vao, KbpsVaoAm: m.kbpsVaoAm, KbpsVaoHinh: m.kbpsVaoHinh, KbpsRa: m.kbpsRa,
		ByteVao: m.byteVao, ByteRa: m.byteRa, GoiVao: m.goiVao, GoiRa: m.goiRa,
		PliChuyen: m.pliChuyen, PliBo: m.pliBo, DaRoi: append([]TomTatXem(nil), m.daRoi...), Anh: m.anhXemTruoc}
	if vao > 0 {
		t.HeSo = m.kbpsRa / vao
	}
	if m.mayPhat != nil {
		t.TenPhat = m.mayPhat.ten
	}
	for _, k := range m.dsTheoMa(true) {
		t.NguoiXem = append(t.NguoiXem, k.tomTat())
	}
	return t
}

// DongBang – một dòng của bảng người xem (≤ 55 ký tự) – dùng chung cho trang_thai và tổng kết.
func DongBang(x TomTatXem) string {
	dau := "–"
	if x.KhungDauMs != nil {
		dau = fmt.Sprintf("%4.0f ms %s", *x.KhungDauMs, x.NguonDau)
	}
	return fmt.Sprintf("%-6s %-4s %6d %6.0f %6.0f %s", catRune(x.Ten, 6), x.Nhan, x.GoiRa,
		float64(x.ByteRa)/1024, x.Kbps, dau)
}

var TieuDeBang = fmt.Sprintf("%-6s %-4s %6s %6s %6s khung I đầu", "tên", "nhận", "gói ra", "KB ra", "kbit/s")

// BangTrangThai – các dòng in cho lệnh trang_thai.
func BangTrangThai(t TrangThaiMayChu) []string {
	phat := "chưa phát"
	if t.DangPhat {
		phat = t.TenPhat + " đang phát"
	}
	ds := []string{
		fmt.Sprintf("Trạng thái: %s, %d người xem", phat, len(t.NguoiXem)),
		fmt.Sprintf("bộ đệm GOP %s: %d gói, %.0f KB", batTat(t.BoDemBat), t.BoDemGoi, float64(t.BoDemByte)/1024),
		fmt.Sprintf("vào %.0f kbit/s (tiếng %.0f, hình %.0f)", t.KbpsVao, t.KbpsVaoAm, t.KbpsVaoHinh),
		fmt.Sprintf("ra %.0f kbit/s, hệ số nhân bản %.2f", t.KbpsRa, t.HeSo),
		fmt.Sprintf("PLI chuyển %d, bỏ %d", t.PliChuyen, t.PliBo),
		TieuDeBang,
	}
	for _, x := range t.NguoiXem {
		ds = append(ds, DongBang(x))
	}
	return ds
}

// BangTongKet – các dòng in khi server kết thúc: mọi người xem đã vào (cả người đã rời).
func BangTongKet(t TrangThaiMayChu) []string {
	ds := append(append([]TomTatXem(nil), t.DaRoi...), t.NguoiXem...)
	heSo := 0.0
	if t.ByteVao > 0 {
		heSo = float64(t.ByteRa) / float64(t.ByteVao)
	}
	ra := []string{
		fmt.Sprintf("===== TỔNG KẾT: %d người xem =====", len(ds)),
		fmt.Sprintf("vào %d gói %.0f KB, ra %d gói %.0f KB", t.GoiVao, float64(t.ByteVao)/1024, t.GoiRa,
			float64(t.ByteRa)/1024),
		fmt.Sprintf("hệ số nhân bản (byte ra/vào) %.2f", heSo),
		fmt.Sprintf("PLI chuyển %d, bỏ %d", t.PliChuyen, t.PliBo),
		TieuDeBang,
	}
	for _, x := range ds {
		x.Kbps = float64(x.ByteRa) * 8 / 1000 / max(x.Giay, 0.001)
		ra = append(ra, DongBang(x))
	}
	return append(ra, "(kbit/s = trung bình suốt thời gian xem)")
}
