package tranhchap

// KhoTranhChap – kho các ô tài nguyên; mọi thao tác GHI đi qua ThucHien() và được xử lý theo chế độ
// hiện tại (khung chung MR6.6–MR6.14).
//
// Nghiệp vụ viết hàm THUẦN  xuLy(doc) -> (kq, ghi):
//   - doc: {khóa ô: giá trị đọc được} cho các ô trong cacKhoa;
//   - kq:  thông điệp trả lời client (M{"ok": true/false, "ma_loi", "ly_do", ...});
//   - ghi: nil (không ghi) hoặc {khóa ô: GIÁ TRỊ MỚI} – chỉ các ô trong cacKhoa. Giá trị ô là struct
//     (sao chép theo giá trị) nên xuLy không sửa được doc tại chỗ – lac_quan có thể gọi xuLy nhiều lần.
// khiGhi(kq) (tùy chọn) được gọi NGAY SAU khi ghi, vẫn trong vùng găng của chế độ (dưới khóa ô với
// khoa, dưới khóa CAS với lac_quan, trong goroutine xử lý với hang_doi): nơi ghi sổ kiểm toán đúng thứ tự.
// traLoi(kq) được gọi đúng một lần khi xong (goroutine gọi, hoặc goroutine xử lý của hang_doi).
//
// "Cổng" (TamDung): đổi chế độ / đặt lại chỉ diễn ra khi không còn thao tác nào đang chạy hay đang
// xếp hàng – không bao giờ có hai chế độ chạy lẫn vào nhau.
//
// An toàn bộ nhớ: giá trị ô nằm trong atomic.Pointer, phiên bản trong atomic.Uint64 – chế độ
// khong_khoa đọc/ghi KHÔNG đồng bộ về mặt nghiệp vụ (đọc – nghỉ – ghi đè) nhưng mỗi lần đọc/ghi một ô
// là nguyên tử, nên chương trình vẫn đúng theo mô hình bộ nhớ của Go (không đọc phải giá trị "rách").

import (
	"fmt"
	"math"
	"math/rand/v2"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

type hop struct{ v any }

// o – một ô tài nguyên (món hàng, ghế, tài khoản…): giá trị, phiên bản (cho lac_quan), khóa riêng
// (cho khoa). Khóa là kênh đệm 1 chỗ: gửi vào = khóa, nhận ra = mở; dùng kênh thay sync.Mutex vì cần
// "chờ khóa tối đa 2 s" (sync.Mutex không có Lock có thời hạn).
type o struct {
	giaTri   atomic.Pointer[hop]
	phienBan atomic.Uint64
	khoa     chan struct{}
}

func (x *o) doc() any { return x.giaTri.Load().v }

func (x *o) khoaCho(han time.Duration) bool {
	select {
	case x.khoa <- struct{}{}:
		return true
	default:
	}
	t := time.NewTimer(han)
	defer t.Stop()
	select {
	case x.khoa <- struct{}{}:
		return true
	case <-t.C:
		return false
	}
}

func (x *o) moKhoa() { <-x.khoa }

type viec struct {
	cacO   []*o
	cacKh  []string
	xuLy   func(M) (M, M)
	khiGhi func(M)
	traLoi func(M)
}

type KhoTranhChap struct {
	ThongKe     *ThongKe
	cheDo       atomic.Int32
	treMs       atomic.Uint64 // math.Float64bits
	KhoaNgayTho atomic.Bool   // khoa: true = khóa theo thứ tự cacKhoa (có thể khóa chết)

	muO sync.RWMutex // bảo vệ bản đồ ô (thêm/xóa ô chỉ khi đặt lại)
	o   map[string]*o

	khoaCas sync.Mutex // lac_quan: khóa rất ngắn quanh đọc ảnh chụp và so sánh-và-đổi

	muCong   sync.Mutex
	cong     *sync.Cond
	dongCong bool
	dangChay int

	hang chan *viec // hang_doi: MỘT goroutine đọc kênh này
}

func chiSoCheDo(cd string) int32 {
	for i, x := range CheDoDS {
		if x == cd {
			return int32(i)
		}
	}
	return -1
}

func MoiKho(tk *ThongKe, cheDo string, treMs float64) (*KhoTranhChap, error) {
	i := chiSoCheDo(cheDo)
	if i < 0 {
		return nil, fmt.Errorf("chế độ không hợp lệ: %s", cheDo)
	}
	k := &KhoTranhChap{ThongKe: tk, o: map[string]*o{}, hang: make(chan *viec, 1<<17)}
	k.cheDo.Store(i)
	k.treMs.Store(math.Float64bits(treMs))
	k.cong = sync.NewCond(&k.muCong)
	go k.vongHangDoi()
	return k, nil
}

// ------------------------------------------------------------ ô tài nguyên

func (k *KhoTranhChap) DatO(khoa string, giaTri any) {
	x := &o{khoa: make(chan struct{}, 1)}
	x.giaTri.Store(&hop{giaTri})
	k.muO.Lock()
	k.o[khoa] = x
	k.muO.Unlock()
}

func (k *KhoTranhChap) XoaHet() {
	k.muO.Lock()
	k.o = map[string]*o{}
	k.muO.Unlock()
}

// Doc – đọc không khóa (để hiển thị): giá trị được thay nguyên khối nên luôn là một bản trọn vẹn;
// ô chưa có (đang đặt lại) -> nil.
func (k *KhoTranhChap) Doc(khoa string) any {
	k.muO.RLock()
	x := k.o[khoa]
	k.muO.RUnlock()
	if x == nil {
		return nil
	}
	return x.doc()
}

func (k *KhoTranhChap) AnhChup() M {
	k.muO.RLock()
	defer k.muO.RUnlock()
	d := M{}
	for kh, x := range k.o {
		d[kh] = x.doc()
	}
	return d
}

func (k *KhoTranhChap) DoDaiHang() int { return len(k.hang) }

func (k *KhoTranhChap) CheDo() string { return CheDoDS[k.cheDo.Load()] }

func (k *KhoTranhChap) TreMs() float64 { return math.Float64frombits(k.treMs.Load()) }

// ------------------------------------------------------------ cấu hình

// TamDung – đóng cổng, chờ mọi thao tác đang chạy/đang xếp hàng xong, chạy f, mở lại cổng.
func (k *KhoTranhChap) TamDung(f func()) {
	k.muCong.Lock()
	for k.dongCong {
		k.cong.Wait()
	}
	k.dongCong = true
	for k.dangChay > 0 {
		k.cong.Wait()
	}
	k.muCong.Unlock()
	defer func() {
		k.muCong.Lock()
		k.dongCong = false
		k.cong.Broadcast()
		k.muCong.Unlock()
	}()
	f()
}

func (k *KhoTranhChap) DatCheDo(cheDo string) error {
	i := chiSoCheDo(cheDo)
	if i < 0 {
		return fmt.Errorf("chế độ không hợp lệ: %s", cheDo)
	}
	k.TamDung(func() { k.cheDo.Store(i) })
	return nil
}

func (k *KhoTranhChap) DatTre(ms float64) error {
	if !(ms >= 0 && ms <= 100) {
		return fmt.Errorf("độ trễ 0–100 ms")
	}
	k.treMs.Store(math.Float64bits(ms))
	return nil
}

// tre – giả lập thời gian xử lý (đọc cơ sở dữ liệu, kiểm tra nghiệp vụ): ngẫu nhiên 0 … treMs.
func (k *KhoTranhChap) tre() {
	if ms := k.TreMs(); ms > 0 {
		time.Sleep(time.Duration(rand.Float64() * ms * float64(time.Millisecond)))
	}
}

// ------------------------------------------------------------ thực hiện một thao tác

func (k *KhoTranhChap) ThucHien(cacKhoa []string, xuLy func(doc M) (kq, ghi M), traLoi func(M), khiGhi func(M)) {
	var ds []string
	gap := map[string]bool{}
	for _, kh := range cacKhoa { // bỏ trùng, giữ thứ tự
		if !gap[kh] {
			gap[kh] = true
			ds = append(ds, kh)
		}
	}
	k.muCong.Lock()
	for k.dongCong {
		k.cong.Wait()
	}
	k.dangChay++
	cheDo := k.CheDo()
	k.muCong.Unlock()
	k.muO.RLock()
	cacO := make([]*o, len(ds))
	thieu := false
	for i, kh := range ds {
		cacO[i] = k.o[kh]
		thieu = thieu || cacO[i] == nil
	}
	k.muO.RUnlock()
	if thieu {
		k.xongMot()
		traLoi(M{"ok": false, "ma_loi": "khong_co", "ly_do": "không có tài nguyên"})
		return
	}
	v := &viec{cacO, ds, xuLy, khiGhi, traLoi}
	if cheDo == "hang_doi" {
		k.hang <- v
		k.ThongKe.DatMax("hang_doi_max", int64(len(k.hang)))
		return // goroutine xử lý sẽ gọi traLoi
	}
	var kq M
	func() {
		defer func() {
			if e := recover(); e != nil { // lỗi nghiệp vụ: trả lời lỗi, không làm sập goroutine
				kq = M{"ok": false, "ma_loi": "loi", "ly_do": fmt.Sprintf("lỗi xử lý: %v", e)}
			}
		}()
		switch cheDo {
		case "khong_khoa":
			kq = k.motLan(v)
		case "khoa":
			kq = k.coKhoa(v)
		default:
			kq = k.lacQuan(v)
		}
	}()
	k.xongMot()
	traLoi(kq)
}

func (k *KhoTranhChap) xongMot() {
	k.muCong.Lock()
	k.dangChay--
	if k.dangChay == 0 {
		k.cong.Broadcast()
	}
	k.muCong.Unlock()
}

func (k *KhoTranhChap) docTatCa(v *viec) M {
	doc := make(M, len(v.cacO))
	for i, x := range v.cacO {
		doc[v.cacKh[i]] = x.doc()
	}
	return doc
}

func (k *KhoTranhChap) ghi(v *viec, ghi, kq M) {
	for i, kh := range v.cacKh {
		if gt, co := ghi[kh]; co {
			v.cacO[i].giaTri.Store(&hop{gt})
			v.cacO[i].phienBan.Add(1)
		}
	}
	if v.khiGhi != nil {
		v.khiGhi(kq)
	}
}

// motLan – khong_khoa (và goroutine hang_doi): đọc -> trễ -> xử lý -> ghi, KHÔNG khóa.
// Hai goroutine cùng đọc giá trị cũ rồi cùng ghi: bản ghi sau đè bản trước (mất cập nhật).
func (k *KhoTranhChap) motLan(v *viec) M {
	doc := k.docTatCa(v)
	k.tre()
	kq, ghi := v.xuLy(doc)
	if len(ghi) > 0 {
		k.ghi(v, ghi, kq)
	}
	return kq
}

// coKhoa – khoa: giữ khóa của MỌI ô liên quan suốt đọc – xử lý – ghi (vùng găng – critical section).
// Khóa theo thứ tự khóa ô đã sắp xếp => mọi goroutine xin khóa cùng một thứ tự => không khóa chết.
// KhoaNgayTho: khóa theo thứ tự yêu cầu (ví dụ nguồn rồi đích), nghỉ giữa hai lần khóa.
func (k *KhoTranhChap) coKhoa(v *viec) M {
	thuTu := make([]int, len(v.cacKh))
	for i := range thuTu {
		thuTu[i] = i
	}
	ngayTho := k.KhoaNgayTho.Load()
	if !ngayTho {
		sort.SliceStable(thuTu, func(a, b int) bool { return v.cacKh[thuTu[a]] < v.cacKh[thuTu[b]] })
	}
	var daGiu []int
	defer func() {
		for j := len(daGiu) - 1; j >= 0; j-- {
			v.cacO[daGiu[j]].moKhoa()
		}
	}()
	for n, i := range thuTu {
		if n > 0 && ngayTho {
			k.tre()
		}
		if !v.cacO[i].khoaCho(time.Duration(ChoKhoaToiDaS * float64(time.Second))) {
			k.ThongKe.Cong("khoa_cho_lau", 1)
			return M{"ok": false, "ma_loi": "khoa_cho_lau",
				"ly_do": fmt.Sprintf("chờ khóa %s quá %s s (khóa chết?)", v.cacKh[i], FmtG(ChoKhoaToiDaS))}
		}
		daGiu = append(daGiu, i)
	}
	return k.motLan(v)
}

// lacQuan – lac_quan (Kung & Robinson 1981): không giữ khóa khi xử lý; lúc ghi kiểm tra phiên bản.
func (k *KhoTranhChap) lacQuan(v *viec) M {
	phienBan := make([]uint64, len(v.cacO))
	for lan := 0; lan < SoLanThu; lan++ {
		k.khoaCas.Lock() // ảnh chụp nhất quán của các ô
		doc := k.docTatCa(v)
		for i, x := range v.cacO {
			phienBan[i] = x.phienBan.Load()
		}
		k.khoaCas.Unlock()
		k.tre()
		kq, ghi := v.xuLy(doc)
		if len(ghi) == 0 {
			return kq // chỉ đọc / từ chối: không cần so sánh-và-đổi
		}
		if func() bool { // so sánh-và-đổi (compare-and-swap)
			k.khoaCas.Lock()
			defer k.khoaCas.Unlock()
			for i, x := range v.cacO {
				if x.phienBan.Load() != phienBan[i] {
					return false
				}
			}
			k.ghi(v, ghi, kq)
			return true
		}() {
			return kq
		}
		k.ThongKe.Cong("xung_dot", 1) // có goroutine khác ghi trước: làm lại từ đầu
		if lan < SoLanThu-1 {
			k.ThongKe.Cong("thu_lai", 1)
			time.Sleep(time.Duration(rand.Float64() * float64(time.Millisecond)))
		}
	}
	k.ThongKe.Cong("het_lan_thu", 1)
	return M{"ok": false, "ma_loi": "xung_dot", "ly_do": "xung đột, thử lại sau"}
}

// vongHangDoi – hang_doi: MỘT goroutine lấy lần lượt từng việc từ kênh (đến trước phục vụ trước) –
// không cần khóa vì chỉ goroutine này chạm vào trạng thái.
func (k *KhoTranhChap) vongHangDoi() {
	for v := range k.hang {
		if v == nil {
			return
		}
		var kq M
		func() {
			defer func() {
				if e := recover(); e != nil {
					kq = M{"ok": false, "ma_loi": "loi", "ly_do": fmt.Sprintf("lỗi xử lý: %v", e)}
				}
			}()
			kq = k.motLan(v)
		}()
		k.xongMot()
		func() {
			defer func() { _ = recover() }()
			v.traLoi(kq)
		}()
	}
}

func (k *KhoTranhChap) Dong() { k.hang <- nil }
