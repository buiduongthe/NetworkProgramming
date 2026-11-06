package tranhchap

// MayChuCoSo – khung máy chủ TCP đa luồng (một goroutine mỗi kết nối) của nhóm MR6.6–MR6.14: chào,
// quản trị, ma_yc lặp lại (idempotency), đẩy cap_nhat gộp tối đa 10 lần/giây. Nghiệp vụ (nghiepvu.go)
// viết giao diện NghiepVu:
//
//	khoiTao()                                  tạo các ô trong m.Kho, gọi m.DatSo(sổ kiểm toán)
//	xuLyYeuCau(khach, nguoi, tin, traLoi)      kiểm tra tin, gọi m.Kho.ThucHien(...)
//	trangThai() M                              trạng thái nghiệp vụ cho cap_nhat (đọc không khóa)
//	tomTat() string                            một dòng ngắn (≤ 48 ký tự) mô tả kết quả
//	dinhKy(bayGio)                             gọi mỗi 100 ms trên goroutine nền (đồng hồ, hết hạn…)
//	kiemThamSoNV(ts) error                     kiểm tra tham số đặt lại (ngoài kiểm tra "tham số lạ")
//
// Báo đổi trạng thái: m.BaoDoi() (đẩy cap_nhat ở nhịp kế tiếp); thông báo: m.ThongBao(text).

import (
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

type NghiepVu interface {
	khoiTao()
	xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M))
	trangThai() M
	tomTat() string
	dinhKy(bayGio float64)
	kiemThamSoNV(ts TS) error
}

// KhachKetNoi – một kết nối tới máy chủ (người dùng thật, kết nối gộp của bộ mô phỏng, hoặc quản trị).
type KhachKetNoi struct {
	Ma      int
	Ten     string
	VaiTro  string
	SoNguoi int // 0: người dùng thật; m: kết nối gộp m người ảo
	conn    net.Conn
	khoaGui sync.Mutex // nhiều goroutine (kết nối, hàng đợi, đẩy) cùng gửi
	daTra   map[int64]M
	thuTu   []int64 // ma_yc theo thứ tự nhận (bỏ cái cũ nhất khi quá NhoKQ)
}

func (k *KhachKetNoi) Gui(v any) bool {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	return GuiJSON(k.conn, v) == nil
}

// nhanYC – true nếu ma_yc mới. ma_yc đã gặp: gửi lại kết quả cũ (nếu đã có), trả về false.
func (k *KhachKetNoi) nhanYC(maYC int64) bool {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	if cu, co := k.daTra[maYC]; co {
		if cu != nil {
			lai := M{}
			for x, v := range cu {
				lai[x] = v
			}
			lai["lap_lai"] = true
			_ = GuiJSON(k.conn, lai)
		}
		return false
	}
	k.ghiNho(maYC, nil)
	return true
}

func (k *KhachKetNoi) ghiNho(maYC int64, tin M) {
	if _, co := k.daTra[maYC]; !co {
		k.thuTu = append(k.thuTu, maYC)
	}
	k.daTra[maYC] = tin
	for len(k.daTra) > NhoKQ && len(k.thuTu) > 0 {
		delete(k.daTra, k.thuTu[0])
		k.thuTu = k.thuTu[1:]
	}
}

func (k *KhachKetNoi) Tra(maYC int64, tin M) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	k.ghiNho(maYC, tin)
	_ = GuiJSON(k.conn, tin)
}

type MayChuCoSo struct {
	Ten           string
	LoaiYeuCau    []string
	ThamSoMacDinh TS
	Bao           func(string)
	ThongKe       *ThongKe
	Kho           *KhoTranhChap
	nv            NghiepVu

	muTS   sync.RWMutex
	thamSo TS

	so     atomic.Pointer[SoKiemToan]
	muKT   sync.RWMutex // đặt lại (ghi) / đọc trạng thái (đọc): không đọc ô giữa lúc đang tạo lại
	khoa   sync.Mutex   // bảo vệ ds (danh sách kết nối)
	ds     map[int]*KhachKetNoi
	chay   atomic.Bool
	DiaChi string

	doi    atomic.Bool
	muSK   sync.Mutex
	suKien []string

	srv    net.Listener
	maTiep int
	xong   sync.WaitGroup
}

// MoiMayChuCoSo – tạo khung; gọi Gan(nv, thamSo) sau khi tạo đối tượng nghiệp vụ.
func MoiMayChuCoSo(ten string, loai []string, macDinh TS, cheDo string, treMs float64, bao func(string)) (*MayChuCoSo, error) {
	if bao == nil {
		bao = func(string) {}
	}
	tk := MoiThongKe()
	kho, err := MoiKho(tk, cheDo, treMs)
	if err != nil {
		return nil, err
	}
	m := &MayChuCoSo{Ten: ten, LoaiYeuCau: loai, ThamSoMacDinh: macDinh, Bao: bao, ThongKe: tk, Kho: kho,
		ds: map[int]*KhachKetNoi{}}
	m.thamSo = append(TS(nil), macDinh...)
	return m, nil
}

// Gan – gắn nghiệp vụ, kiểm tra và áp dụng tham số ban đầu, tạo trạng thái.
func (m *MayChuCoSo) Gan(nv NghiepVu, thamSo TS) error {
	m.nv = nv
	if err := m.kiemThamSo(thamSo); err != nil {
		return err
	}
	for _, kv := range thamSo {
		m.thamSo = m.thamSo.Dat(kv.K, kv.V)
	}
	m.khoiTaoLai()
	return nil
}

func (m *MayChuCoSo) kiemThamSo(ts TS) error {
	for _, kv := range ts {
		if _, co := m.ThamSoMacDinh.Lay(kv.K); !co {
			return fmt.Errorf("tham số lạ: %s", kv.K)
		}
	}
	return m.nv.kiemThamSoNV(ts)
}

// ------------------------------------------------------------ tham số, sổ

func (m *MayChuCoSo) ThamSo() TS {
	m.muTS.RLock()
	defer m.muTS.RUnlock()
	return append(TS(nil), m.thamSo...)
}

func (m *MayChuCoSo) TS(k string) any {
	m.muTS.RLock()
	defer m.muTS.RUnlock()
	v, _ := m.thamSo.Lay(k)
	return v
}

func (m *MayChuCoSo) TSInt(k string) int64     { return SoNguyen(m.TS(k)) }
func (m *MayChuCoSo) TSFloat(k string) float64 { return SoThuc(m.TS(k)) }
func (m *MayChuCoSo) TSStr(k string) string    { return Chuoi(m.TS(k)) }

func (m *MayChuCoSo) DatSo(s *SoKiemToan) { m.so.Store(s) }
func (m *MayChuCoSo) So() *SoKiemToan     { return m.so.Load() }

// ------------------------------------------------------------ điều khiển

func (m *MayChuCoSo) khoiTaoLai() {
	m.Kho.TamDung(func() { // không còn thao tác nào đang chạy
		m.muKT.Lock()
		defer m.muKT.Unlock()
		m.Kho.XoaHet()
		m.ThongKe.DatLai()
		m.nv.khoiTao()
	})
	m.BaoDoi()
}

// DatLai – ép kiểu theo giá trị mặc định, kiểm tra, in tổng kết lượt trước, tạo lại trạng thái.
func (m *MayChuCoSo) DatLai(thamSo TS) error {
	var ts TS
	for _, kv := range thamSo {
		md, co := m.ThamSoMacDinh.Lay(kv.K)
		if !co {
			return fmt.Errorf("tham số lạ: %s", kv.K)
		}
		v, err := EpKieu(md, kv.V)
		if err != nil {
			return err
		}
		ts = ts.Dat(kv.K, v)
	}
	if err := m.kiemThamSo(ts); err != nil {
		return err
	}
	tk := m.ThongKe.AnhChup()
	if SoNguyen(tk["so_yc"]) > 0 {
		_, sp, _ := m.So().TongKet(m.Kho.AnhChup())
		m.Bao(fmt.Sprintf("Lượt trước: %s, %d yc, tc %d, sai phạm %d", m.Kho.CheDo(), SoNguyen(tk["so_yc"]),
			SoNguyen(tk["thanh_cong"]), sp))
	}
	m.muTS.Lock()
	for _, kv := range ts {
		m.thamSo = m.thamSo.Dat(kv.K, kv.V)
	}
	m.muTS.Unlock()
	m.khoiTaoLai()
	text := "Đặt lại"
	if len(ts) > 0 {
		var p []string
		for _, kv := range ts {
			p = append(p, kv.K+"="+StrPy(kv.V))
		}
		text += " (" + strings.Join(p, ", ") + ")"
	}
	m.ThongBao(text)
	return nil
}

func (m *MayChuCoSo) DatCheDo(cheDo string) error {
	if cheDo == m.Kho.CheDo() {
		return nil
	}
	if err := m.Kho.DatCheDo(cheDo); err != nil {
		return err
	}
	m.ThongBao(fmt.Sprintf("Chế độ: %s – %s", cheDo, TenCheDo[cheDo]))
	m.BaoDoi()
	return nil
}

func (m *MayChuCoSo) DatTre(ms float64) error {
	if err := m.Kho.DatTre(ms); err != nil {
		return err
	}
	m.Bao(fmt.Sprintf("Độ trễ xử lý: ngẫu nhiên 0–%s ms", FmtG(m.Kho.TreMs())))
	return nil
}

func (m *MayChuCoSo) BaoDoi() { m.doi.Store(true) }

// ThongBao – ghi nhật ký máy chủ và gửi {"loai":"su_kien"} cho mọi client (ở nhịp đẩy kế tiếp).
func (m *MayChuCoSo) ThongBao(text string) {
	m.Bao(text)
	m.muSK.Lock()
	m.suKien = append(m.suKien, text)
	m.muSK.Unlock()
}

func (m *MayChuCoSo) Chay() bool { return m.chay.Load() }

// ------------------------------------------------------------ khởi động / dừng

func (m *MayChuCoSo) BatDau(host string, cong int) error {
	if m.chay.Load() {
		return errors.New("máy chủ đang chạy")
	}
	srv, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(cong)))
	if err != nil {
		return err
	}
	m.srv, m.DiaChi = srv, fmt.Sprintf("%s:%d", host, cong)
	m.maTiep = 0
	m.chay.Store(true)
	m.xong.Add(2)
	go m.nhanKetNoi()
	go m.vongNen()
	m.Bao(fmt.Sprintf("Lắng nghe %s:%d; chế độ %s, trễ 0–%s ms", host, cong, m.Kho.CheDo(), FmtG(m.Kho.TreMs())))
	return nil
}

func (m *MayChuCoSo) Dung() {
	if !m.chay.Swap(false) {
		return
	}
	m.khoa.Lock()
	var ds []*KhachKetNoi
	for _, k := range m.ds {
		ds = append(ds, k)
	}
	m.khoa.Unlock()
	for _, k := range ds {
		if tc, ok := k.conn.(*net.TCPConn); ok {
			_ = tc.CloseRead()
		}
		_ = k.conn.Close()
	}
	_ = m.srv.Close()
	xong := make(chan struct{})
	go func() { m.xong.Wait(); close(xong) }()
	select {
	case <-xong:
	case <-time.After(2 * time.Second):
	}
	m.Bao("Máy chủ đã dừng")
}

// ------------------------------------------------------------ kết nối

func (m *MayChuCoSo) nhanKetNoi() {
	defer m.xong.Done()
	for m.chay.Load() {
		conn, err := m.srv.Accept()
		if err != nil {
			if !m.chay.Load() {
				return
			}
			var ne net.Error
			if errors.As(err, &ne) && ne.Timeout() {
				continue
			}
			return
		}
		go m.phucVu(conn)
	}
}

// phucVu – goroutine của một kết nối: chào, rồi đọc và xử lý lần lượt từng yêu cầu (có thể pipelining).
func (m *MayChuCoSo) phucVu(conn net.Conn) {
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}
	r := MoiBoDoc(conn)
	var k *KhachKetNoi
	defer func() {
		if k != nil {
			m.khoa.Lock()
			delete(m.ds, k.Ma)
			m.khoa.Unlock()
			if k.SoNguoi == 0 && k.VaiTro == "nguoi_dung" && m.chay.Load() {
				m.Bao(k.Ten + " rời")
			}
		}
		_ = conn.Close()
	}()
	tin, _, err := DocDong(r)
	if err != nil {
		return
	}
	soNguoi := 0
	hopLe := tin != nil && tin["loai"] == "chao" && TenHopLe(tin["ten"])
	vaiTro := "nguoi_dung"
	if hopLe {
		if v, co := tin["vai_tro"]; co {
			s, _ := v.(string)
			vaiTro = s
			hopLe = s == "nguoi_dung" || s == "quan_tri"
		}
		if v, co := tin["so_nguoi"]; co && v != nil {
			n, la := v.(int64)
			hopLe = hopLe && la && n > 0
			soNguoi = int(n)
		}
	}
	if !hopLe {
		_ = GuiJSON(conn, M{"loai": "loi", "ly_do": "cần chào: tên 1–16 ký tự, vai_tro hợp lệ"})
		return
	}
	k = &KhachKetNoi{Ten: tin["ten"].(string), VaiTro: vaiTro, conn: conn, SoNguoi: soNguoi, daTra: map[int64]M{}}
	k.khoaGui.Lock() // goroutine đẩy chưa gửi được gì cho k trước lời chào
	m.khoa.Lock()
	m.maTiep++
	k.Ma = m.maTiep
	m.ds[k.Ma] = k
	m.khoa.Unlock()
	e1 := GuiJSON(conn, TS{{"loai", "chao"}, {"ma", k.Ma}, {"che_do", m.Kho.CheDo()}, {"may_chu", m.Ten}})
	e2 := GuiJSON(conn, m.goiCapNhat())
	k.khoaGui.Unlock()
	if e1 != nil || e2 != nil {
		return
	}
	if soNguoi == 0 && k.VaiTro == "nguoi_dung" { // kết nối gộp, quản trị: không in
		m.Bao(k.Ten + " vào")
	}
	for m.chay.Load() {
		tin, tho, err := DocDong(r)
		if err != nil {
			if m.chay.Load() && k.SoNguoi == 0 {
				m.Bao(fmt.Sprintf("%s: lỗi kết nối (%v)", k.Ten, err))
			}
			return
		}
		if tin == nil {
			return
		}
		m.nhanTin(k, tin, tho)
	}
}

func (m *MayChuCoSo) nhanTin(k *KhachKetNoi, tin M, tho []byte) {
	loai, _ := tin["loai"].(string)
	maYC, la := tin["ma_yc"].(int64)
	if !la {
		k.Gui(M{"loai": "loi", "ly_do": "thiếu ma_yc (số nguyên)"})
		return
	}
	if !k.nhanYC(maYC) {
		return // ma_yc lặp lại: đã gửi lại kết quả cũ
	}
	traLoiLoi := func(maLoi, lyDo string) {
		k.Tra(maYC, M{"loai": "kq", "ma_yc": maYC, "ok": false, "ma_loi": maLoi, "ly_do": lyDo})
	}
	if laNghiepVu(m.LoaiYeuCau, loai) {
		var nguoi any = k.Ten
		if k.SoNguoi > 0 {
			nguoi = tin["nguoi"]
		}
		if !TenHopLe(nguoi) {
			traLoiLoi("sai_du_lieu", "thiếu tên người (nguoi)")
			return
		}
		t0 := time.Now()
		m.ThongKe.BatDauYC()
		traLoi := func(kq M) { // gọi từ goroutine kết nối hoặc goroutine hàng đợi
			m.ThongKe.XongYC(kq, float64(time.Since(t0).Nanoseconds())/1e3)
			tin := M{"loai": "kq", "ma_yc": maYC}
			for x, v := range kq {
				tin[x] = v
			}
			k.Tra(maYC, tin)
		}
		m.nv.xuLyYeuCau(k, nguoi.(string), tin, traLoi)
		return
	}
	switch loai {
	case "che_do", "tre", "dat_lai", "thong_ke":
		if k.VaiTro != "quan_tri" {
			traLoiLoi("khong_quyen", "chỉ quản trị")
			return
		}
		var err error
		switch loai {
		case "thong_ke":
			tk := m.ThongKeDayDu()
			tk["loai"], tk["ma_yc"] = "thong_ke", maYC
			k.Tra(maYC, tk)
			return
		case "che_do":
			s, la := tin["gia_tri"].(string)
			if !la {
				s = StrPy(tin["gia_tri"])
			}
			err = m.DatCheDo(s)
		case "tre":
			var ms any
			ms, err = EpKieu(0.0, tin["ms"])
			if err == nil {
				err = m.DatTre(ms.(float64))
			}
		default:
			var ts TS
			for _, x := range ThuTuKhoa(tho) {
				if x != "loai" && x != "ma_yc" {
					ts = ts.Dat(x, tin[x])
				}
			}
			err = m.DatLai(ts)
		}
		if err != nil {
			traLoiLoi("sai_du_lieu", err.Error())
			return
		}
		k.Tra(maYC, M{"loai": "kq", "ma_yc": maYC, "ok": true})
	default:
		traLoiLoi("khong_ho_tro", "loại không hỗ trợ: "+StrPy(tin["loai"]))
	}
}

func laNghiepVu(ds []string, loai string) bool {
	for _, x := range ds {
		if x == loai {
			return true
		}
	}
	return false
}

// ------------------------------------------------------------ đẩy trạng thái (gộp ≤ 10 lần/giây)

func (m *MayChuCoSo) goiCapNhat() M {
	m.muKT.RLock()
	tt := m.nv.trangThai()
	m.muKT.RUnlock()
	tt["loai"], tt["che_do"] = "cap_nhat", m.Kho.CheDo()
	return tt
}

func (m *MayChuCoSo) vongNen() {
	defer m.xong.Done()
	for m.chay.Load() {
		time.Sleep(NhipDay)
		func() {
			defer func() {
				if e := recover(); e != nil {
					m.Bao(fmt.Sprintf("lỗi định kỳ: %v", e))
				}
			}()
			m.nv.dinhKy(GioDonDieu())
		}()
		var goi []byte
		m.muSK.Lock()
		for _, s := range m.suKien {
			goi = append(goi, MaHoa(TS{{"loai", "su_kien"}, {"noi_dung", s}})...)
		}
		m.suKien = nil
		m.muSK.Unlock()
		if m.doi.Swap(false) { // xóa cờ TRƯỚC khi chụp: đổi sau đó vẫn được đẩy
			goi = append(goi, MaHoa(m.goiCapNhat())...)
		}
		if len(goi) == 0 {
			continue
		}
		m.khoa.Lock()
		var ds []*KhachKetNoi
		for _, k := range m.ds {
			ds = append(ds, k)
		}
		m.khoa.Unlock()
		for _, k := range ds {
			k.khoaGui.Lock()
			_, _ = k.conn.Write(goi)
			k.khoaGui.Unlock()
		}
	}
}

// ------------------------------------------------------------ số liệu

// TrangThai – trạng thái hiện tại (như cap_nhat gửi client) – giao diện máy chủ đọc định kỳ.
func (m *MayChuCoSo) TrangThai() M { return m.goiCapNhat() }

// SoKetNoi – (số kết nối, số người – kết nối gộp tính theo so_nguoi, quản trị không tính).
func (m *MayChuCoSo) SoKetNoi() (int, int) {
	m.khoa.Lock()
	defer m.khoa.Unlock()
	nguoi := 0
	for _, k := range m.ds {
		if k.VaiTro != "quan_tri" {
			nguoi += max(k.SoNguoi, 1)
		}
	}
	return len(m.ds), nguoi
}

// ThongKeDayDu – nội dung thông điệp thong_ke.
func (m *MayChuCoSo) ThongKeDayDu() M {
	tk := m.ThongKe.AnhChup()
	m.muKT.RLock()
	dsSP, tongSP, viDu := m.So().TongKet(m.Kho.AnhChup())
	tomTat := m.nv.tomTat()
	m.muKT.RUnlock()
	soKN, soNguoi := m.SoKetNoi()
	if len(viDu) > 3 {
		viDu = viDu[:3]
	}
	tk["che_do"], tk["tre_ms"] = m.Kho.CheDo(), m.Kho.TreMs()
	tk["hang_doi"] = int64(m.Kho.DoDaiHang())
	tk["sai_pham"], tk["sai_pham_tong"], tk["vi_du_sai_pham"] = dsSP, tongSP, append([]string{}, viDu...)
	tk["tom_tat"], tk["so_ket_noi"], tk["so_nguoi"] = tomTat, int64(soKN), int64(soNguoi)
	return tk
}
