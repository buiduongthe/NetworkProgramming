package tructiep

// Lõi người xem phát trực tiếp, dạng đối tượng, không chạm giao diện.
//
// Dùng chung cho bản dòng lệnh (xem.go ở thư mục cha) và gui/xem_gui.go. Bốn goroutine, chung khóa mu:
//  1. đọc TCP: thong_tin, dong_ho (báo cáo đồng hồ), su_kien, ket_thuc, bi_ngat;
//  2. nhận UDP: RTP -> bộ đệm jitter; mảnh video -> GhepKhung -> GiaiMaVideo -> hàng hiển thị;
//  3. phát 20 ms = ĐỒNG HỒ CHỦ: lấy một khung âm thanh, đưa ra loa, cập nhật
//     dongHoAm = ts/16 (ms phương tiện) và giờ thực lúc phát; ghi lịch sử ~3 s để đo lệch;
//  4. hiển thị 5 ms: kiểm tra mất tham chiếu (xin khung khóa), chọn khung để hiện.
//
// Đồng bộ tiếng-hình (lip-sync) – âm thanh làm đồng hồ chủ (như trình phát thật: tai nhạy với tiếng
// bị ngắt hơn mắt nhạy với một khung hình hiện sớm/muộn):
//   - BẬT: chỉ hiện khung mới nhất có nhãn ≤ đồng hồ phát hiện tại
//     dongHoAm + (bây giờ − lúc phát khung âm thanh đó); khung quá cũ bị bỏ.
//   - TẮT (hoặc chỉ nhận video): hiện ngay khi giải mã xong -> hình đi TRƯỚC tiếng đúng bằng thời
//     gian âm thanh nằm trong bộ đệm jitter (độ sâu × 20 ms).
//
// Lệch tiếng-hình của một khung = giờ thực lúc hiện − giờ thực lúc âm thanh cùng nhãn được phát
// (nội suy trên lưới 20 ms của lịch sử phát); dương = hình chậm hơn tiếng, âm = hình đi trước.
// Độ trễ đầu-cuối (âm thanh) = giờ Unix lúc phát − (gio + (ts_ms − ts)) theo báo cáo đồng hồ mới nhất
// – chỉ đúng khi máy phát và người xem cùng đồng hồ (cùng máy hoặc đồng bộ NTP).

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

// GiaiDoan – một giai đoạn bật/tắt đồng bộ và tổng lệch tiếng-hình trong giai đoạn đó.
type GiaiDoan struct {
	DongBo bool
	Tong   float64
	N      int
}

type moc struct{ ts, gio float64 } // (nhãn ms, giờ đơn điệu) của một khung tiếng đã phát

// KhungHien – khung đang hiện: ảnh, nhãn hình, số khung, đồng hồ tiếng lúc hiện (nil nếu không có).
type KhungHien struct {
	W, H   int
	Rgb    []byte
	Ts     uint32
	So     int64
	DongHo *float64
}

type NguoiXem struct {
	Ten                    string
	Bao                    func(string)
	mu, khoaTCP            sync.Mutex
	tcp                    net.Conn
	udp                    *net.UDPConn
	daKetNoi               bool
	Ma                     uint16
	doSau                  int
	dongBo, amThanh, video bool
	matGoi                 float64
	rng                    *NgauNhien
	boGiaLap               int
	loa                    LoaRa
	thongTin               Tin
	mucAm                  float64
	giaiDoan               []GiaiDoan

	lucChao                                    float64
	lucMoc                                     float64 // mốc đo khung đầu: lúc vào, hoặc lúc máy phát
	choPhat                                    bool    // bắt đầu phát nếu khi vào chưa có ai phát
	goiAm, byteNhan, khungI, khungP, khungHien int
	dungHinh, pliGui, boCu                     int
	matCu, hongCu, boPCu                       int
	tongTre                                    float64
	nTre                                       int
	treHien, lechHien                          *float64
	khungDauMs                                 *float64
	nguonDau                                   string
	iDauCu, iDauDen                            *float64 // khung I đầu: "cũ" bao nhiêu ms, tới sau bao nhiêu ms
	coIDau                                     bool
	choHinhLai                                 *float64
	lucPli                                     float64
	dangDung                                   bool
	wAm, wHien, wByte                          int
	wTre, wLech                                []float64

	boDem                          *BoDemJitter
	ghep                           *GhepKhung
	giaiMa                         *GiaiMaVideo
	hang                           []KhungNen
	lichSu                         []moc
	dongHoAm, lucPhatAm, lucNhanAm *float64
	tsAmMoi, tsHinhMoi             *float64
	dongHoBao                      *moc // (ts, gio) báo cáo đồng hồ mới nhất
	hien                           *KhungHien
	soHien                         int
	song                           atomic.Bool
}

// MoiNguoiXem – loa = nil: không phát ra loa (đồng hồ phát vẫn chạy đúng nhịp 20 ms).
func MoiNguoiXem(ten string, bao func(string), doSau int, dongBo, amThanh, video bool, matGoi float64,
	loa LoaRa) *NguoiXem {
	if bao == nil {
		bao = func(string) {}
	}
	n := &NguoiXem{Ten: ten, Bao: bao, doSau: doSau, dongBo: dongBo, amThanh: amThanh, video: video,
		matGoi: matGoi, rng: MoiNgauNhien(0), loa: loa, mucAm: math.Inf(-1),
		giaiDoan: []GiaiDoan{{DongBo: dongBo}}}
	n.lamMoiThongKe()
	n.lamMoiPhien()
	n.song.Store(true)
	go n.vongAm()
	go n.vongHien()
	return n
}

func (n *NguoiXem) lamMoiThongKe() {
	n.lucChao = BayGio()
	n.lucMoc, n.choPhat = n.lucChao, false
	n.goiAm, n.byteNhan, n.khungI, n.khungP, n.khungHien = 0, 0, 0, 0, 0
	n.dungHinh, n.pliGui, n.boCu = 0, 0, 0
	n.matCu, n.hongCu, n.boPCu = 0, 0, 0
	n.tongTre, n.nTre = 0, 0
	n.treHien, n.lechHien = nil, nil
	n.khungDauMs, n.nguonDau, n.coIDau, n.iDauCu, n.iDauDen = nil, "", false, nil, nil
	n.choHinhLai = nil
	n.lucPli = -1e9
	n.dangDung = false
	n.wAm, n.wHien, n.wByte = 0, 0, 0
	n.wTre, n.wLech = nil, nil
}

// lamMoiPhien – trạng thái của một phiên phát (làm mới khi máy phát dừng phát).
func (n *NguoiXem) lamMoiPhien() {
	if n.boDem != nil {
		n.matCu += n.boDem.Mat()
		n.hongCu += n.ghep.KhungHong
		n.boPCu += n.giaiMa.BoP
	}
	n.boDem = MoiBoDemJitter(n.doSau)
	n.ghep, n.giaiMa = MoiGhep(), MoiGiaiMa()
	n.hang = nil
	n.lichSu = nil
	n.dongHoAm, n.lucPhatAm, n.lucNhanAm = nil, nil, nil
	n.tsAmMoi, n.tsHinhMoi = nil, nil
	n.dongHoBao = nil
	n.hien = nil
	n.soHien = 0
}

// ------------------------------------------------------------ kết nối

func (n *NguoiXem) KetNoi(host string, cong int) (uint16, error) {
	tcp, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), 5*time.Second)
	if err != nil {
		return 0, err
	}
	if t, ok := tcp.(*net.TCPConn); ok {
		t.SetNoDelay(true)
	}
	r := bufio.NewReaderSize(tcp, 4096)
	tcp.SetDeadline(time.Now().Add(5 * time.Second))
	if err := GuiJSON(tcp, J("loai", "chao", "vai_tro", "xem", "ten", n.Ten)); err != nil {
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
	udp, err := net.DialUDP("udp", nil, ua) // "connect": chỉ nhận datagram từ server
	if err != nil {
		tcp.Close()
		return 0, err
	}
	udp.SetReadBuffer(1 << 20)
	n.mu.Lock()
	n.tcp, n.udp, n.Ma = tcp, udp, uint16(tl.So("ma", 0))
	n.rng = MoiNgauNhien(uint32(n.Ma))
	n.lamMoiPhien()
	n.lamMoiThongKe()
	n.choPhat = !tl.Bool("phat", false)
	n.daKetNoi = true
	am, vd := n.amThanh, n.video
	n.mu.Unlock()
	if !(am && vd) {
		n.gui(J("loai", "dang_ky", "am_thanh", am, "video", vd))
	}
	udp.Write(GoiGiu(n.Ma)) // đăng ký địa chỉ UDP
	if tl.Bool("phat", false) {
		n.Bao(fmt.Sprintf("Vào xem: mã %d, máy phát đang phát", n.Ma))
	} else {
		n.Bao(fmt.Sprintf("Vào xem: mã %d, chưa có ai phát", n.Ma))
	}
	go n.docTCP(r)
	go n.nhanUDP(udp)
	return n.Ma, nil
}

func (n *NguoiXem) gui(v any) {
	n.mu.Lock()
	tcp := n.tcp
	n.mu.Unlock()
	if tcp == nil {
		return
	}
	n.khoaTCP.Lock()
	defer n.khoaTCP.Unlock()
	GuiJSON(tcp, v)
}

func (n *NguoiXem) DaKetNoi() bool { n.mu.Lock(); defer n.mu.Unlock(); return n.daKetNoi }

func (n *NguoiXem) Ngat() {
	n.mu.Lock()
	tcp, udp := n.tcp, n.udp
	n.tcp, n.udp = nil, nil
	n.daKetNoi = false
	n.mu.Unlock()
	if tcp != nil {
		dongHaiChieu(tcp)
		tcp.Close()
	}
	if udp != nil {
		udp.Close()
	}
}

func (n *NguoiXem) docTCP(r *bufio.Reader) {
	for {
		tin, err := DocDong(r)
		if err != nil {
			break
		}
		switch tin.Chuoi("loai") {
		case "thong_tin":
			n.mu.Lock()
			n.thongTin = tin
			if n.choPhat { // vào trước khi phát: đo từ lúc bắt đầu phát
				n.choPhat = false
				if n.khungDauMs == nil {
					n.lucMoc = BayGio()
				}
			}
			n.mu.Unlock()
			n.Bao(fmt.Sprintf("Luồng %sx%s, %s fps, GOP %s, %s Hz", SoChu(tin["rong"]), SoChu(tin["cao"]),
				SoChu(tin["fps"]), SoChu(tin["gop"]), SoChu(tin["tan_so_mau"])))
		case "dong_ho":
			ts, ok1 := tin["ts"].(float64)
			gio, ok2 := tin["gio"].(float64)
			if ok1 && ok2 {
				n.mu.Lock()
				n.dongHoBao = &moc{ts, gio}
				n.mu.Unlock()
			}
		case "su_kien":
			n.Bao("Sự kiện: " + tin.Chuoi("noi_dung"))
		case "ket_thuc":
			n.mu.Lock()
			n.thongTin = nil
			n.choPhat = true
			n.lamMoiPhien()
			n.mu.Unlock()
			n.Bao("Máy phát kết thúc phát")
		case "bi_ngat":
			n.Bao("Bị ngắt: " + tin.Chuoi("ly_do"))
		}
	}
	if n.DaKetNoi() {
		n.Bao("Mất kết nối server")
		n.Ngat()
	}
}

// ------------------------------------------------------------ goroutine nhận UDP

// uocLuongTs – ước lượng ms phương tiện hiện tại (để biết khung I đầu tiên "cũ" bao nhiêu).
func (n *NguoiXem) uocLuongTs() *float64 {
	if n.dongHoBao != nil {
		return conTro(n.dongHoBao.ts + (GioUnixMs() - n.dongHoBao.gio)) // giả định cùng đồng hồ
	}
	var moi *float64
	if n.tsHinhMoi != nil && *n.tsHinhMoi != 0 {
		moi = conTro(*n.tsHinhMoi)
	}
	if n.tsAmMoi != nil && *n.tsAmMoi != 0 {
		x := *n.tsAmMoi * MsMoiMau
		if moi == nil || x > *moi {
			moi = conTro(x)
		}
	}
	return moi
}

func (n *NguoiXem) nhanUDP(udp *net.UDPConn) {
	for {
		buf := make([]byte, 2048)
		sz, err := udp.Read(buf)
		if err != nil {
			if ne, ok := err.(net.Error); ok && ne.Timeout() {
				continue
			}
			if errors.Is(err, net.ErrClosed) || !n.DaKetNoi() {
				return
			}
			continue // ví dụ ICMP "port unreachable" khi server chưa chạy
		}
		if sz == 0 {
			continue
		}
		goi := buf[:sz]
		n.mu.Lock()
		if n.matGoi > 0 && n.rng.Float64() < n.matGoi {
			n.boGiaLap++ // "mạng" làm rơi gói ở chiều xuống
			n.mu.Unlock()
			continue
		}
		bayGio := BayGio()
		n.byteNhan += sz
		n.wByte += sz
		switch goi[0] {
		case DauRtp:
			if r, ok := TachRtp(goi); ok && r.Pt == PtAmThanh && len(r.Payload) == ByteMoiKhung && n.amThanh {
				n.boDem.NhanGoi(r.Seq, r.Ts, r.Payload)
				n.lucNhanAm, n.tsAmMoi = conTro(bayGio), conTro(float64(r.Ts))
				n.goiAm++
				n.wAm++
			}
		case PhienBan:
			if n.video {
				if h, payload, ok := TachHeader(goi); ok && (h.Loai == LoaiI || h.Loai == LoaiP) {
					if k := n.ghep.Them(h, payload, bayGio); k != nil {
						n.nhanKhung(k, bayGio)
					}
				}
			}
		}
		n.mu.Unlock()
	}
}

// nhanKhung – khung vừa đủ mảnh -> giải mã (có thể giải thêm các khung đang chờ) -> hàng hiển thị.
func (n *NguoiXem) nhanKhung(k *KhungNen, bayGio float64) {
	for _, kh := range n.giaiMa.Them(k, bayGio) {
		ts := float64(kh.Ts)
		if n.tsHinhMoi == nil || ts > *n.tsHinhMoi {
			n.tsHinhMoi = conTro(ts)
		}
		if kh.Loai == LoaiI {
			n.khungI++
			if !n.coIDau {
				n.coIDau = true
				if uoc := n.uocLuongTs(); uoc != nil {
					n.iDauCu = conTro(*uoc - ts)
				}
				n.iDauDen = conTro((bayGio - n.lucMoc) * 1000)
			}
			if n.dangDung {
				n.dangDung = false
				ms := 0.0
				if luc, co := n.giaiMa.ThoiDiemMat(); co {
					ms = (bayGio - luc) * 1000
				}
				n.Bao(fmt.Sprintf("Hồi phục bằng khung I #%d sau %.0f ms", kh.So, ms))
			}
		} else {
			n.khungP++
		}
		n.hang = append(n.hang, kh)
		if len(n.hang) > HangHienThi {
			n.hang = n.hang[1:]
			n.boCu++
		}
	}
}

// ------------------------------------------------------------ goroutine phát 20 ms (đồng hồ chủ)

func (n *NguoiXem) vongAm() {
	keTiep := BayGio()
	dem := 0
	imLang := make([]int16, MauMoiKhung)
	for n.song.Load() {
		dem++
		var mau []int16
		n.mu.Lock()
		udp := n.udp
		var r *KhungPhat
		if n.daKetNoi && n.amThanh {
			r = n.boDem.Lay()
		}
		bayGio := BayGio()
		if r != nil {
			tsMs := float64(r.Ts) * MsMoiMau
			n.dongHoAm, n.lucPhatAm = conTro(tsMs), conTro(bayGio)
			n.lichSu = append(n.lichSu, moc{tsMs, bayGio})
			if len(n.lichSu) > 150 { // ~3 s gần nhất
				n.lichSu = n.lichSu[len(n.lichSu)-150:]
			}
			if r.Mat {
				mau = imLang // mất gói: phát im lặng
			} else {
				mau = ByteSangPcm(r.Payload)
				if n.dongHoBao != nil { // trễ đầu-cuối (cùng đồng hồ)
					tre := GioUnixMs() - (n.dongHoBao.gio + (tsMs - n.dongHoBao.ts))
					n.treHien = conTro(tre)
					n.tongTre += tre
					n.nTre++
					n.wTre = append(n.wTre, tre)
				}
			}
		}
		if mau != nil {
			n.mucAm = MucDbfs(mau)
		} else {
			n.mucAm = math.Inf(-1)
		}
		loa, ma := n.loa, n.Ma
		n.mu.Unlock()
		if loa != nil {
			if mau == nil {
				mau = imLang
			}
			loa.Phat(mau)
		}
		if udp != nil && dem%50 == 0 {
			udp.Write(GoiGiu(ma)) // giữ kết nối mỗi 1 s
		}
		keTiep += KhungMs / 1000.0
		con := keTiep - BayGio()
		if con < -0.1 {
			keTiep = BayGio() // tụt quá xa: bắt nhịp lại
		} else if con > 0 {
			ngu(con)
		}
	}
}

// ------------------------------------------------------------ goroutine hiển thị 5 ms

func (n *NguoiXem) vongHien() {
	for n.song.Load() {
		time.Sleep(5 * time.Millisecond)
		xin, guiPli := "", false
		n.mu.Lock()
		if !n.daKetNoi {
			n.mu.Unlock()
			continue
		}
		bayGio := BayGio()
		gm := n.giaiMa
		thieu := "None"
		if so, co := gm.ThamChieu(); co && gm.CoDoi() {
			thieu = strconv.FormatInt(so+1, 10)
		}
		duocXin := bayGio-n.lucPli >= PliXemS // giới hạn 1 lần / 1 s
		if n.video && gm.KiemTra(bayGio) {
			n.dungHinh++
			n.dangDung = true
			if duocXin {
				xin = "Thiếu khung #" + thieu + " -> đứng hình, xin khung khóa"
			} else {
				xin = "Thiếu khung #" + thieu + " -> đứng hình (xin sau giới hạn 1 s)"
			}
			guiPli = duocXin
		} else if n.video && gm.MatThamChieu && duocXin {
			xin = "Chưa có khung I: xin khung khóa" // bị giới hạn lúc trước / I bị mất
			guiPli = true
		}
		if guiPli {
			n.lucPli = bayGio
			n.pliGui++
		}
		n.ghep.DonDep(bayGio, -1)
		if chon := n.chonKhung(bayGio); chon != nil {
			n.hienKhung(chon, bayGio)
		}
		n.mu.Unlock()
		if xin != "" {
			n.Bao(xin)
			if guiPli {
				n.gui(J("loai", "xin_khung_khoa"))
			}
		}
	}
}

// dongHoHienTai – đồng hồ phát âm thanh hiện tại (ms phương tiện) hoặc nil nếu tiếng không chạy.
func (n *NguoiXem) dongHoHienTai(bayGio float64) *float64 {
	if n.lucPhatAm == nil || bayGio-*n.lucPhatAm > 0.2 {
		return nil
	}
	return conTro(*n.dongHoAm + math.Min(bayGio-*n.lucPhatAm, 0.04)*1000)
}

func (n *NguoiXem) chonKhung(bayGio float64) *KhungNen {
	if len(n.hang) == 0 {
		return nil
	}
	if !n.video {
		n.hang = nil
		return nil
	}
	// Tiếng "đang chạy": vừa nhận gói tiếng trong 0,5 s, hoặc mới vào xem chưa tới 0,5 s (chờ tiếng
	// tới để khung đầu cũng được đồng bộ). Không có tiếng -> hiện ngay, không chờ mãi.
	var tiengChay bool
	if n.lucNhanAm != nil {
		tiengChay = n.amThanh && bayGio-*n.lucNhanAm < 0.5
	} else {
		tiengChay = n.amThanh && bayGio-n.lucChao < 0.5
	}
	if n.dongBo && tiengChay {
		am := n.dongHoHienTai(bayGio)
		if am == nil {
			return nil // chờ đồng hồ tiếng (bộ đệm jitter đang nạp)
		}
		var chon *KhungNen
		for len(n.hang) > 0 && float64(n.hang[0].Ts) <= *am {
			if chon != nil {
				n.boCu++ // khung quá cũ: bỏ
			}
			k := n.hang[0]
			chon = &k
			n.hang = n.hang[1:]
		}
		return chon
	}
	k := n.hang[len(n.hang)-1] // không đồng bộ: hiện ngay khung mới nhất
	n.boCu += len(n.hang) - 1
	n.hang = nil
	return &k
}

// gioAm – giờ (đơn điệu) mà âm thanh có nhãn tsMs được/sẽ được phát – nội suy theo lịch sử phát.
func (n *NguoiXem) gioAm(tsMs float64) *float64 {
	for i := len(n.lichSu) - 1; i >= 0; i-- {
		if m := n.lichSu[i]; m.ts <= tsMs {
			return conTro(m.gio + (tsMs-m.ts)/1000)
		}
	}
	return nil
}

func (n *NguoiXem) hienKhung(k *KhungNen, bayGio float64) {
	n.hien = &KhungHien{k.W, k.H, k.DuLieu, k.Ts, k.So, n.dongHoHienTai(bayGio)}
	n.soHien++
	n.khungHien++
	n.wHien++
	if n.khungDauMs == nil {
		n.khungDauMs = conTro((bayGio - n.lucMoc) * 1000)
		// Nguồn khung đầu: khung I đã "cũ" (chụp trước lúc vào > 1,5 chu kỳ khung) -> từ bộ đệm GOP;
		// I mới nhưng tới muộn -> phải chờ khung I; I mới tới ngay -> vừa đúng lúc có khung I.
		fps := 10.0
		if n.thongTin != nil {
			fps = n.thongTin.So("fps", 10)
		}
		nguong := 1500 / fps
		them := ""
		switch {
		case n.iDauCu != nil && *n.iDauCu > nguong:
			n.nguonDau, them = "bộ đệm GOP", fmt.Sprintf(" (I cũ %.1f s)", *n.iDauCu/1000)
		case n.iDauDen != nil && *n.iDauDen > nguong:
			n.nguonDau = "chờ khung I"
		default:
			n.nguonDau = "khung I trực tiếp"
		}
		moc := ""
		if n.lucMoc > n.lucChao {
			moc = " (từ lúc phát)"
		}
		n.baoNgoai(fmt.Sprintf("Khung đầu sau %.0f ms%s: %s%s", *n.khungDauMs, moc, n.nguonDau, them))
	}
	if n.choHinhLai != nil {
		n.baoNgoai(fmt.Sprintf("Hình trở lại sau %.0f ms", (bayGio-*n.choHinhLai)*1000))
		n.choHinhLai = nil
	}
	tiengChay := n.amThanh && n.lucPhatAm != nil && bayGio-*n.lucPhatAm < 0.5
	var gio *float64
	if tiengChay {
		gio = n.gioAm(float64(k.Ts))
	}
	if gio != nil {
		lech := (bayGio - *gio) * 1000
		n.lechHien = conTro(lech)
		g := &n.giaiDoan[len(n.giaiDoan)-1]
		g.Tong += lech
		g.N++
		n.wLech = append(n.wLech, lech)
	} else {
		n.lechHien = nil
	}
}

// baoNgoai – báo khi đang giữ khóa: Bao chỉ in/đưa vào hàng đợi giao diện, không gọi lại lõi.
func (n *NguoiXem) baoNgoai(s string) { n.Bao(s) }

// ------------------------------------------------------------ điều khiển

func (n *NguoiXem) DatDongBo(bat bool) {
	n.mu.Lock()
	if bat != n.dongBo {
		n.dongBo = bat
		n.giaiDoan = append(n.giaiDoan, GiaiDoan{DongBo: bat})
	}
	n.mu.Unlock()
	if bat {
		n.Bao("Đồng bộ tiếng-hình: BẬT")
	} else {
		n.Bao("Đồng bộ tiếng-hình: TẮT")
	}
}

func (n *NguoiXem) DangKy(amThanh, video bool) {
	n.mu.Lock()
	if !video && n.video {
		n.ghep.BoDangDo()
		n.hang = nil
	}
	if video && !n.video {
		n.choHinhLai = conTro(BayGio())
	}
	if !amThanh && n.amThanh {
		n.boDem.Xoa()
		n.lichSu = nil
		n.lucPhatAm, n.lucNhanAm = nil, nil
	}
	if amThanh && !n.amThanh {
		n.matCu += n.boDem.Mat()
		n.boDem = MoiBoDemJitter(n.doSau)
	}
	n.amThanh, n.video = amThanh, video
	n.mu.Unlock()
	n.gui(J("loai", "dang_ky", "am_thanh", amThanh, "video", video))
	ten := "âm thanh+video"
	if !amThanh {
		ten = "video"
	} else if !video {
		ten = "âm thanh"
	}
	n.Bao("Đăng ký nhận: " + ten)
}

func (n *NguoiXem) DatDoSau(x int) {
	n.mu.Lock()
	n.doSau = gioiHan(x, 1, 25)
	n.boDem.DatDoSau(n.doSau)
	n.mu.Unlock()
}

func (n *NguoiXem) DatLoa(l LoaRa) { n.mu.Lock(); n.loa = l; n.mu.Unlock() }

// AnhChup – khung đang hiện kèm nhãn hình, đồng hồ tiếng LÚC HIỆN khung, lệch, số khung.
type AnhChup struct {
	W, H   int
	Rgb    []byte
	Ts     uint32
	DongHo *float64
	Lech   *float64
	So     int64
}

func (n *NguoiXem) Chup() *AnhChup {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.hien == nil {
		return nil
	}
	h := n.hien
	return &AnhChup{h.W, h.H, h.Rgb, h.Ts, h.DongHo, n.lechHien, h.So}
}

// Hien – khung đang hiện và số thứ tự lần hiện (để giao diện chỉ vẽ lại khi đổi).
func (n *NguoiXem) Hien() (*KhungHien, int) {
	n.mu.Lock()
	defer n.mu.Unlock()
	return n.hien, n.soHien
}

// ------------------------------------------------------------ số liệu

// ThongKeXemGiay – số liệu của giây vừa qua.
type ThongKeXemGiay struct {
	Am, Mat, Hien int
	Tre, Lech     *float64
	Kbps          float64
}

func trungBinh(x []float64) *float64 {
	if len(x) == 0 {
		return nil
	}
	t := 0.0
	for _, v := range x {
		t += v
	}
	return conTro(t / float64(len(x)))
}

// ThongKeMoiGiay – gọi mỗi 1 s: số liệu của giây vừa qua (rồi xóa bộ đếm cửa sổ).
func (n *NguoiXem) ThongKeMoiGiay() ThongKeXemGiay {
	n.mu.Lock()
	defer n.mu.Unlock()
	r := ThongKeXemGiay{n.wAm, n.matCu + n.boDem.Mat(), n.wHien, trungBinh(n.wTre), trungBinh(n.wLech),
		float64(n.wByte) * 8 / 1000}
	n.wAm, n.wHien, n.wByte = 0, 0, 0
	n.wTre, n.wLech = nil, nil
	return r
}

// TrangThaiXem – ảnh chụp cho dòng lệnh và giao diện.
type TrangThaiXem struct {
	DaKetNoi                  bool
	Ma                        uint16
	ThongTin                  Tin
	GoiAm, MatAm              int
	TreTB, TreHien            *float64
	KhungI, KhungP, KhungHien int
	Hong, BoP, BoCu           int
	DungHinh, PliGui          int
	KhungDauMs                *float64
	NguonDau                  string
	LechHien                  *float64
	GiaiDoan                  []GiaiDoan
	Kbps                      float64
	DongBo, AmThanh, Video    bool
	DoSau                     int
	MucAm                     float64
	SoHien                    int
	BoGiaLap                  int
}

func (n *NguoiXem) TrangThai() TrangThaiXem {
	n.mu.Lock()
	defer n.mu.Unlock()
	giay := math.Max(BayGio()-n.lucChao, 0.001)
	t := TrangThaiXem{DaKetNoi: n.daKetNoi, Ma: n.Ma, ThongTin: n.thongTin, GoiAm: n.goiAm,
		MatAm: n.matCu + n.boDem.Mat(), TreHien: n.treHien, KhungI: n.khungI, KhungP: n.khungP,
		KhungHien: n.khungHien, Hong: n.hongCu + n.ghep.KhungHong, BoP: n.boPCu + n.giaiMa.BoP, BoCu: n.boCu,
		DungHinh: n.dungHinh, PliGui: n.pliGui, KhungDauMs: n.khungDauMs, NguonDau: n.nguonDau,
		LechHien: n.lechHien, GiaiDoan: append([]GiaiDoan(nil), n.giaiDoan...),
		Kbps: float64(n.byteNhan) * 8 / 1000 / giay, DongBo: n.dongBo, AmThanh: n.amThanh, Video: n.video,
		DoSau: n.doSau, MucAm: n.mucAm, SoHien: n.soHien, BoGiaLap: n.boGiaLap}
	if n.nTre > 0 {
		t.TreTB = conTro(n.tongTre / float64(n.nTre))
	}
	return t
}

func (n *NguoiXem) Dong() {
	n.song.Store(false)
	n.Ngat()
	n.mu.Lock()
	loa := n.loa
	n.mu.Unlock()
	if loa != nil {
		loa.Dong()
	}
}

// DongTongKetXem – các dòng TỔNG KẾT của người xem (dùng cho dòng lệnh).
func DongTongKetXem(t TrangThaiXem, treVao float64) []string {
	dau := "vào ngay; "
	if treVao > 0 {
		dau = fmt.Sprintf("vào muộn %.1f s; ", treVao)
	}
	if t.KhungDauMs != nil {
		dau += fmt.Sprintf("khung đầu sau %.0f ms", *t.KhungDauMs)
	} else {
		dau += "chưa có khung nào"
	}
	nguon := t.NguonDau
	if nguon == "" {
		nguon = "–"
	}
	ds := []string{"===== TỔNG KẾT =====", dau,
		"  nguồn khung đầu: " + nguon,
		fmt.Sprintf("tiếng: nhận %d gói, mất %d, trễ TB %s ms", t.GoiAm, t.MatAm, ChuMs(t.TreTB, false)),
		"  (trễ đầu-cuối: giả định cùng đồng hồ)",
		fmt.Sprintf("hình: nhận %d khung (I %d, P %d), hỏng %d", t.KhungI+t.KhungP, t.KhungI, t.KhungP, t.Hong),
		fmt.Sprintf("  P bỏ chờ I %d; hiện %d, bỏ cũ %d", t.BoP, t.KhungHien, t.BoCu),
		fmt.Sprintf("đứng hình %d, xin khung khóa %d", t.DungHinh, t.PliGui),
		"lệch tiếng-hình TB (+ = hình chậm hơn tiếng):"}
	for i, g := range t.GiaiDoan {
		che := "TẮT"
		if g.DongBo {
			che = "BẬT"
		}
		if g.N > 0 {
			ds = append(ds, fmt.Sprintf("  %d. đồng bộ %s: %+.0f ms (n=%d)", i+1, che, g.Tong/float64(g.N), g.N))
		} else {
			ds = append(ds, fmt.Sprintf("  %d. đồng bộ %s: – (không có khung)", i+1, che))
		}
	}
	return append(ds, fmt.Sprintf("nhận TB %.0f kbit/s", t.Kbps))
}

// AnhChuThich – ảnh chụp có chú thích: khung đang hiện phóng ×2 + dải chữ nhãn hình, đồng hồ tiếng,
// lệch. Trả về (rộng, cao, rgb).
func AnhChuThich(c *AnhChup) (int, int, []byte) {
	W, H := 2*c.W, 2*c.H+62
	ra := append(PhongTo(c.Rgb, c.W, c.H, 2), make([]byte, W*62*3)...)
	ToMau(ra, W, 0, 2*c.H, W, 62, Mau{20, 20, 20})
	VeChu(ra, W, H, 6, 2*c.H+6, fmt.Sprintf("HINH  T=%.3f #%d", float64(c.Ts)/1000, c.So), 2, Mau{255, 255, 255})
	tieng := "TIENG -"
	if c.DongHo != nil {
		tieng = fmt.Sprintf("TIENG T=%.3f", *c.DongHo/1000)
	}
	VeChu(ra, W, H, 6, 2*c.H+24, tieng, 2, Mau{255, 255, 255})
	chu, mau := "LECH -", Mau{200, 200, 200}
	if c.Lech != nil {
		l := *c.Lech
		chu = fmt.Sprintf("LECH %+.0f MS", l)
		if l < -40 {
			chu += " HINH SOM"
		} else if l > 40 {
			chu += " HINH TRE"
		}
		if math.Abs(l) <= 40 {
			mau = Mau{120, 255, 120}
		} else {
			mau = Mau{255, 120, 120}
		}
	}
	VeChu(ra, W, H, 6, 2*c.H+42, chu, 2, mau)
	ToMau(ra, W, 0, 2*c.H, W, 2, Mau{255, 255, 0})
	return W, H, ra
}
