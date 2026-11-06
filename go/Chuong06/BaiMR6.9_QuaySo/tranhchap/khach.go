package tranhchap

// Lõi client dùng chung (giao diện người dùng, bộ mô phỏng, quản trị) – khung chung MR6.6–MR6.14,
// không có gì về nghiệp vụ.
//
// KetNoiKhach – một kết nối TCP:
//
//	KetNoi(...)       chào, đọc trả lời chào và cap_nhat đầu tiên (đồng bộ) -> TrangThai()
//	GuiYC(loai, khiKq, truong)   gửi một yêu cầu (tự đánh ma_yc), KHÔNG chờ: khiKq(tin, rttMs) được gọi
//	                  khi có trả lời cùng ma_yc (pipelining: gửi tiếp được ngay)
//	GuiLo([...])      gửi nhiều yêu cầu trong MỘT lần Write (bộ mô phỏng "bắn đồng loạt")
//	Hoi(loai, ...)    gửi rồi CHỜ trả lời (cần goroutine đọc chạy nền) – dùng cho quản trị
//	BatDauDoc()       goroutine đọc chạy nền (giao diện);  VongDoc(xong)  đọc ngay trên goroutine gọi
//
// Gọi lại (từ goroutine đọc): KhiCapNhat(tin), KhiTin(tin) (su_kien, loi, trả lời lạ), KhiDong().
// QuanTri – kết nối vai_tro "quan_tri": CheDo(), Tre(), DatLai(ts), ThongKe().

import (
	"bufio"
	"errors"
	"fmt"
	"net"
	"strconv"
	"sync"
	"sync/atomic"
	"time"
)

type choKQ struct {
	khiKq func(M, float64)
	t0    time.Time
}

type KetNoiKhach struct {
	KhiCapNhat func(M)
	KhiTin     func(M)
	KhiDong    func()
	conn       net.Conn
	r          *bufio.Reader
	mo         atomic.Bool
	imDong     atomic.Bool
	Chao       M
	muTT       sync.Mutex
	trangThai  M
	ma         int64
	cho        map[int64]choKQ
	khoa       sync.Mutex // bảo vệ cho và việc gửi
}

func MoiKetNoiKhach() *KetNoiKhach { return &KetNoiKhach{cho: map[int64]choKQ{}} }

func (kn *KetNoiKhach) Mo() bool { return kn.mo.Load() }

// TrangThai – cap_nhat mới nhất.
func (kn *KetNoiKhach) TrangThai() M {
	kn.muTT.Lock()
	defer kn.muTT.Unlock()
	return kn.trangThai
}

// ------------------------------------------------------------ kết nối

// KetNoi – soNguoi > 0: kết nối gộp của bộ mô phỏng chở soNguoi người ảo.
func (kn *KetNoiKhach) KetNoi(host string, cong int, ten, vaiTro string, soNguoi int, choS float64) (M, error) {
	han := time.Duration(choS * float64(time.Second))
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), han)
	if err != nil {
		return nil, err
	}
	if tc, ok := conn.(*net.TCPConn); ok {
		_ = tc.SetNoDelay(true)
	}
	chao := TS{{"loai", "chao"}, {"ten", ten}, {"vai_tro", vaiTro}}
	if soNguoi > 0 {
		chao = append(chao, KV{"so_nguoi", soNguoi})
	}
	_ = conn.SetDeadline(time.Now().Add(han))
	r := MoiBoDoc(conn)
	loi := func(e error) (M, error) { _ = conn.Close(); return nil, e }
	if err := GuiJSON(conn, chao); err != nil {
		return loi(err)
	}
	tin, _, err := DocDong(r)
	if err != nil {
		return loi(err)
	}
	if tin == nil || tin["loai"] != "chao" {
		lyDo := "máy chủ đóng kết nối"
		if tin != nil {
			if s, ok := tin["ly_do"].(string); ok {
				lyDo = s
			}
		}
		return loi(errors.New(lyDo))
	}
	tt, _, err := DocDong(r) // máy chủ gửi ngay một cap_nhat sau lời chào
	if err != nil {
		return loi(err)
	}
	_ = conn.SetDeadline(time.Time{})
	kn.Chao = tin
	kn.muTT.Lock()
	kn.trangThai = tt
	kn.muTT.Unlock()
	kn.conn, kn.r = conn, r
	kn.mo.Store(true)
	return tin, nil
}

// TatBaoDong – không gọi KhiDong nữa (người dùng tự rời/thoát).
func (kn *KetNoiKhach) TatBaoDong() { kn.imDong.Store(true) }

func (kn *KetNoiKhach) Dong() {
	kn.mo.Store(false)
	if kn.conn != nil {
		if tc, ok := kn.conn.(*net.TCPConn); ok {
			_ = tc.CloseRead()
		}
		_ = kn.conn.Close()
	}
}

// ------------------------------------------------------------ gửi

func (kn *KetNoiKhach) dangKy(loai string, khiKq func(M, float64), truong TS) []byte {
	kn.ma++
	kn.cho[kn.ma] = choKQ{khiKq, time.Now()}
	tin := append(TS{{"loai", loai}, {"ma_yc", kn.ma}}, truong...)
	return MaHoa(tin)
}

// GuiYC – gửi một yêu cầu, không chờ; trả về ma_yc.
func (kn *KetNoiKhach) GuiYC(loai string, khiKq func(M, float64), truong TS) (int64, error) {
	kn.khoa.Lock()
	defer kn.khoa.Unlock()
	if kn.conn == nil {
		return 0, errors.New("chưa kết nối")
	}
	b := kn.dangKy(loai, khiKq, truong)
	_, err := kn.conn.Write(b)
	return kn.ma, err
}

// YeuCauLo – một yêu cầu trong lô.
type YeuCauLo struct {
	Loai   string
	Truong TS
	KhiKq  func(M, float64)
}

// GuiLo – một lần Write cho cả lô.
func (kn *KetNoiKhach) GuiLo(ds []YeuCauLo) error {
	kn.khoa.Lock()
	defer kn.khoa.Unlock()
	var b []byte
	for _, y := range ds {
		b = append(b, kn.dangKy(y.Loai, y.KhiKq, y.Truong)...)
	}
	_, err := kn.conn.Write(b)
	return err
}

// Hoi – gửi rồi chờ trả lời (tối đa choS giây).
func (kn *KetNoiKhach) Hoi(loai string, choS float64, truong TS) (M, error) {
	xong := make(chan M, 1)
	if _, err := kn.GuiYC(loai, func(tin M, _ float64) { xong <- tin }, truong); err != nil {
		return nil, err
	}
	select {
	case kq := <-xong:
		return kq, nil
	case <-time.After(time.Duration(choS * float64(time.Second))):
		return nil, fmt.Errorf("không có trả lời cho %s", loai)
	}
}

// ------------------------------------------------------------ đọc

func (kn *KetNoiKhach) BatDauDoc() { go kn.VongDoc(nil) }

// VongDoc – đọc và phân phát tới khi kết nối đóng, hoặc (nếu có) xong() đúng sau một thông điệp.
func (kn *KetNoiKhach) VongDoc(xong func() bool) {
	for {
		if xong != nil && xong() {
			return // đủ việc: kết nối vẫn mở, người gọi tự đóng
		}
		tin, _, err := DocDong(kn.r)
		if err != nil || tin == nil {
			break
		}
		kn.phanPhat(tin)
	}
	kn.mo.Store(false)
	if kn.KhiDong != nil && !kn.imDong.Load() {
		kn.KhiDong()
	}
}

func (kn *KetNoiKhach) phanPhat(tin M) {
	var cho choKQ
	co := false
	if ma, la := tin["ma_yc"].(int64); la {
		if lap, _ := tin["lap_lai"].(bool); !lap {
			kn.khoa.Lock()
			cho, co = kn.cho[ma]
			delete(kn.cho, ma)
			kn.khoa.Unlock()
		}
	}
	switch {
	case co:
		if cho.khiKq != nil {
			cho.khiKq(tin, float64(time.Since(cho.t0).Nanoseconds())/1e6)
		}
	case tin["loai"] == "cap_nhat":
		kn.muTT.Lock()
		kn.trangThai = tin
		kn.muTT.Unlock()
		if kn.KhiCapNhat != nil {
			kn.KhiCapNhat(tin)
		}
	default:
		if kn.KhiTin != nil {
			kn.KhiTin(tin)
		}
	}
}

// QuanTri – kết nối quản trị (đồng bộ): mỗi lệnh chờ trả lời; bị từ chối -> lỗi.
type QuanTri struct{ *KetNoiKhach }

func MoiQuanTri(host string, cong int, ten string) (*QuanTri, error) {
	kn := MoiKetNoiKhach()
	if _, err := kn.KetNoi(host, cong, ten, "quan_tri", 0, 5); err != nil {
		return nil, err
	}
	kn.BatDauDoc()
	return &QuanTri{kn}, nil
}

func (q *QuanTri) lenh(loai string, truong TS) (M, error) {
	kq, err := q.Hoi(loai, 30, truong)
	if err != nil {
		return nil, err
	}
	if ok, co := kq["ok"].(bool); co && !ok {
		lyDo, la := kq["ly_do"].(string)
		if !la {
			lyDo = "bị từ chối"
		}
		return nil, errors.New(lyDo)
	}
	return kq, nil
}

func (q *QuanTri) CheDo(giaTri string) error {
	_, err := q.lenh("che_do", TS{{"gia_tri", giaTri}})
	return err
}
func (q *QuanTri) Tre(ms float64) error { _, err := q.lenh("tre", TS{{"ms", ms}}); return err }
func (q *QuanTri) DatLai(ts TS) error   { _, err := q.lenh("dat_lai", ts); return err }
func (q *QuanTri) ThongKe() (M, error)  { return q.lenh("thong_ke", nil) }
