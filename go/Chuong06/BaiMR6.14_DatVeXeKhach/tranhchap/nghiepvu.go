package tranhchap

// NGHIỆP VỤ MR6.14 – lõi máy chủ đặt vé xe khách theo chặng + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Chuyến xe qua các trạm Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang (3 chặng). Hai loại xe:
//
//	giuong – xe giường nằm 2 tầng, 40 giường: tầng dưới D01–D20, tầng trên T01–T20 (3 dãy × 7 hàng,
//	         hàng cuối 2 giường);
//	ghe    – xe ghế ngồi 50 chỗ 01–50 (11 hàng × 4 ghế, lối đi ở giữa, hàng cuối 6 ghế liền).
//
// Khách đặt một ĐOẠN [tu, den) (chỉ số trạm, tu < den), nên cùng một giường bán được cho nhiều người nếu
// các đoạn không chồng nhau. Ba thao tác ghi:
//
//	{"loai":"giu_cho","ma_yc":k,"ghe":["D01"],"tu":0,"den":2[,"nguoi":"u17"]}   giữ chỗ có thời hạn (TTL)
//	{"loai":"thanh_toan","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                 chỗ mình đang giữ -> vé
//	{"loai":"nha","ma_yc":k,"ghe":["D01"],"tu":0,"den":2}                        trả lại chỗ đang giữ
//
// Mỗi cặp (ghế, chặng) là MỘT ô của KhoTranhChap, tên ô "D01#1" (chặng 1 = Bảo Lộc → Đà Lạt); giữ 2 giường
// cho đoạn 3 chặng = khóa 6 ô theo thứ tự tên ô (không khóa chết). Luật như MR6.8: giữ được khi MỌI ô đều
// trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình); hết hạn sau ttl_s giây – goroutine nền dọn ô
// quá hạn mỗi 100 ms; thanh toán chỉ nhận ô mình giữ, còn hạn.
//
// Sổ kiểm toán đếm hai sai phạm:
//
//	trung_cho – một (ghế, chặng) được giữ/bán cho người thứ hai khi người khác đang giữ (còn hạn) hoặc đã mua;
//	tt_sai    – thanh toán được chấp nhận cho chỗ không do mình giữ hoặc lượt giữ đã hết hạn.

import (
	"fmt"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"unicode/utf8"
)

const TenMayChu = "Đặt vé xe khách MR6.14"

var LoaiYeuCau = []string{"giu_cho", "thanh_toan", "nha"}

var ThamSoMacDinh = TS{{"ttl_s", 30.0}, {"loai_xe", "giuong"}, {"ghe_nong", "D01"}, {"chuyen", "07:30"},
	{"nha_xe", "Xe khách Cao Nguyên"}}

var (
	Tram     = []string{"Sài Gòn", "Bảo Lộc", "Đà Lạt", "Nha Trang"}
	MaTram   = []string{"SG", "BL", "DL", "NT"}
	SoChang  = len(Tram) - 1
	GiaChang = map[string][]int64{"giuong": {220_000, 130_000, 170_000}, // giá minh họa mỗi chặng, đồng/chỗ
		"ghe": {170_000, 100_000, 130_000}}
	TenXe = map[string]string{"giuong": "Xe giường nằm 40 chỗ", "ghe": "Xe ghế ngồi 50 chỗ"}
)

const ToiDaGhe = 6 // một yêu cầu tối đa 6 chỗ

// ChoXe – một chỗ trên sơ đồ xe.
type ChoXe struct {
	Ma              string
	Tang, Hang, Cot int
}

// BoTri – sơ đồ xe theo thứ tự hiển thị. giuong: tầng 0 = dưới (D), 1 = trên (T); cột 0, 1, 2; hàng 6 chỉ có
// cột 0 và 2. ghe: tầng 0; 6 cột: hàng 0–10 dùng cột 0, 1 | lối đi | 4, 5; hàng cuối (11) đủ 6 cột.
func BoTri(loaiXe string) []ChoXe {
	var ds []ChoXe
	if loaiXe == "giuong" {
		for tang, chu := range []string{"D", "T"} {
			n := 0
			for hang := 0; hang < 7; hang++ {
				cot := []int{0, 1, 2}
				if hang == 6 {
					cot = []int{0, 2}
				}
				for _, c := range cot {
					n++
					ds = append(ds, ChoXe{fmt.Sprintf("%s%02d", chu, n), tang, hang, c})
				}
			}
		}
		return ds
	}
	n := 0
	for hang := 0; hang < 12; hang++ {
		cot := []int{0, 1, 4, 5}
		if hang == 11 {
			cot = []int{0, 1, 2, 3, 4, 5}
		}
		for _, c := range cot {
			n++
			ds = append(ds, ChoXe{fmt.Sprintf("%02d", n), 0, hang, c})
		}
	}
	return ds
}

func DSMaGhe(loaiXe string) []string {
	var ds []string
	for _, x := range BoTri(loaiXe) {
		ds = append(ds, x.Ma)
	}
	return ds
}

func TenChang(k int) string      { return Tram[k] + "–" + Tram[k+1] }
func TenDoan(tu, den int) string { return Tram[tu] + " → " + Tram[den] }

func tenO(o string) string {
	g, k, _ := strings.Cut(o, "#")
	n, _ := strconv.Atoi(k)
	return g + " (" + TenChang(n) + ")"
}

func oCua(ghe []string, tu, den int) []string {
	var ds []string
	for _, g := range ghe {
		for k := tu; k < den; k++ {
			ds = append(ds, fmt.Sprintf("%s#%d", g, k))
		}
	}
	return ds
}

// Cho – giá trị của một ô (chỗ, chặng).
type Cho struct {
	TT          string // "trong" | "giu" | "ban"
	Nguoi       string
	HetHan      float64 // theo GioDonDieu() (khi TT = "giu")
	MaVe        string
	NguoiHetHan string // ai giữ ô lần cuối trước khi hết hạn (để báo đúng lý do "het_han")
}

func choTrong(lanCuoi string) Cho { return Cho{TT: "trong", NguoiHetHan: lanCuoi} }

// ------------------------------------------------------------ sổ kiểm toán

type gdDatVeXe struct {
	loai   string
	o      []string
	nguoi  string
	luc    float64
	hetHan float64
	soVe   int64
}

type oSo struct {
	tt     string
	nguoi  string
	hetHan float64
}

type luatDatVeXe struct {
	o    map[string]oSo // theo sổ: ô -> (giữ/bán, người, hạn)
	soVe atomic.Int64   // số vé (ghế × đoạn) đã bán theo sổ
}

func (l *luatDatVeXe) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdDatVeXe)
	if gd.loai == "ban" {
		l.soVe.Add(gd.soVe)
	}
	for _, o := range gd.o {
		cu, co := l.o[o]
		switch gd.loai {
		case "giu":
			if co && cu.tt == "ban" {
				so.SaiPham("trung_cho", fmt.Sprintf("%s giữ cho %s khi đã bán cho %s", tenO(o), gd.nguoi, cu.nguoi))
				continue
			}
			if co && cu.nguoi != gd.nguoi && gd.luc < cu.hetHan {
				so.SaiPham("trung_cho", fmt.Sprintf("%s giữ cho %s khi %s đang giữ", tenO(o), gd.nguoi, cu.nguoi))
			}
			l.o[o] = oSo{"giu", gd.nguoi, gd.hetHan}
		case "ban":
			if co && cu.tt == "ban" {
				so.SaiPham("trung_cho", fmt.Sprintf("%s bán cho %s khi đã bán cho %s", tenO(o), gd.nguoi, cu.nguoi))
				continue
			}
			if !co || cu.nguoi != gd.nguoi {
				so.SaiPham("tt_sai", fmt.Sprintf("%s: %s trả tiền chỗ không do mình giữ", tenO(o), gd.nguoi))
			} else if gd.luc >= cu.hetHan {
				so.SaiPham("tt_sai", fmt.Sprintf("%s: %s trả tiền khi đã hết hạn giữ", tenO(o), gd.nguoi))
			}
			l.o[o] = oSo{"ban", gd.nguoi, 0}
		default: // nhả / hết hạn
			if co && cu.tt == "giu" && cu.nguoi == gd.nguoi {
				delete(l.o, o)
			}
		}
	}
}

func (l *luatDatVeXe) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// ------------------------------------------------------------ máy chủ

type MayChuDatVeXe struct {
	*MayChuCoSo
	mu        sync.Mutex
	dangDon   map[string]bool // ô quá hạn đang được goroutine nền dọn
	nguoiThat map[string]bool // tên người dùng thật (in nhật ký cho họ)
	luat      *luatDatVeXe
	boTri     []ChoXe
	dsGhe     []string
	gheHopLe  map[string]bool
	tatCaO    []string
	demGhi    atomic.Int64 // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
	phien     atomic.Int64
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuDatVeXe, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuDatVeXe{MayChuCoSo: co, dangDon: map[string]bool{}, nguoiThat: map[string]bool{}}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại

func (m *MayChuDatVeXe) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("ttl_s"); co {
		if t := SoThuc(v); t < 1 || t > 600 {
			return fmt.Errorf("thời hạn giữ chỗ 1–600 s")
		}
	}
	loai := m.TSStr("loai_xe")
	if v, co := ts.Lay("loai_xe"); co {
		loai = StrPy(v)
	}
	if _, co := GiaChang[loai]; !co {
		return fmt.Errorf("loại xe: giuong hoặc ghe")
	}
	if v, co := ts.Lay("ghe_nong"); co {
		if s := StrPy(v); s != "" && !coTrongDS(DSMaGhe(loai), s) {
			return fmt.Errorf("ghế nóng phải là mã chỗ của %s hoặc rỗng", strings.ToLower(TenXe[loai]))
		}
	}
	if v, co := ts.Lay("nha_xe"); co {
		if n := utf8.RuneCountInString(StrPy(v)); n < 1 || n > 40 {
			return fmt.Errorf("tên nhà xe 1–40 ký tự")
		}
	}
	return nil
}

func coTrongDS(ds []string, x string) bool {
	for _, s := range ds {
		if s == x {
			return true
		}
	}
	return false
}

// GheNong – ghế nóng đang dùng: tham số ghe_nong nếu là chỗ của loại xe hiện tại, không thì chỗ đầu (D01 / 01)
// – đổi loại xe mà không đặt ghe_nong thì ghế nóng tự về chỗ đầu; rỗng = không có ghế nóng.
func (m *MayChuDatVeXe) GheNong() string {
	g := m.TSStr("ghe_nong")
	m.mu.Lock()
	defer m.mu.Unlock()
	if g == "" || m.gheHopLe[g] {
		return g
	}
	return m.dsGhe[0]
}

func (m *MayChuDatVeXe) khoiTao() {
	boTri := BoTri(m.TSStr("loai_xe"))
	var ds, tatCa []string
	hopLe := map[string]bool{}
	for _, x := range boTri {
		ds = append(ds, x.Ma)
		hopLe[x.Ma] = true
		for k := 0; k < SoChang; k++ {
			tatCa = append(tatCa, fmt.Sprintf("%s#%d", x.Ma, k))
		}
	}
	for _, o := range tatCa {
		m.Kho.DatO(o, choTrong(""))
	}
	luat := &luatDatVeXe{o: map[string]oSo{}}
	m.DatSo(MoiSoKiemToan(TS{{"trung_cho", "trùng chỗ"}, {"tt_sai", "thanh toán sai"}}, luat))
	m.mu.Lock()
	m.dangDon, m.nguoiThat, m.luat = map[string]bool{}, map[string]bool{}, luat
	m.boTri, m.dsGhe, m.gheHopLe, m.tatCaO = boTri, ds, hopLe, tatCa
	m.mu.Unlock()
}

// ------------------------------------------------------------ giữ chỗ / thanh toán / nhả

// docYeuCau – (ghế, tu, den) hợp lệ, hoặc lý do.
func (m *MayChuDatVeXe) docYeuCau(tin M) ([]string, int, int, string) {
	m.mu.Lock()
	hopLe := m.gheHopLe
	m.mu.Unlock()
	loai := m.TSStr("loai_xe")
	x, la := tin["ghe"].([]any)
	loiGhe := fmt.Sprintf("ghế: danh sách 1–%d mã chỗ của %s", ToiDaGhe, strings.ToLower(TenXe[loai]))
	if !la || len(x) < 1 || len(x) > ToiDaGhe {
		return nil, 0, 0, loiGhe
	}
	gap := map[string]bool{}
	var ghe []string
	for _, p := range x {
		s, la := p.(string)
		if !la || !hopLe[s] || gap[s] {
			return nil, 0, 0, loiGhe
		}
		gap[s] = true
		ghe = append(ghe, s)
	}
	tu, la1 := soNguyenThat(tin["tu"])
	den, la2 := soNguyenThat(tin["den"])
	if !la1 || !la2 || tu < 0 || tu >= den || den > SoChang {
		return nil, 0, 0, fmt.Sprintf("đoạn đường: 0 ≤ tu < den ≤ %d (chỉ số trạm)", SoChang)
	}
	return ghe, tu, den, ""
}

// soNguyenThat – số nguyên JSON (như Python: không nhận số thực, true/false, chuỗi).
func soNguyenThat(v any) (int, bool) {
	x, la := v.(int64)
	return int(x), la
}

func (m *MayChuDatVeXe) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	ghe, tu, den, loi := m.docYeuCau(tin)
	if loi != "" {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": loi})
		return
	}
	oYC := oCua(ghe, tu, den)
	that := k.SoNguoi == 0
	if that {
		m.mu.Lock()
		m.nguoiThat[nguoi] = true
		m.mu.Unlock()
	}
	ttl, chuyen := m.TSFloat("ttl_s"), m.TSStr("chuyen")
	var gia int64
	for _, x := range GiaChang[m.TSStr("loai_xe")][tu:den] {
		gia += x
	}
	gia *= int64(len(ghe))
	chung := func(kq M) M {
		kq["ghe"], kq["tu"], kq["den"], kq["nguoi"] = ghe, int64(tu), int64(den), nguoi
		return kq
	}
	tra := func(kq M) { // bỏ các trường nội bộ ("_…") trước khi gửi
		ra := M{}
		for x, v := range kq {
			if !strings.HasPrefix(x, "_") {
				ra[x] = v
			}
		}
		traLoi(ra)
	}
	switch tin["loai"] {
	case "giu_cho":
		xuLy := func(doc M) (M, M) { // hàm THUẦN: mọi ô phải trống / quá hạn / của mình
			luc := GioDonDieu()
			var moi []string
			conLai := ttl
			for _, o := range oYC {
				v := doc[o].(Cho)
				g, kc, _ := strings.Cut(o, "#")
				kn, _ := strconv.Atoi(kc)
				if v.TT == "ban" {
					return M{"ok": false, "ma_loi": "da_ban", "ly_do": "chỗ " + g + " chặng " + TenChang(kn) + " đã bán",
						"ghe_ban": []string{g}, "tu": int64(tu), "den": int64(den)}, nil
				}
				if v.TT == "giu" && luc < v.HetHan {
					if v.Nguoi != nguoi {
						return M{"ok": false, "ma_loi": "dang_giu", "ly_do": "chỗ " + g + " chặng " + TenChang(kn) +
							" đang có người giữ", "ghe_ban": []string{g}, "tu": int64(tu), "den": int64(den)}, nil
					}
					conLai = min(conLai, v.HetHan-luc)
				} else {
					moi = append(moi, o)
				}
			}
			hetHan := luc + ttl
			var ghi M
			if len(moi) > 0 {
				ghi = M{}
				for _, o := range moi {
					ghi[o] = Cho{TT: "giu", Nguoi: nguoi, HetHan: hetHan}
				}
			}
			return chung(M{"ok": true, "het_han_ms": LamTron(conLai * 1000), "tien": gia,
				"_moi": moi, "_luc": luc, "_het_han": hetHan}), ghi
		}
		m.Kho.ThucHien(oYC, xuLy, tra, func(kq M) { m.daGiu(kq, that) })
	case "thanh_toan":
		xuLy := func(doc M) (M, M) {
			luc := GioDonDieu()
			hetHan, khongGiu := "", "" // ưu tiên báo "hết hạn" nếu có dấu vết lượt giữ của mình
			for _, o := range oYC {
				v := doc[o].(Cho)
				g, _, _ := strings.Cut(o, "#")
				if v.TT == "giu" && v.Nguoi == nguoi && luc < v.HetHan {
					continue
				}
				if (v.TT == "giu" && v.Nguoi == nguoi) || (v.TT == "trong" && v.NguoiHetHan == nguoi) {
					if hetHan == "" {
						hetHan = g
					}
				} else if khongGiu == "" {
					khongGiu = g
				}
			}
			if hetHan != "" {
				return M{"ok": false, "ma_loi": "het_han", "ly_do": "giữ chỗ " + hetHan + " đã hết hạn"}, nil
			}
			if khongGiu != "" {
				return M{"ok": false, "ma_loi": "khong_giu", "ly_do": "chỗ " + khongGiu + " không do bạn giữ"}, nil
			}
			ma := "XK" + strings.ReplaceAll(chuyen, ":", "")
			var ve []string
			maVe := map[string]string{}
			for _, g := range ghe {
				mv := ma + "-" + g + "-" + MaTram[tu] + MaTram[den]
				ve = append(ve, mv)
				maVe[g] = mv
			}
			ghi := M{}
			for _, o := range oYC {
				g, _, _ := strings.Cut(o, "#")
				ghi[o] = Cho{TT: "ban", Nguoi: nguoi, MaVe: maVe[g]}
			}
			return chung(M{"ok": true, "ma_ve": ve, "tien": gia, "_luc": luc}), ghi
		}
		m.Kho.ThucHien(oYC, xuLy, tra, func(kq M) { m.daBan(kq, that) })
	default:
		xuLy := func(doc M) (M, M) {
			var nha []string
			ghi := M{}
			for _, o := range oYC {
				if v := doc[o].(Cho); v.TT == "giu" && v.Nguoi == nguoi {
					nha = append(nha, o)
					ghi[o] = choTrong("")
				}
			}
			if len(nha) == 0 {
				return M{"ok": false, "ma_loi": "khong_giu", "ly_do": "bạn không giữ chỗ nào trong số này"}, nil
			}
			return chung(M{"ok": true, "_o": nha}), ghi
		}
		m.Kho.ThucHien(oYC, xuLy, tra, func(kq M) { m.daNha(kq, that) })
	}
}

// khiGhi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
// tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
func (m *MayChuDatVeXe) danhSo(kq M) {
	p := m.demGhi.Add(1)
	m.phien.Store(p)
	kq["phien"] = p
}

func doanCua(kq M) ([]string, int, int, string) {
	return kq["ghe"].([]string), int(kq["tu"].(int64)), int(kq["den"].(int64)), kq["nguoi"].(string)
}

func (m *MayChuDatVeXe) daGiu(kq M, that bool) {
	m.danhSo(kq)
	ghe, tu, den, nguoi := doanCua(kq)
	moi, _ := kq["_moi"].([]string)
	m.So().Ghi(gdDatVeXe{loai: "giu", o: moi, nguoi: nguoi, luc: kq["_luc"].(float64), hetHan: kq["_het_han"].(float64)})
	if that {
		m.ThongBao(fmt.Sprintf("%s giữ %s (%s, %s s)", nguoi, strings.Join(ghe, ", "), TenDoan(tu, den), FmtG(m.TSFloat("ttl_s"))))
	}
	m.BaoDoi()
}

func (m *MayChuDatVeXe) daBan(kq M, that bool) {
	m.danhSo(kq)
	ghe, tu, den, nguoi := doanCua(kq)
	m.So().Ghi(gdDatVeXe{loai: "ban", o: oCua(ghe, tu, den), nguoi: nguoi, luc: kq["_luc"].(float64), soVe: int64(len(ghe))})
	if that {
		m.ThongBao(fmt.Sprintf("%s mua %d vé %s %s (%s)", nguoi, len(ghe), strings.Join(ghe, ", "), TenDoan(tu, den),
			Tien(kq["tien"].(int64))))
	}
	m.BaoDoi()
}

func (m *MayChuDatVeXe) daNha(kq M, that bool) {
	m.danhSo(kq)
	ghe, tu, den, nguoi := doanCua(kq)
	m.So().Ghi(gdDatVeXe{loai: "nha", o: kq["_o"].([]string), nguoi: nguoi})
	if that {
		m.ThongBao(fmt.Sprintf("%s nhả %s (%s)", nguoi, strings.Join(ghe, ", "), TenDoan(tu, den)))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ dọn ô quá hạn (goroutine nền, 100 ms)

func (m *MayChuDatVeXe) dinhKy(bayGio float64) {
	m.mu.Lock()
	tatCa := m.tatCaO
	m.mu.Unlock()
	for _, o := range tatCa {
		v, co := m.Kho.Doc(o).(Cho)
		if !co || v.TT != "giu" || bayGio < v.HetHan {
			continue
		}
		m.mu.Lock()
		dang := m.dangDon[o]
		m.dangDon[o] = true
		m.mu.Unlock()
		if dang {
			continue
		}
		o := o
		xuLy := func(doc M) (M, M) { // nhả ô quá hạn cũng là một thao tác ghi
			v := doc[o].(Cho)
			if v.TT != "giu" || GioDonDieu() < v.HetHan {
				return M{"ok": false}, nil // vừa được thanh toán / giữ lại
			}
			return M{"ok": true, "o": []string{o}, "nguoi": v.Nguoi}, M{o: choTrong(v.Nguoi)}
		}
		m.Kho.ThucHien([]string{o}, xuLy, func(M) {
			m.mu.Lock()
			delete(m.dangDon, o)
			m.mu.Unlock()
		}, m.daHetHan)
	}
}

func (m *MayChuDatVeXe) daHetHan(kq M) {
	m.danhSo(kq)
	o, nguoi := kq["o"].([]string), kq["nguoi"].(string)
	m.So().Ghi(gdDatVeXe{loai: "het_han", o: o, nguoi: nguoi})
	m.mu.Lock()
	that := m.nguoiThat[nguoi]
	m.mu.Unlock()
	if that {
		m.ThongBao(fmt.Sprintf("Hết hạn giữ %s (%s), chỗ trống lại", tenO(o[0]), nguoi))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ trạng thái gửi client

// trangThai – so_do[i] = chuỗi SoChang ký tự cho chỗ ghe[i]: '.' trống, 'g' đang giữ, 'b' đã bán (từng chặng);
// chu / con_lai_ms theo tên ô "D01#1".
func (m *MayChuDatVeXe) trangThai() M {
	phien := m.phien.Load() // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
	bayGio := GioDonDieu()
	m.mu.Lock()
	boTri, dsGhe := m.boTri, m.dsGhe
	m.mu.Unlock()
	var soDo []string
	chu, conLai := M{}, M{}
	var trong, giu, ban int64
	for _, g := range dsGhe {
		var dong strings.Builder
		for k := 0; k < SoChang; k++ {
			o := fmt.Sprintf("%s#%d", g, k)
			v, _ := m.Kho.Doc(o).(Cho)
			switch {
			case v.TT == "giu" && bayGio < v.HetHan:
				dong.WriteByte('g')
				chu[o] = v.Nguoi
				conLai[o] = LamTron((v.HetHan - bayGio) * 1000)
				giu++
			case v.TT == "ban":
				dong.WriteByte('b')
				chu[o] = v.Nguoi
				ban++
			default:
				dong.WriteByte('.')
				trong++
			}
		}
		soDo = append(soDo, dong.String())
	}
	var bt [][]int64
	for _, x := range boTri {
		bt = append(bt, []int64{int64(x.Tang), int64(x.Hang), int64(x.Cot)})
	}
	ts := m.ThamSo()
	lay := func(k string) any { v, _ := ts.Lay(k); return v }
	loai := Chuoi(lay("loai_xe"))
	return M{"phien": phien, "nha_xe": lay("nha_xe"), "chuyen": lay("chuyen"), "loai_xe": loai, "ten_xe": TenXe[loai],
		"tram": Tram, "gia_chang": GiaChang[loai], "ttl_s": lay("ttl_s"), "ghe_nong": m.GheNong(), "ghe": dsGhe,
		"bo_tri": bt, "so_do": soDo, "chu": chu, "con_lai_ms": conLai, "dem": M{"trong": trong, "giu": giu, "ban": ban}}
}

func (m *MayChuDatVeXe) tomTat() string {
	var ban, giu int
	bayGio := GioDonDieu()
	m.mu.Lock()
	tatCa, luat := m.tatCaO, m.luat
	m.mu.Unlock()
	for _, o := range tatCa {
		v, _ := m.Kho.Doc(o).(Cho)
		if v.TT == "ban" {
			ban++
		}
		if v.TT == "giu" && bayGio < v.HetHan {
			giu++
		}
	}
	return fmt.Sprintf("Bán %d/%d chỗ-chặng, giữ %d, sổ %d vé", ban, len(tatCa), giu, luat.soVe.Load())
}
