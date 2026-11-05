package tranhchap

// NGHIỆP VỤ MR6.13 – lõi máy chủ bán vé có phòng chờ ảo + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Bán 500 vé concert cho rất nhiều người cùng lúc. Ba thao tác của người dùng:
//
//	{"loai":"xep_hang","ma_yc":k}         vào phòng chờ -> số thứ tự, số người phía trước, thời gian chờ ước tính
//	{"loai":"cho_luot","ma_yc":k}         "chờ dài" (long polling): máy chủ CHỈ trả lời khi tới lượt (kèm han_ms –
//	                                      thời gian còn lại để mua) hoặc khi hết vé
//	{"loai":"mua","ma_yc":k,"so_ve":2}    mua 1–4 vé; khi phòng chờ bật phải đang có lượt còn hạn
//
// (kết nối gộp của bộ mô phỏng thêm "nguoi":"u17").
//
// Phòng chờ ảo (virtual waiting room): máy chủ cấp lượt theo THÙNG TOKEN (token bucket): token rót vào đều
// toc_do token/giây, thùng chứa tối đa dung_luong token; mỗi 100 ms goroutine nền lấy người đầu hàng (số
// thứ tự nhỏ nhất) ra, mỗi người tốn một token -> lượt có hạn luot_s giây (mặc định 20 s) để mua. Nhờ vậy
// phía "mua" (phần đắt: kiểm tra, ghi kho) chỉ nhận đều đặn vài chục người mỗi giây dù hàng chờ có hàng
// nghìn người. Tắt phòng chờ (phong_cho = 0): ai cũng gọi mua ngay -> dồn cục; máy chủ chỉ cho tối đa
// gioi_han_xu_ly yêu cầu mua đang xử lý cùng lúc, quá số đó từ chối ngay "qua_tai" (như HTTP 503).
//
// Các ô của KhoTranhChap (đi qua 4 cách xử lý tranh chấp):
//
//	"hang"        hangO{cuoi: số thứ tự cuối đã phát} – xep_hang đọc, +1, ghi: phát số thứ tự;
//	"kho"         khoVe{con: vé còn, daBan: đã bán} – mua đọc, kiểm tra số vé, trừ kho;
//	"luot:<tên>"  luotO{con: còn lượt?, han: hạn} – tạo khi cấp lượt; mua đọc cùng "kho" và ghi con = false
//	              trong CÙNG thao tác (tiêu lượt nguyên tử với trừ kho).
//
// Sổ kiểm toán đếm ba sai phạm:
//
//	ban_qua    "vượt vé"  – số vé bán vượt tổng số vé (theo sổ: cộng số vé mọi đơn được chấp nhận);
//	khong_luot "sai lượt" – một đơn được chấp nhận khi người đó không còn lượt (lượt đã dùng ở tab khác);
//	dao_thu_tu "đảo số"   – người vào hàng sau nhận số thứ tự không lớn hơn người vào trước (FIFO bị phá).

import (
	"container/heap"
	"fmt"
	"math"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"
)

const TenMayChu = "Bán vé MR6.13"

var LoaiYeuCau = []string{"xep_hang", "cho_luot", "mua"}

var ThamSoMacDinh = TS{{"ten_su_kien", "Đêm nhạc Mùa Đông 2026"}, {"gia_ve", int64(1_500_000)}, {"tong_ve", int64(500)},
	{"ve_toi_da", int64(4)}, {"phong_cho", int64(1)}, {"toc_do", 50.0}, {"dung_luong", int64(50)}, {"luot_s", 20.0},
	{"gioi_han_xu_ly", int64(64)}}

type khoVe struct{ con, daBan int64 }
type hangO struct{ cuoi int64 }
type luotO struct {
	con bool
	han float64
}

func kqHetVe() M { return M{"ok": false, "ma_loi": "het_ve", "ly_do": "đã bán hết vé"} }

// ------------------------------------------------------------ sổ kiểm toán

type gdPhongCho struct {
	loai, nguoi string
	so, soVe    int64
	canLuot     bool
}

type luatPhongCho struct {
	tongVe int64
	daBan  atomic.Int64    // tổng vé theo sổ
	soMax  int64           // số thứ tự lớn nhất đã phát (theo thứ tự ghi sổ)
	luot   map[string]bool // người đang có lượt chưa dùng (theo sổ)
}

func (l *luatPhongCho) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdPhongCho)
	switch gd.loai {
	case "xep":
		if gd.so <= l.soMax {
			so.SaiPham("dao_thu_tu", fmt.Sprintf("%s vào sau nhưng nhận số %d ≤ %d", gd.nguoi, gd.so, l.soMax))
		} else {
			l.soMax = gd.so
		}
	case "luot":
		l.luot[gd.nguoi] = true
	case "mua":
		if gd.canLuot {
			if l.luot[gd.nguoi] {
				delete(l.luot, gd.nguoi)
			} else {
				so.SaiPham("khong_luot", fmt.Sprintf("%s mua %d vé khi lượt đã dùng", gd.nguoi, gd.soVe))
			}
		}
		truoc := l.daBan.Load()
		sau := l.daBan.Add(gd.soVe)
		if vuot := sau - max(truoc, l.tongVe); vuot > 0 { // đếm theo SỐ VÉ bán vượt
			so.dem["ban_qua"] += vuot - 1
			so.SaiPham("ban_qua", fmt.Sprintf("%s mua %d vé, sổ đã bán %d", gd.nguoi, gd.soVe, sau))
		}
	}
}

func (l *luatPhongCho) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// ------------------------------------------------------------ hàng chờ (heap theo số thứ tự)

type phanTuHang struct {
	so, stt int64
	ten     string
}

type dongHeap []phanTuHang

func (h dongHeap) Len() int { return len(h) }
func (h dongHeap) Less(i, j int) bool {
	if h[i].so != h[j].so {
		return h[i].so < h[j].so
	}
	return h[i].stt < h[j].stt
}
func (h dongHeap) Swap(i, j int) { h[i], h[j] = h[j], h[i] }
func (h *dongHeap) Push(x any)   { *h = append(*h, x.(phanTuHang)) }
func (h *dongHeap) Pop() any {
	c := *h
	x := c[len(c)-1]
	*h = c[:len(c)-1]
	return x
}

type nguoiCho struct {
	so        int64
	trangThai string // "cho" | "luot" | "da_mua" | "het_han" | "het_ve"
	han       float64
	cho       []func(M) // các yêu cầu cho_luot đang giữ
}

type hanLuot struct {
	han float64
	ten string
}

// ------------------------------------------------------------ máy chủ

type MayChuPhongCho struct {
	*MayChuCoSo
	khoaPC                                 sync.Mutex   // bảo vệ phòng chờ (danh sách người, hàng, thùng token)
	maDon                                  atomic.Int64 // mã đơn tăng mãi, kể cả qua các lần đặt lại
	nguoi                                  map[string]*nguoiCho
	hang                                   dongHeap
	stt                                    int64
	coLuot                                 []hanLuot // theo thứ tự cấp lượt – để thu hồi lượt quá hạn
	dangPhucVu, dangCho, daCapLuot, hetHan int64
	token, tToken                          float64
	hetVe                                  bool
	dangXuLy, xuLyMax, quaTai, soDon       int64
	lichSu                                 []M
	luat                                   *luatPhongCho
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuPhongCho, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuPhongCho{MayChuCoSo: co, nguoi: map[string]*nguoiCho{}}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại

func (m *MayChuPhongCho) kiemThamSoNV(ts TS) error {
	lay := func(k string, md float64) float64 {
		if v, co := ts.Lay(k); co {
			return SoThuc(v)
		}
		return md
	}
	if lay("tong_ve", 1) < 1 || lay("ve_toi_da", 1) < 1 || lay("ve_toi_da", 1) > 10 || lay("gia_ve", 0) < 0 {
		return fmt.Errorf("tổng vé ≥ 1, vé tối đa 1–10, giá ≥ 0")
	}
	if p := lay("phong_cho", 0); p != 0 && p != 1 {
		return fmt.Errorf("phong_cho = 0 hoặc 1")
	}
	if t := lay("toc_do", 1); !(t > 0 && t <= 100_000) || lay("dung_luong", 1) < 1 {
		return fmt.Errorf("tốc độ cấp lượt 0–100000 người/s, dung lượng thùng ≥ 1")
	}
	if lay("luot_s", 1) <= 0 || lay("gioi_han_xu_ly", 1) < 1 {
		return fmt.Errorf("thời hạn lượt > 0, giới hạn xử lý ≥ 1")
	}
	if v, co := ts.Lay("ten_su_kien"); co {
		if n := utf8.RuneCountInString(StrPy(v)); n < 1 || n > 40 {
			return fmt.Errorf("tên sự kiện 1–40 ký tự")
		}
	}
	return nil
}

func (m *MayChuPhongCho) khoiTao() {
	m.Kho.DatO("kho", khoVe{con: m.TSInt("tong_ve")})
	m.Kho.DatO("hang", hangO{})
	luat := &luatPhongCho{tongVe: m.TSInt("tong_ve"), luot: map[string]bool{}}
	m.DatSo(MoiSoKiemToan(TS{{"ban_qua", "vượt vé"}, {"khong_luot", "sai lượt"}, {"dao_thu_tu", "đảo số"}}, luat))
	m.khoaPC.Lock()
	var cu []func(M)
	for _, ng := range m.nguoi {
		cu = append(cu, ng.cho...)
	}
	m.nguoi, m.hang, m.stt, m.coLuot = map[string]*nguoiCho{}, nil, 0, nil
	m.dangPhucVu, m.dangCho, m.daCapLuot, m.hetHan = 0, 0, 0, 0
	m.token, m.tToken = float64(m.TSInt("dung_luong")), GioDonDieu()
	m.hetVe = false
	m.dangXuLy, m.xuLyMax, m.quaTai, m.soDon = 0, 0, 0, 0
	m.lichSu, m.luat = nil, luat
	m.khoaPC.Unlock()
	for _, cb := range cu { // ai đang "chờ dài" từ phiên trước: trả lời ngay
		cb(M{"ok": false, "ma_loi": "dat_lai", "ly_do": "phiên bán vé được đặt lại"})
	}
}

// DatTocDo – đổi tốc độ cấp lượt ngay khi đang chạy (không đặt lại phiên).
func (m *MayChuPhongCho) DatTocDo(tocDo float64) error {
	if err := m.kiemThamSoNV(TS{{"toc_do", tocDo}}); err != nil {
		return err
	}
	m.muTS.Lock()
	m.thamSo = m.thamSo.Dat("toc_do", tocDo)
	m.muTS.Unlock()
	m.ThongBao(fmt.Sprintf("Tốc độ cấp lượt: %s người/s", FmtG(tocDo)))
	m.BaoDoi()
	return nil
}

// ------------------------------------------------------------ yêu cầu

func (m *MayChuPhongCho) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	switch {
	case tin["loai"] == "mua":
		m.mua(nguoi, tin, traLoi)
	case m.TSInt("phong_cho") == 0:
		traLoi(M{"ok": false, "ma_loi": "pc_tat", "ly_do": "phòng chờ đang tắt – mua trực tiếp"})
	case tin["loai"] == "xep_hang":
		m.xepHang(nguoi, traLoi)
	default:
		m.choLuot(nguoi, traLoi)
	}
}

// viTri – số người phía trước và thời gian chờ ước tính (gọi dưới khoaPC).
func (m *MayChuPhongCho) viTri(ng *nguoiCho) M {
	truoc := int64(0)
	if ng.trangThai == "cho" {
		truoc = max(0, ng.so-m.dangPhucVu-1)
	}
	return M{"so": ng.so, "truoc": truoc, "cho_s": math.RoundToEven(float64(truoc+1)/m.TSFloat("toc_do")*10) / 10,
		"trang_thai": ng.trangThai}
}

func (m *MayChuPhongCho) xepHang(nguoi string, traLoi func(M)) {
	var kq M
	m.khoaPC.Lock()
	if m.hetVe {
		kq = kqHetVe()
	} else if ng := m.nguoi[nguoi]; ng != nil { // đã trong hàng (tải lại trang, tab thứ hai): giữ chỗ cũ
		kq = m.viTri(ng)
		kq["ok"], kq["da_co"] = true, true
	}
	m.khoaPC.Unlock()
	if kq != nil {
		traLoi(kq)
		return
	}
	xuLy := func(doc M) (M, M) { // phát số thứ tự: đọc – cộng 1 – ghi
		so := doc["hang"].(hangO).cuoi + 1
		return M{"ok": true, "so": so}, M{"hang": hangO{so}}
	}
	khiGhi := func(kq M) { // trong vùng găng: đưa vào hàng, ghi sổ
		so := kq["so"].(int64)
		m.khoaPC.Lock()
		ng := m.nguoi[nguoi]
		moi := ng == nil
		switch {
		case moi && m.hetVe: // vừa hết vé trong lúc xếp hàng: cho_luot báo hết vé ngay
			ng = &nguoiCho{so: so, trangThai: "het_ve"}
			m.nguoi[nguoi] = ng
		case moi:
			ng = &nguoiCho{so: so, trangThai: "cho"}
			m.nguoi[nguoi] = ng
			m.stt++
			heap.Push(&m.hang, phanTuHang{so, m.stt, nguoi})
			m.dangCho++
		default:
			kq["da_co"] = true
		}
		for x, v := range m.viTri(ng) {
			kq[x] = v
		}
		m.khoaPC.Unlock()
		if moi {
			m.So().Ghi(gdPhongCho{loai: "xep", nguoi: nguoi, so: so})
		}
		m.BaoDoi()
	}
	m.Kho.ThucHien([]string{"hang"}, xuLy, traLoi, khiGhi)
}

// choLuot – chờ dài: chưa tới lượt thì GIỮ traLoi, goroutine nền gọi khi cấp lượt / hết vé.
func (m *MayChuPhongCho) choLuot(nguoi string, traLoi func(M)) {
	var kq M
	m.khoaPC.Lock()
	ng := m.nguoi[nguoi]
	switch {
	case ng == nil:
		kq = M{"ok": false, "ma_loi": "chua_xep_hang", "ly_do": "chưa vào hàng chờ"}
	case ng.trangThai == "cho":
		ng.cho = append(ng.cho, traLoi)
		m.khoaPC.Unlock()
		return
	case ng.trangThai == "luot":
		kq = M{"ok": true, "luot": true, "so": ng.so, "han_ms": max(0, LamTron((ng.han-GioDonDieu())*1000))}
	case ng.trangThai == "da_mua":
		kq = M{"ok": false, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"}
	case ng.trangThai == "het_han":
		kq = M{"ok": false, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"}
	default:
		kq = kqHetVe()
	}
	m.khoaPC.Unlock()
	traLoi(kq)
}

func (m *MayChuPhongCho) mua(nguoi string, tin M, traLoi func(M)) {
	toiDa := m.TSInt("ve_toi_da")
	soVe, la := tin["so_ve"].(int64)
	if !la || soVe < 1 || soVe > toiDa {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": fmt.Sprintf("số vé 1–%d", toiDa)})
		return
	}
	m.khoaPC.Lock() // giới hạn số yêu cầu mua đang xử lý cùng lúc
	quaTai := m.dangXuLy >= m.TSInt("gioi_han_xu_ly")
	if quaTai {
		m.quaTai++
	} else {
		m.dangXuLy++
		m.xuLyMax = max(m.xuLyMax, m.dangXuLy)
	}
	m.khoaPC.Unlock()
	if quaTai {
		traLoi(M{"ok": false, "ma_loi": "qua_tai", "ly_do": "hệ thống quá tải, vui lòng thử lại"})
		return
	}
	pc, gia := m.TSInt("phong_cho") != 0, m.TSInt("gia_ve")
	oLuot := "luot:" + nguoi     // ô lượt của người này (tạo khi được cấp lượt)
	xuLy := func(doc M) (M, M) { // kiểm tra lượt + số vé còn; trừ kho VÀ tiêu lượt
		kh := doc["kho"].(khoVe)
		if pc {
			l := doc[oLuot].(luotO)
			if !l.con {
				return M{"ok": false, "ma_loi": "da_mua", "ly_do": "lượt này đã dùng để mua"}, nil
			}
			if GioDonDieu() > l.han {
				return M{"ok": false, "ma_loi": "het_luot", "ly_do": "lượt mua đã hết hạn"}, nil
			}
		}
		if kh.con <= 0 {
			kq := kqHetVe()
			kq["con"] = int64(0)
			return kq, nil
		}
		if kh.con < soVe {
			return M{"ok": false, "ma_loi": "het_ve", "ly_do": fmt.Sprintf("chỉ còn %d vé", kh.con), "con": kh.con}, nil
		}
		ghi := M{"kho": khoVe{kh.con - soVe, kh.daBan + soVe}}
		if pc {
			l := doc[oLuot].(luotO)
			l.con = false
			ghi[oLuot] = l
		}
		return M{"ok": true, "so_ve": soVe, "thanh_tien": soVe * gia, "con": kh.con - soVe}, ghi
	}
	khiGhi := func(kq M) { // trong vùng găng: tiêu lượt, ghi sổ
		kq["ma_don"] = m.maDon.Add(1)
		m.khoaPC.Lock()
		if ng := m.nguoi[nguoi]; pc && ng != nil {
			ng.trangThai = "da_mua"
		}
		m.soDon++
		m.lichSu = append([]M{{"ma": kq["ma_don"], "gio": time.Now().Format("15:04:05"), "nguoi": nguoi, "so_ve": soVe}},
			m.lichSu...)
		if len(m.lichSu) > 30 {
			m.lichSu = m.lichSu[:30]
		}
		m.khoaPC.Unlock()
		m.So().Ghi(gdPhongCho{loai: "mua", nguoi: nguoi, soVe: soVe, canLuot: pc})
		m.BaoDoi()
	}
	tra := func(kq M) {
		m.khoaPC.Lock()
		m.dangXuLy--
		m.khoaPC.Unlock()
		if kq["ma_loi"] == "khong_co" { // chưa có ô lượt: chưa được cấp lượt
			kq = M{"ok": false, "ma_loi": "khong_luot", "ly_do": "chưa tới lượt – hãy vào hàng chờ"}
		}
		traLoi(kq)
	}
	cac := []string{"kho"}
	if pc {
		cac = append(cac, oLuot)
	}
	m.Kho.ThucHien(cac, xuLy, tra, khiGhi)
}

// ------------------------------------------------------------ goroutine nền (100 ms): thùng token

type traSau struct {
	cb func(M)
	kq M
}

func (m *MayChuPhongCho) dinhKy(bayGio float64) {
	kh, co := m.Kho.Doc("kho").(khoVe)
	if !co {
		return // đang đặt lại
	}
	tocDo, dungLuong, luotS := m.TSFloat("toc_do"), float64(m.TSInt("dung_luong")), m.TSFloat("luot_s")
	pc := m.TSInt("phong_cho") != 0
	var tha []traSau
	baoHet := false
	m.khoaPC.Lock()
	for len(m.coLuot) > 0 && m.coLuot[0].han <= bayGio { // thu hồi lượt quá hạn
		x := m.coLuot[0]
		m.coLuot = m.coLuot[1:]
		if ng := m.nguoi[x.ten]; ng != nil && ng.trangThai == "luot" && ng.han == x.han {
			ng.trangThai = "het_han"
			m.hetHan++
		}
	}
	switch {
	case kh.con <= 0 && !m.hetVe: // hết vé: báo mọi người đang chờ, đóng hàng
		m.hetVe, baoHet = true, true
		for _, ng := range m.nguoi {
			if ng.trangThai == "cho" {
				ng.trangThai = "het_ve"
				for _, cb := range ng.cho {
					tha = append(tha, traSau{cb, kqHetVe()})
				}
				ng.cho = nil
			}
		}
		m.hang, m.dangCho = nil, 0
	case pc && !m.hetVe:
		m.token = math.Min(dungLuong, m.token+tocDo*(bayGio-m.tToken))
		for m.hang.Len() > 0 && m.token >= 1 {
			p := heap.Pop(&m.hang).(phanTuHang)
			ng := m.nguoi[p.ten]
			if ng == nil || ng.trangThai != "cho" || ng.so != p.so {
				continue
			}
			m.token--
			m.dangCho--
			m.So().Ghi(gdPhongCho{loai: "luot", nguoi: p.ten}) // sổ biết lượt TRƯỚC khi người đó thấy lượt
			ng.trangThai, ng.han = "luot", bayGio+luotS
			m.Kho.DatO("luot:"+p.ten, luotO{true, ng.han}) // ô mới: chưa ai chạm tới
			m.coLuot = append(m.coLuot, hanLuot{ng.han, p.ten})
			m.dangPhucVu = max(m.dangPhucVu, p.so)
			m.daCapLuot++
			kq := M{"ok": true, "luot": true, "so": p.so, "han_ms": LamTron(luotS * 1000)}
			for _, cb := range ng.cho {
				tha = append(tha, traSau{cb, kq})
			}
			ng.cho = nil
		}
	}
	m.tToken = bayGio
	m.khoaPC.Unlock()
	if baoHet {
		m.ThongBao(fmt.Sprintf("Đã bán hết %d vé", m.TSInt("tong_ve")))
	}
	for _, t := range tha {
		t.cb(t.kq)
	}
	if len(tha) > 0 {
		m.BaoDoi()
	}
}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuPhongCho) trangThai() M {
	kh, _ := m.Kho.Doc("kho").(khoVe)
	h, _ := m.Kho.Doc("hang").(hangO)
	ts := m.ThamSo()
	lay := func(k string) any { v, _ := ts.Lay(k); return v }
	m.khoaPC.Lock()
	defer m.khoaPC.Unlock()
	ls := append([]M(nil), m.lichSu...)
	if len(ls) > 10 {
		ls = ls[:10]
	}
	if ls == nil {
		ls = []M{}
	}
	return M{"su_kien": M{"ten": lay("ten_su_kien"), "gia_ve": lay("gia_ve"), "tong_ve": lay("tong_ve"),
		"ve_toi_da": lay("ve_toi_da"), "luot_s": lay("luot_s")},
		"phong_cho": SoNguyen(lay("phong_cho")) != 0, "con_lai": kh.con, "da_ban": kh.daBan,
		"toc_do": lay("toc_do"), "dung_luong": lay("dung_luong"), "token": int64(m.token),
		"dang_phuc_vu": m.dangPhucVu, "so_cap": h.cuoi, "dang_cho": m.dangCho, "da_cap_luot": m.daCapLuot,
		"het_han": m.hetHan, "het_ve": m.hetVe, "dang_xu_ly": m.dangXuLy, "xu_ly_max": m.xuLyMax,
		"gioi_han": lay("gioi_han_xu_ly"), "qua_tai": m.quaTai, "so_don": m.soDon, "ban_so": m.luat.daBan.Load(),
		"lich_su": ls}
}

func (m *MayChuPhongCho) tomTat() string {
	kh, _ := m.Kho.Doc("kho").(khoVe)
	m.khoaPC.Lock()
	luat, soDon := m.luat, m.soDon
	m.khoaPC.Unlock()
	return fmt.Sprintf("Sổ: bán %d/%d vé, %d đơn; kho còn %d", luat.daBan.Load(), m.TSInt("tong_ve"), soDon, kh.con)
}
