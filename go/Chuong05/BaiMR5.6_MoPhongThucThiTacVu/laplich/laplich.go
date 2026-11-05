// Package laplich – Bài mở rộng MR5.6: bộ lập lịch mô phỏng đơn luồng/đa luồng × đồng bộ/bất đồng bộ.
//
// Thời gian rời rạc, 1 nhịp = 100 ms. Mỗi tác vụ T1, T2, T3 có thời lượng D giây, gồm ba bước:
// CPU (D/4) → chờ I/O (D/2, ví dụ đọc mạng hay đĩa) → CPU (phần còn lại).
// Tùy chọn:
//   - phụ thuộc: T3 chỉ bắt đầu khi T1 và T2 đã xong (WaitGroup.Wait / chờ kênh / join);
//   - chia: bước CPU cuối của T1 được chia đôi, hai nửa chạy cùng lúc trên hai luồng rồi hợp lại
//     (chỉ khi bước đó dài ≥ 2 nhịp);
//   - việc khẩn U: đến khi các luồng đang bận (mặc định lúc 0,3 s), cần 0,5 s CPU, ưu tiên cao;
//   - số lõi CPU và lát thời gian (quantum) của bộ lập lịch hệ điều hành.
//
// Bốn chế độ:
//
//	don_dong_bo      một luồng, lời gọi chặn: làm lần lượt theo thứ tự đến (T1, T2, T3 rồi U);
//	da_dong_bo       mỗi tác vụ một luồng hệ điều hành; hệ điều hành chia lát thời gian và cho luồng ưu
//	                 tiên cao CHEN NGANG ngay; T3 chờ T1, T2 bằng join; nửa sau CPU của T1 chạy trên T1b;
//	don_bat_dong_bo  một luồng chạy vòng lặp sự kiện: bước CPU chạy tới khi xong (hợp tác), chờ I/O không
//	                 giữ luồng; việc sẵn sàng xếp hàng FIFO;
//	da_bat_dong_bo   nhóm luồng thợ (số thợ = số lõi) + bất đồng bộ: mỗi bước CPU giao cho thợ rảnh đầu
//	                 tiên, nên sau một lần chờ I/O cùng một tác vụ có thể chạy tiếp trên MỘT LUỒNG KHÁC.
//
// Mỗi nhịp gồm bốn bước theo thứ tự cố định: (1) tác vụ đến, (2) các luồng/tác vụ tiến sang thao tác kế
// tiếp, (3) lập lịch (cấp lõi, giao việc cho thợ), (4) chạy một nhịp và ghi lại. Kết quả tất định: cùng
// tham số → cùng biểu đồ, cùng số liệu ở mọi ngôn ngữ. Gói này không tạo goroutine nào.
package laplich

import (
	"fmt"
	"math"
	"strings"
	"unicode/utf8"
)

// DanhSachCheDo – thứ tự cố định của bốn chế độ (như thứ tự khóa của dict CHE_DO bản Python).
var DanhSachCheDo = []string{"don_dong_bo", "da_dong_bo", "don_bat_dong_bo", "da_bat_dong_bo"}

// CheDo – tên đầy đủ của từng chế độ.
var CheDo = map[string]string{
	"don_dong_bo":     "Đơn luồng – đồng bộ (Single-threaded Sync)",
	"da_dong_bo":      "Đa luồng – đồng bộ (Multi-threaded Sync)",
	"don_bat_dong_bo": "Đơn luồng – bất đồng bộ (Single-threaded Async)",
	"da_bat_dong_bo":  "Đa luồng – bất đồng bộ (Multi-threaded Async)",
}

// TenNgan – tên ngắn dùng trong bảng so sánh.
var TenNgan = map[string]string{"don_dong_bo": "đơn, đồng bộ", "da_dong_bo": "đa, đồng bộ",
	"don_bat_dong_bo": "đơn, bất đ.bộ", "da_bat_dong_bo": "đa, bất đ.bộ"}

// Trạng thái một ô (một làn, một nhịp).
const (
	CPU     = "cpu"
	IO      = "io"
	SanSang = "san_sang"
	Cho     = "cho"
	Ranh    = "ranh"
	UCPU    = 5 // việc khẩn: 0,5 s CPU
)

func chiaSan(a, b int) int { // chia lấy phần nguyên về phía âm vô cùng (như // của Python)
	q := a / b
	if (a%b != 0) && ((a < 0) != (b < 0)) {
		q--
	}
	return q
}

// Giay – số nhịp (100 ms) → chuỗi giây, một chữ số thập phân.
func Giay(nhip int) string {
	q := chiaSan(nhip, 10)
	return fmt.Sprintf("%d.%d", q, nhip-q*10)
}

// lamTron – round() của Python: làm tròn về số chẵn gần nhất khi đúng nửa (banker's rounding).
func lamTron(x float64) int { return int(math.RoundToEven(x)) }

// ThamSo – tham số mô phỏng, thời gian tính bằng nhịp.
type ThamSo struct {
	D                    [3]int
	SoLoi, Lat           int
	PhuThuoc, Chia, Khan bool
	TKhan                int
}

// TaoThamSo nhận thời gian bằng giây (như ThamSo(...) bản Python).
func TaoThamSo(t1, t2, t3 float64, soLoi int, lat float64, phuThuoc, chia, khan bool, tKhan float64) *ThamSo {
	ts := &ThamSo{D: [3]int{lamTron(t1 * 10), lamTron(t2 * 10), lamTron(t3 * 10)},
		SoLoi: max(1, soLoi), Lat: max(1, lamTron(lat*10)),
		PhuThuoc: phuThuoc, Chia: chia, Khan: khan, TKhan: lamTron(tKhan * 10)}
	return ts
}

// Buoc – một bước của tác vụ: loại cpu | io | chia (CPU chia đôi chạy song song) và số nhịp.
type Buoc struct {
	Loai string
	N    int
}

// Viec – một tác vụ.
type Viec struct {
	Ten          string
	Buoc         []Buoc
	UuTien, Den  int
	PhuThuoc     []string
	BatDau, Xong int  // nhịp đầu tiên dùng CPU, nhịp hoàn tất (-1: chưa)
	k            int  // (bất đồng bộ) bước đang làm
	conCho       int  // (bất đồng bộ) số nửa của bước chia còn chưa xong
	daChay       bool // (bất đồng bộ) đã bắt đầu
}

func taoViec(ts *ThamSo) []*Viec {
	var ds []*Viec
	for i := 0; i < 3; i++ {
		n := ts.D[i]
		c1, io := n/4, n/2
		c2 := n - c1 - io
		loai3 := "cpu"
		if ts.Chia && i == 0 && c2 >= 2 { // < 2 nhịp: không chia được
			loai3 = "chia"
		}
		var buoc []Buoc
		for _, b := range []Buoc{{"cpu", c1}, {"io", io}, {loai3, c2}} {
			if b.N > 0 {
				buoc = append(buoc, b)
			}
		}
		var dep []string
		if ts.PhuThuoc && i == 2 {
			dep = []string{"T1", "T2"}
		}
		ds = append(ds, &Viec{Ten: fmt.Sprintf("T%d", i+1), Buoc: buoc, PhuThuoc: dep, BatDau: -1, Xong: -1})
	}
	if ts.Khan {
		ds = append(ds, &Viec{Ten: "U", Buoc: []Buoc{{"cpu", UCPU}}, UuTien: 1, Den: ts.TKhan, BatDau: -1, Xong: -1})
	}
	return ds
}

// O – một ô của biểu đồ: trạng thái và nhãn (tên tác vụ).
type O struct {
	TrangThai, Nhan string
}

// Lan – một làn (một luồng, vòng lặp hay thợ) của biểu đồ.
type Lan struct {
	Ten string
	O   []O
}

// DongNhatKy – một dòng nhật ký: nhịp và nội dung.
type DongNhatKy struct {
	T    int
	Text string
}

// KetQua – kết quả một lần chạy.
type KetQua struct {
	CheDo             string
	TS                *ThamSo
	Lan               []Lan
	IO                []int // số I/O đang chờ mỗi nhịp (bất đồng bộ)
	NhatKy            []DongNhatKy
	Viec              []*Viec
	ChenNgang, HetLat int
	Tong              int
}

func (kq *KetQua) log(t int, text string) { kq.NhatKy = append(kq.NhatKy, DongNhatKy{t, text}) }

// ==================================================================== đồng bộ (luồng hệ điều hành)

// thao tác của một luồng
type thaoTac struct {
	loai string
	n    int      // số nhịp còn lại (cpu, io, tao_phu)
	deps []string // cho_viec
	phu  *luong   // cho_phu
	text string   // ghi
	nhan string
	v    *Viec
}

type luong struct {
	ten       string
	uuTien    int
	ct        []*thaoTac // chương trình: các thao tác chưa làm
	op        *thaoTac   // thao tác đang làm
	ketThuc   bool
	trangThai string
	latDaChay int
	o         []O
}

type ganSS struct { // một mục của hàng sẵn sàng: luồng và số thứ tự vào hàng
	l   *luong
	stt int
}

type dongBo struct {
	ts       *ThamSo
	nhieu    bool
	kq       *KetQua
	viec     []*Viec
	luong    []*luong
	sanSang  []ganSS
	dangChay []*luong
	stt      int
	hang     []*Viec // đơn luồng: tác vụ đã đến, chờ luồng chính (FIFO)
	chinh    *luong
	t        int
}

func (d *dongBo) log(text string) { d.kq.log(d.t, text) }

func (d *dongBo) daXong(ten string) bool {
	for _, v := range d.viec {
		if v.Ten == ten && v.Xong >= 0 {
			return true
		}
	}
	return false
}

func (d *dongBo) duPhuThuoc(deps []string) bool {
	for _, x := range deps {
		if !d.daXong(x) {
			return false
		}
	}
	return true
}

// ctViec – chương trình của tác vụ v: [chờ phụ thuộc], các bước, xong.
func (d *dongBo) ctViec(v *Viec, motLuong bool) []*thaoTac {
	var ct []*thaoTac
	if len(v.PhuThuoc) > 0 && !motLuong {
		ct = append(ct, &thaoTac{loai: "cho_viec", deps: append([]string(nil), v.PhuThuoc...), nhan: v.Ten, v: v})
	}
	for _, b := range v.Buoc {
		if b.Loai == "chia" {
			a, c := (b.N+1)/2, b.N/2
			if motLuong {
				ct = append(ct, &thaoTac{loai: "ghi", text: v.Ten + ": chia CPU cuối 2 nửa, một luồng nên làm lần lượt", nhan: v.Ten, v: v},
					&thaoTac{loai: "cpu", n: a, nhan: v.Ten, v: v}, &thaoTac{loai: "cpu", n: c, nhan: v.Ten, v: v})
			} else {
				ct = append(ct, &thaoTac{loai: "tao_phu", n: c, nhan: v.Ten, v: v},
					&thaoTac{loai: "cpu", n: a, nhan: v.Ten, v: v}, &thaoTac{loai: "cho_phu", nhan: v.Ten, v: v})
			}
		} else {
			ct = append(ct, &thaoTac{loai: b.Loai, n: b.N, nhan: v.Ten, v: v})
		}
	}
	return append(ct, &thaoTac{loai: "xong", nhan: v.Ten, v: v})
}

func (d *dongBo) themLuong(ten string, uuTien int, ct []*thaoTac) *luong {
	L := &luong{ten: ten, uuTien: uuTien, ct: ct, trangThai: Ranh, o: make([]O, d.t)}
	for i := range L.o {
		L.o[i] = O{Ranh, ""}
	}
	d.luong = append(d.luong, L)
	return L
}

func (d *dongBo) vaoSanSang(L *luong) {
	d.stt++
	d.sanSang = append(d.sanSang, ganSS{L, d.stt})
}

func viTri(ds []*luong, L *luong) int {
	for i, x := range ds {
		if x == L {
			return i
		}
	}
	return -1
}

func (d *dongBo) roiLoi(L *luong) {
	if i := viTri(d.dangChay, L); i >= 0 {
		d.dangChay = append(d.dangChay[:i], d.dangChay[i+1:]...)
	}
}

func (d *dongBo) trongSanSang(L *luong) bool {
	for _, s := range d.sanSang {
		if s.l == L {
			return true
		}
	}
	return false
}

func (d *dongBo) tatCaDaDen() bool {
	for _, v := range d.viec {
		if v.Den > d.t {
			return false
		}
	}
	return true
}

// tien – bước 2: luồng L làm các thao tác tức thời và sang thao tác kế tiếp nếu thao tác hiện tại đã xong.
func (d *dongBo) tien(L *luong) {
	for {
		if op := L.op; op != nil {
			if (op.loai == "cpu" || op.loai == "io") && op.n > 0 {
				return
			}
			if op.loai == "cho_viec" {
				if !d.duPhuThuoc(op.deps) {
					return
				}
				d.log(fmt.Sprintf("%s: %s đã xong (join), %s chạy tiếp", L.ten, strings.Join(op.deps, " và "), op.nhan))
			}
			if op.loai == "cho_phu" {
				if !op.phu.ketThuc {
					return
				}
				d.log(fmt.Sprintf("%s: hai nửa của %s đã xong (join %s)", L.ten, op.nhan, op.phu.ten))
			}
		}
		if len(L.ct) == 0 {
			if !d.nhieu && L == d.chinh && len(d.hang) > 0 {
				v := d.hang[0]
				d.hang = d.hang[1:]
				d.log("Luồng chính bắt đầu " + v.Ten)
				L.ct = d.ctViec(v, true)
			} else {
				L.op = nil
				d.roiLoi(L)
				if d.nhieu || d.tatCaDaDen() {
					L.ketThuc = true
				}
				L.trangThai = Ranh
				return
			}
		}
		L.op = L.ct[0]
		L.ct = L.ct[1:]
		op := L.op
		switch op.loai {
		case "cpu":
			if viTri(d.dangChay, L) < 0 && !d.trongSanSang(L) {
				d.vaoSanSang(L)
			}
			if viTri(d.dangChay, L) >= 0 {
				L.trangThai = CPU
			} else {
				L.trangThai = SanSang
			}
			return
		}
		d.roiLoi(L)
		switch op.loai {
		case "io":
			L.trangThai = IO
			return
		case "cho_viec":
			L.trangThai = Cho
			if !d.duPhuThuoc(op.deps) {
				d.log(fmt.Sprintf("%s: %s chờ %s xong (join)", L.ten, op.nhan, strings.Join(op.deps, " và ")))
			}
		case "tao_phu":
			phu := d.themLuong("Luồng "+op.nhan+"b", L.uuTien,
				[]*thaoTac{{loai: "cpu", n: op.n, nhan: op.nhan, v: op.v}, {loai: "het", nhan: op.nhan, v: op.v}})
			d.log(fmt.Sprintf("%s: chia CPU cuối 2 nửa, %s chạy nửa sau", op.nhan, phu.ten))
			L.ct[1].phu = phu // thao tác cho_phu chờ đúng luồng phụ này
		case "cho_phu":
			L.trangThai = Cho
		case "ghi":
			d.log(op.text)
		case "xong":
			op.v.Xong = d.t
			d.log(fmt.Sprintf("%s xong lúc %s s", op.v.Ten, Giay(d.t)))
		case "het":
		}
	}
}

func (d *dongBo) totNhat() int { // chỉ số mục tốt nhất trong hàng sẵn sàng: ưu tiên cao, rồi đến trước
	best := 0
	for i, s := range d.sanSang {
		b := d.sanSang[best]
		if s.l.uuTien > b.l.uuTien || (s.l.uuTien == b.l.uuTien && s.stt < b.stt) {
			best = i
		}
	}
	return best
}

func (d *dongBo) boSanSang(i int) ganSS {
	s := d.sanSang[i]
	d.sanSang = append(d.sanSang[:i], d.sanSang[i+1:]...)
	return s
}

// lapLich – bước 3: bộ lập lịch hệ điều hành cấp lõi CPU cho các luồng sẵn sàng.
func (d *dongBo) lapLich() {
	ts := d.ts
	// a. hết lát thời gian: nhường lõi nếu có luồng ưu tiên bằng hoặc cao hơn đang chờ
	for _, r := range append([]*luong(nil), d.dangChay...) {
		if r.latDaChay >= ts.Lat {
			coCho := false
			for _, s := range d.sanSang {
				if s.l.uuTien >= r.uuTien {
					coCho = true
					break
				}
			}
			if coCho {
				d.roiLoi(r)
				r.latDaChay = 0
				d.vaoSanSang(r)
				d.kq.HetLat++
				d.log(fmt.Sprintf("%s hết lát thời gian %s s, nhường lõi", r.ten, Giay(ts.Lat)))
			}
		}
	}
	// b. lõi còn trống: cấp cho luồng sẵn sàng tốt nhất
	for len(d.dangChay) < ts.SoLoi && len(d.sanSang) > 0 {
		s := d.boSanSang(d.totNhat())
		s.l.latDaChay = 0
		d.dangChay = append(d.dangChay, s.l)
	}
	// c. chen ngang: luồng sẵn sàng ưu tiên cao hơn luồng đang chạy thì lấy lõi ngay
	for len(d.sanSang) > 0 {
		bi := d.totNhat()
		s := d.sanSang[bi]
		nanI := -1 // nạn nhân: ưu tiên thấp nhất, rồi lát đã chạy nhiều nhất, rồi vị trí đầu
		for i, r := range d.dangChay {
			if r.uuTien >= s.l.uuTien {
				continue
			}
			if nanI < 0 {
				nanI = i
				continue
			}
			n := d.dangChay[nanI]
			if r.uuTien < n.uuTien || (r.uuTien == n.uuTien && r.latDaChay > n.latDaChay) {
				nanI = i
			}
		}
		if nanI < 0 {
			break
		}
		nan := d.dangChay[nanI]
		d.boSanSang(bi)
		s.l.latDaChay = 0
		d.dangChay[nanI] = s.l
		nan.latDaChay = 0
		d.vaoSanSang(nan)
		d.kq.ChenNgang++
		d.log(fmt.Sprintf("%s CHEN NGANG: %s tạm dừng, nhường lõi", s.l.ten, nan.ten))
	}
	for _, L := range d.luong {
		if L.op != nil && L.op.loai == "cpu" && L.op.n > 0 {
			if viTri(d.dangChay, L) >= 0 {
				L.trangThai = CPU
			} else {
				L.trangThai = SanSang
			}
		}
	}
}

func (d *dongBo) chay() *KetQua {
	for {
		// 1. tác vụ đến
		for _, v := range d.viec {
			if v.Den != d.t {
				continue
			}
			if v.UuTien != 0 {
				d.log(v.Ten + " (việc khẩn, ưu tiên cao) đến")
			}
			if d.nhieu {
				L := d.themLuong("Luồng "+v.Ten, v.UuTien, d.ctViec(v, false))
				if v.UuTien != 0 {
					d.log("tạo " + L.ten)
				}
			} else {
				d.hang = append(d.hang, v)
				if d.chinh == nil {
					d.chinh = d.themLuong("Luồng chính", 0, nil)
				} else if v.UuTien != 0 && (d.chinh.op != nil || len(d.chinh.ct) > 0) {
					d.log(v.Ten + " xếp hàng: luồng chính bận, không chen ngang được")
				}
			}
		}
		// 2. tiến (theo thứ tự tạo luồng; luồng mới tạo trong bước này cũng được tiến). Hai lượt để luồng
		//    đang join thấy ngay luồng có số thứ tự lớn hơn vừa kết thúc trong cùng nhịp.
		for lan := 0; lan < 2; lan++ {
			for i := 0; i < len(d.luong); i++ {
				if !d.luong[i].ketThuc {
					d.tien(d.luong[i])
				}
			}
		}
		het := true
		for _, L := range d.luong {
			if !L.ketThuc {
				het = false
				break
			}
		}
		if het && d.tatCaDaDen() {
			break
		}
		// 3. lập lịch
		d.lapLich()
		// 4. chạy một nhịp
		for _, L := range d.luong {
			op := L.op
			switch {
			case L.ketThuc || op == nil:
				L.o = append(L.o, O{Ranh, ""})
			case L.trangThai == CPU:
				op.n--
				L.latDaChay++
				if op.v.BatDau < 0 {
					op.v.BatDau = d.t
				}
				L.o = append(L.o, O{CPU, op.nhan})
			case L.trangThai == IO:
				op.n--
				L.o = append(L.o, O{IO, op.nhan})
			default:
				L.o = append(L.o, O{L.trangThai, op.nhan})
			}
		}
		d.t++
	}
	d.kq.Tong = d.t
	for _, L := range d.luong {
		d.kq.Lan = append(d.kq.Lan, Lan{L.ten, L.o})
	}
	return d.kq
}

// ==================================================================== bất đồng bộ (vòng lặp / nhóm thợ)

type phanViec struct { // một bước CPU: tác vụ, số nhịp còn lại, là một nửa của bước chia?
	v   *Viec
	n   int
	nua bool
}

type choIO struct {
	v *Viec
	n int
}

type batDongBo struct {
	ts      *ThamSo
	nhieu   bool
	kq      *KetQua
	viec    []*Viec
	tenTho  []string
	tho     []*phanViec // việc đang chạy trên mỗi thợ (nil: rảnh)
	o       [][]O
	hang    []*phanViec // các bước CPU sẵn sàng, FIFO
	choIO   []*choIO
	cuoiTho map[string]int // tác vụ -> thợ chạy bước CPU gần nhất
	t       int
}

func (b *batDongBo) log(text string) { b.kq.log(b.t, text) }

func (b *batDongBo) duPhuThuoc(v *Viec) bool {
	for _, d := range v.PhuThuoc {
		ok := false
		for _, w := range b.viec {
			if w.Ten == d && w.Xong >= 0 {
				ok = true
				break
			}
		}
		if !ok {
			return false
		}
	}
	return true
}

// batDauBuoc – tác vụ v bắt đầu bước v.k (hoặc hoàn tất nếu hết bước).
func (b *batDongBo) batDauBuoc(v *Viec) {
	if v.k >= len(v.Buoc) {
		v.Xong = b.t
		b.log(fmt.Sprintf("%s xong lúc %s s", v.Ten, Giay(b.t)))
		for _, w := range b.viec { // tác vụ đang chờ (await) các tác vụ vừa xong
			if !w.daChay && w.Den <= b.t && len(w.PhuThuoc) > 0 && b.duPhuThuoc(w) {
				b.log(fmt.Sprintf("%s đã xong → %s bắt đầu (await)", strings.Join(w.PhuThuoc, " và "), w.Ten))
				b.khoiDong(w)
			}
		}
		return
	}
	bc := v.Buoc[v.k]
	switch bc.Loai {
	case "cpu":
		b.hang = append(b.hang, &phanViec{v, bc.N, false})
	case "io":
		b.choIO = append(b.choIO, &choIO{v, bc.N})
	default: // chia: hai nửa vào hàng cùng lúc
		v.conCho = 2
		b.hang = append(b.hang, &phanViec{v, (bc.N + 1) / 2, true}, &phanViec{v, bc.N / 2, true})
		if len(b.tho) > 1 {
			b.log(v.Ten + ": chia CPU cuối 2 nửa, chạy song song trên 2 thợ")
		} else {
			b.log(v.Ten + ": chia CPU cuối 2 nửa, một luồng nên làm lần lượt")
		}
	}
}

func (b *batDongBo) khoiDong(v *Viec) {
	v.daChay = true
	v.k = 0
	b.batDauBuoc(v)
}

func (b *batDongBo) chay() *KetQua {
	soTho := len(b.tho)
	for {
		// 1. tác vụ đến
		for _, v := range b.viec {
			if v.Den != b.t {
				continue
			}
			if v.UuTien != 0 {
				b.log(v.Ten + " (việc khẩn) đến – xếp hàng, không chen ngang được")
			}
			if len(v.PhuThuoc) > 0 && !b.duPhuThuoc(v) {
				b.log(fmt.Sprintf("%s chờ %s xong (await), không giữ luồng nào", v.Ten, strings.Join(v.PhuThuoc, " và ")))
			} else {
				b.khoiDong(v)
			}
		}
		// 2. tiến: bước CPU xong (theo thứ tự thợ), rồi I/O xong (theo thứ tự bắt đầu chờ)
		for i := 0; i < soTho; i++ {
			x := b.tho[i]
			if x != nil && x.n == 0 {
				b.tho[i] = nil
				v := x.v
				if x.nua {
					v.conCho--
					if v.conCho == 0 {
						b.log(v.Ten + ": hai nửa đã xong, hợp kết quả")
						v.k++
						b.batDauBuoc(v)
					}
				} else {
					v.k++
					b.batDauBuoc(v)
				}
			}
		}
		for _, x := range append([]*choIO(nil), b.choIO...) {
			if x.n == 0 {
				for j, y := range b.choIO {
					if y == x {
						b.choIO = append(b.choIO[:j], b.choIO[j+1:]...)
						break
					}
				}
				x.v.k++
				b.batDauBuoc(x.v)
			}
		}
		xongHet := true
		for _, v := range b.viec {
			if v.Xong < 0 {
				xongHet = false
				break
			}
		}
		if xongHet {
			break
		}
		// 3. giao việc: thợ rảnh (theo thứ tự) lấy bước CPU đứng đầu hàng
		for i := 0; i < soTho; i++ {
			if b.tho[i] == nil && len(b.hang) > 0 {
				x := b.hang[0]
				b.hang = b.hang[1:]
				b.tho[i] = x
				truoc, co := b.cuoiTho[x.v.Ten]
				if b.nhieu && co && truoc != i && !x.nua {
					b.log(fmt.Sprintf("%s chạy tiếp trên %s (trước đó ở %s)", x.v.Ten, b.tenTho[i], b.tenTho[truoc]))
				}
				b.cuoiTho[x.v.Ten] = i
			}
		}
		// 4. chạy một nhịp
		for i := 0; i < soTho; i++ {
			x := b.tho[i]
			if x == nil {
				b.o[i] = append(b.o[i], O{Ranh, ""})
			} else {
				x.n--
				if x.v.BatDau < 0 {
					x.v.BatDau = b.t
				}
				b.o[i] = append(b.o[i], O{CPU, x.v.Ten})
			}
		}
		b.kq.IO = append(b.kq.IO, len(b.choIO))
		for _, x := range b.choIO {
			x.n--
		}
		b.t++
	}
	b.kq.Tong = b.t
	for i := 0; i < soTho; i++ {
		b.kq.Lan = append(b.kq.Lan, Lan{b.tenTho[i], b.o[i]})
	}
	return b.kq
}

// Chay mô phỏng một chế độ.
func Chay(cheDo string, ts *ThamSo) *KetQua {
	switch cheDo {
	case "don_dong_bo", "da_dong_bo":
		nhieu := cheDo == "da_dong_bo"
		d := &dongBo{ts: ts, nhieu: nhieu, kq: &KetQua{CheDo: cheDo, TS: ts}}
		d.viec = taoViec(ts)
		d.kq.Viec = d.viec
		return d.chay()
	}
	nhieu := cheDo == "da_bat_dong_bo"
	b := &batDongBo{ts: ts, nhieu: nhieu, kq: &KetQua{CheDo: cheDo, TS: ts}, cuoiTho: map[string]int{}}
	b.viec = taoViec(ts)
	b.kq.Viec = b.viec
	soTho := 1
	b.tenTho = []string{"Vòng lặp"}
	if nhieu {
		soTho = ts.SoLoi
		b.tenTho = nil
		for i := 0; i < soTho; i++ {
			b.tenTho = append(b.tenTho, fmt.Sprintf("Thợ %d", i+1))
		}
	}
	b.tho = make([]*phanViec, soTho)
	b.o = make([][]O, soTho)
	return b.chay()
}

// ==================================================================== chỉ số và biểu đồ văn bản

// ChiSo – các chỉ số của một lần chạy.
type ChiSo struct {
	Tong  int // hoàn tất (nhịp)
	UCho  int // độ trễ phản hồi của U (nhịp, -1 nếu không có U)
	Luong int // số luồng
	Ban   int // % lõi bận
	Chen  int
	Lat   int
}

// TinhChiSo – như chi_so() bản Python.
func TinhChiSo(kq *KetQua) ChiSo {
	ban := 0
	for _, l := range kq.Lan {
		for _, c := range l.O {
			if c.TrangThai == CPU {
				ban++
			}
		}
	}
	uCho := -1
	for _, v := range kq.Viec {
		if v.Ten == "U" {
			uCho = v.BatDau - v.Den
			break
		}
	}
	return ChiSo{Tong: kq.Tong, UCho: uCho, Luong: len(kq.Lan),
		Ban: ban * 100 / max(1, kq.TS.SoLoi*kq.Tong), Chen: kq.ChenNgang, Lat: kq.HetLat}
}

// Trai – căn trái chuỗi trong n ký tự (đếm theo ký tự Unicode như Python, không theo byte như %-11s).
func Trai(s string, n int) string {
	if k := utf8.RuneCountInString(s); k < n {
		return s + strings.Repeat(" ", n-k)
	}
	return s
}

// Phai – căn phải chuỗi trong n ký tự (đếm theo ký tự Unicode).
func Phai(s string, n int) string {
	if k := utf8.RuneCountInString(s); k < n {
		return strings.Repeat(" ", n-k) + s
	}
	return s
}

var kyHieu = map[string]string{IO: "~", SanSang: "-", Cho: ".", Ranh: " "}

func kyTu(c O) string {
	if c.TrangThai == CPU {
		if c.Nhan == "U" {
			return "U"
		}
		return c.Nhan[1:2] // T1 → "1"
	}
	return kyHieu[c.TrangThai]
}

func cat[T any](s []T, dau, cuoi int) []T { // s[dau:cuoi] an toàn như lát cắt của Python
	cuoi = min(cuoi, len(s))
	if dau >= cuoi {
		return nil
	}
	return s[dau:cuoi]
}

// Gantt – biểu đồ Gantt văn bản, 1 ô = 1 nhịp (0,1 s); dài hơn `rong` nhịp thì xuống dòng thành nhiều khối.
func Gantt(kq *KetQua, rong int) []string {
	var dong []string
	for dau := 0; dau < kq.Tong; dau += rong {
		cuoi := min(kq.Tong, dau+rong)
		for _, l := range kq.Lan {
			var sb strings.Builder
			for _, c := range cat(l.O, dau, cuoi) {
				sb.WriteString(kyTu(c))
			}
			dong = append(dong, "  "+Trai(l.Ten, 11)+"|"+sb.String()+"|")
		}
		if len(kq.IO) > 0 {
			var sb strings.Builder
			for _, n := range cat(kq.IO, dau, cuoi) {
				if n != 0 {
					fmt.Fprint(&sb, min(n, 9))
				} else {
					sb.WriteString(" ")
				}
			}
			dong = append(dong, "  "+Trai("I/O chờ", 11)+"|"+sb.String()+"|")
		}
		nhan := Giay(cuoi) + " s"
		dong = append(dong, "  "+Trai("", 11)+" "+Giay(dau)+Phai(nhan, cuoi-dau-utf8.RuneCountInString(Giay(dau))+2))
	}
	return dong
}
