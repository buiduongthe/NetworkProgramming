package tranhchap

// NGHIỆP VỤ MR6.8 – lõi máy chủ đặt vé xem phim + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Phòng chiếu 8 hàng (A–H) × 12 ghế; n người cùng chọn ghế. Ba thao tác ghi:
//
//	{"loai":"giu_cho","ma_yc":k,"ghe":["E7","E8"][,"nguoi":"u17"]}    giữ chỗ có thời hạn (TTL)
//	{"loai":"thanh_toan","ma_yc":k,"ghe":["E7","E8"]}                  ghế mình đang giữ, còn hạn -> vé
//	{"loai":"nha","ma_yc":k,"ghe":["E7"]}                              trả lại ghế đang giữ
//
// Luật: giữ được khi MỌI ghế trong yêu cầu đều trống (hoặc lượt giữ cũ đã hết hạn, hoặc đã là của mình);
// lượt giữ hết hạn sau ttl_s giây (mặc định 30 s) – goroutine nền của máy chủ (100 ms/lần) "dọn" ghế quá
// hạn, và một lượt giữ quá hạn cũng bị coi là trống ngay cả khi chưa kịp dọn; thanh toán chỉ nhận ghế
// mình đang giữ và còn hạn (quá hạn -> "het_han"). Mỗi ghế là MỘT ô của KhoTranhChap (struct Ghe) nên
// khóa theo từng ghế: giữ nhiều ghế = khóa nhiều ô theo thứ tự tên ô (không khóa chết).
//
// Sổ kiểm toán đếm hai sai phạm:
//
//	trung_ghe – một ghế được giữ/bán cho người thứ hai khi đang được người khác giữ (còn hạn) hoặc đã bán;
//	tt_sai    – thanh toán được chấp nhận cho ghế không do mình giữ hoặc lượt giữ đã hết hạn.

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"unicode/utf8"
)

const TenMayChu = "Đặt vé xem phim MR6.8"

var LoaiYeuCau = []string{"giu_cho", "thanh_toan", "nha"}

var ThamSoMacDinh = TS{{"ttl_s", 30.0}, {"ghe_nong", "E7"}, {"gia_ve", int64(90_000)}, {"phim", "Hành tinh xanh"},
	{"suat", "19:30"}}

const (
	Hang     = "ABCDEFGH"
	Cot      = 12
	ToiDaGhe = 8 // một yêu cầu tối đa 8 ghế
)

var (
	TatCaGhe []string
	gheHopLe = map[string]bool{}
)

func init() {
	for _, r := range Hang {
		for c := 1; c <= Cot; c++ {
			g := fmt.Sprintf("%c%d", r, c)
			TatCaGhe = append(TatCaGhe, g)
			gheHopLe[g] = true
		}
	}
}

// dsGhe – 1–8 mã ghế hợp lệ, không trùng; nil nếu sai.
func dsGhe(v any) []string {
	x, la := v.([]any)
	if !la || len(x) < 1 || len(x) > ToiDaGhe {
		return nil
	}
	gap := map[string]bool{}
	var ds []string
	for _, p := range x {
		s, la := p.(string)
		if !la || !gheHopLe[s] || gap[s] {
			return nil
		}
		gap[s] = true
		ds = append(ds, s)
	}
	return ds
}

// Ghe – giá trị của một ô ghế.
type Ghe struct {
	TT          string // "trong" | "giu" | "ban"
	Nguoi       string
	HetHan      float64 // theo GioDonDieu() (khi TT = "giu")
	MaVe        string
	NguoiHetHan string // ai giữ ghế lần cuối trước khi hết hạn (để báo đúng lý do "het_han")
}

func gheTrong(lanCuoi string) Ghe { return Ghe{TT: "trong", NguoiHetHan: lanCuoi} }

// ------------------------------------------------------------ sổ kiểm toán

type gdDatVe struct {
	loai   string
	ghe    []string
	nguoi  string
	luc    float64
	hetHan float64
}

type gheSo struct {
	tt     string
	nguoi  string
	hetHan float64
}

type luatDatVe struct {
	ghe  map[string]gheSo // theo sổ: ghế -> (giữ/bán, người, hạn)
	soVe atomic.Int64     // số vé đã bán theo sổ
}

func (l *luatDatVe) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdDatVe)
	for _, g := range gd.ghe {
		cu, co := l.ghe[g]
		switch gd.loai {
		case "giu":
			if co && cu.tt == "ban" {
				so.SaiPham("trung_ghe", fmt.Sprintf("%s giữ cho %s khi đã bán cho %s", g, gd.nguoi, cu.nguoi))
				continue
			}
			if co && cu.nguoi != gd.nguoi && gd.luc < cu.hetHan {
				so.SaiPham("trung_ghe", fmt.Sprintf("%s giữ cho %s khi %s đang giữ", g, gd.nguoi, cu.nguoi))
			}
			l.ghe[g] = gheSo{"giu", gd.nguoi, gd.hetHan}
		case "ban":
			l.soVe.Add(1)
			if co && cu.tt == "ban" {
				so.SaiPham("trung_ghe", fmt.Sprintf("%s bán cho %s khi đã bán cho %s", g, gd.nguoi, cu.nguoi))
				continue
			}
			if !co || cu.nguoi != gd.nguoi {
				so.SaiPham("tt_sai", fmt.Sprintf("%s: %s trả tiền ghế không do mình giữ", g, gd.nguoi))
			} else if gd.luc >= cu.hetHan {
				so.SaiPham("tt_sai", fmt.Sprintf("%s: %s trả tiền khi đã hết hạn giữ", g, gd.nguoi))
			}
			l.ghe[g] = gheSo{"ban", gd.nguoi, 0}
		default: // nhả / hết hạn
			if co && cu.tt == "giu" && cu.nguoi == gd.nguoi {
				delete(l.ghe, g)
			}
		}
	}
}

func (l *luatDatVe) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// ------------------------------------------------------------ máy chủ

type MayChuDatVe struct {
	*MayChuCoSo
	mu        sync.Mutex
	dangDon   map[string]bool // ghế quá hạn đang được goroutine nền dọn
	nguoiThat map[string]bool // tên người dùng thật (in nhật ký cho họ)
	luat      *luatDatVe
	demGhi    atomic.Int64 // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
	phien     atomic.Int64
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuDatVe, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuDatVe{MayChuCoSo: co, dangDon: map[string]bool{}, nguoiThat: map[string]bool{}}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại

func (m *MayChuDatVe) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("ttl_s"); co {
		if t := SoThuc(v); t < 1 || t > 600 {
			return fmt.Errorf("thời hạn giữ chỗ 1–600 s")
		}
	}
	if v, co := ts.Lay("ghe_nong"); co {
		if s := StrPy(v); s != "" && !gheHopLe[s] {
			return fmt.Errorf("ghế nóng phải là mã ghế A1–H12 hoặc rỗng")
		}
	}
	if v, co := ts.Lay("gia_ve"); co && SoNguyen(v) <= 0 {
		return fmt.Errorf("giá vé > 0")
	}
	if v, co := ts.Lay("phim"); co {
		if n := utf8.RuneCountInString(StrPy(v)); n < 1 || n > 40 {
			return fmt.Errorf("tên phim 1–40 ký tự")
		}
	}
	return nil
}

func (m *MayChuDatVe) khoiTao() {
	for _, g := range TatCaGhe {
		m.Kho.DatO(g, gheTrong(""))
	}
	luat := &luatDatVe{ghe: map[string]gheSo{}}
	m.DatSo(MoiSoKiemToan(TS{{"trung_ghe", "ghế trùng"}, {"tt_sai", "thanh toán sai"}}, luat))
	m.mu.Lock()
	m.dangDon, m.nguoiThat, m.luat = map[string]bool{}, map[string]bool{}, luat
	m.mu.Unlock()
}

// ------------------------------------------------------------ giữ chỗ / thanh toán / nhả

func (m *MayChuDatVe) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	ghe := dsGhe(tin["ghe"])
	if ghe == nil {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": fmt.Sprintf("ghế: danh sách 1–%d mã A1–H12", ToiDaGhe)})
		return
	}
	that := k.SoNguoi == 0
	if that {
		m.mu.Lock()
		m.nguoiThat[nguoi] = true
		m.mu.Unlock()
	}
	ttl, gia, suat := m.TSFloat("ttl_s"), m.TSInt("gia_ve"), m.TSStr("suat")
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
		xuLy := func(doc M) (M, M) { // hàm THUẦN: mọi ghế phải trống / quá hạn / của mình
			luc := GioDonDieu()
			var moi []string
			conLai := ttl
			for _, g := range ghe {
				v := doc[g].(Ghe)
				if v.TT == "ban" {
					return M{"ok": false, "ma_loi": "da_ban", "ly_do": "ghế " + g + " đã bán", "ghe_ban": []string{g}}, nil
				}
				if v.TT == "giu" && luc < v.HetHan {
					if v.Nguoi != nguoi {
						return M{"ok": false, "ma_loi": "dang_giu", "ly_do": "ghế " + g + " đang có người giữ",
							"ghe_ban": []string{g}}, nil
					}
					conLai = min(conLai, v.HetHan-luc)
				} else {
					moi = append(moi, g)
				}
			}
			hetHan := luc + ttl
			ghi := M{}
			for _, g := range moi {
				ghi[g] = Ghe{TT: "giu", Nguoi: nguoi, HetHan: hetHan}
			}
			if moi == nil {
				moi = []string{}
			}
			return M{"ok": true, "ghe": ghe, "moi": moi, "het_han_ms": LamTron(conLai * 1000), "nguoi": nguoi,
				"_luc": luc, "_het_han": hetHan}, ghi
		}
		m.Kho.ThucHien(ghe, xuLy, tra, func(kq M) { m.daGiu(kq, that) })
	case "thanh_toan":
		xuLy := func(doc M) (M, M) {
			luc := GioDonDieu()
			for _, g := range ghe {
				v := doc[g].(Ghe)
				if v.TT == "giu" && v.Nguoi == nguoi && luc < v.HetHan {
					continue
				}
				if (v.TT == "giu" && v.Nguoi == nguoi) || (v.TT == "trong" && v.NguoiHetHan == nguoi) {
					return M{"ok": false, "ma_loi": "het_han", "ly_do": "giữ chỗ " + g + " đã hết hạn"}, nil
				}
				return M{"ok": false, "ma_loi": "khong_giu", "ly_do": "ghế " + g + " không do bạn giữ"}, nil
			}
			var ve []string
			ghi := M{}
			for _, g := range ghe {
				mv := "S" + strings.ReplaceAll(suat, ":", "") + "-" + g
				ve = append(ve, mv)
				ghi[g] = Ghe{TT: "ban", Nguoi: nguoi, MaVe: mv}
			}
			return M{"ok": true, "ghe": ghe, "ma_ve": ve, "tien": gia * int64(len(ghe)), "nguoi": nguoi, "_luc": luc}, ghi
		}
		m.Kho.ThucHien(ghe, xuLy, tra, func(kq M) { m.daBan(kq, that) })
	default:
		xuLy := func(doc M) (M, M) {
			var nha []string
			ghi := M{}
			for _, g := range ghe {
				if v := doc[g].(Ghe); v.TT == "giu" && v.Nguoi == nguoi {
					nha = append(nha, g)
					ghi[g] = gheTrong("")
				}
			}
			if len(nha) == 0 {
				return M{"ok": false, "ma_loi": "khong_giu", "ly_do": "bạn không giữ ghế nào trong số này"}, nil
			}
			return M{"ok": true, "ghe": nha, "nguoi": nguoi}, ghi
		}
		m.Kho.ThucHien(ghe, xuLy, tra, func(kq M) { m.daNha(kq, that) })
	}
}

// khiGhi: gọi trong vùng găng ngay sau khi ghi – ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
// tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết cap_nhat chụp trước hay sau lần ghi của mình.
func (m *MayChuDatVe) danhSo(kq M) {
	p := m.demGhi.Add(1)
	m.phien.Store(p)
	kq["phien"] = p
}

func (m *MayChuDatVe) daGiu(kq M, that bool) {
	m.danhSo(kq)
	moi, nguoi := kq["moi"].([]string), kq["nguoi"].(string)
	m.So().Ghi(gdDatVe{"giu", moi, nguoi, kq["_luc"].(float64), kq["_het_han"].(float64)})
	if that {
		m.ThongBao(fmt.Sprintf("%s giữ %s (%s s)", nguoi, strings.Join(moi, ", "), FmtG(m.TSFloat("ttl_s"))))
	}
	m.BaoDoi()
}

func (m *MayChuDatVe) daBan(kq M, that bool) {
	m.danhSo(kq)
	ghe, nguoi := kq["ghe"].([]string), kq["nguoi"].(string)
	m.So().Ghi(gdDatVe{loai: "ban", ghe: ghe, nguoi: nguoi, luc: kq["_luc"].(float64)})
	if that {
		m.ThongBao(fmt.Sprintf("%s mua %d vé %s (%s)", nguoi, len(ghe), strings.Join(ghe, ", "), Tien(kq["tien"].(int64))))
	}
	m.BaoDoi()
}

func (m *MayChuDatVe) daNha(kq M, that bool) {
	m.danhSo(kq)
	ghe, nguoi := kq["ghe"].([]string), kq["nguoi"].(string)
	m.So().Ghi(gdDatVe{loai: "nha", ghe: ghe, nguoi: nguoi})
	if that {
		m.ThongBao(fmt.Sprintf("%s nhả %s", nguoi, strings.Join(ghe, ", ")))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ dọn ghế quá hạn (goroutine nền, 100 ms)

func (m *MayChuDatVe) dinhKy(bayGio float64) {
	for _, g := range TatCaGhe {
		v, co := m.Kho.Doc(g).(Ghe)
		if !co || v.TT != "giu" || bayGio < v.HetHan {
			continue
		}
		m.mu.Lock()
		dang := m.dangDon[g]
		m.dangDon[g] = true
		m.mu.Unlock()
		if dang {
			continue
		}
		g := g
		xuLy := func(doc M) (M, M) { // nhả ghế quá hạn cũng là một thao tác ghi
			v := doc[g].(Ghe)
			if v.TT != "giu" || GioDonDieu() < v.HetHan {
				return M{"ok": false}, nil // vừa được thanh toán / giữ lại
			}
			return M{"ok": true, "ghe": []string{g}, "nguoi": v.Nguoi}, M{g: gheTrong(v.Nguoi)}
		}
		m.Kho.ThucHien([]string{g}, xuLy, func(M) {
			m.mu.Lock()
			delete(m.dangDon, g)
			m.mu.Unlock()
		}, m.daHetHan)
	}
}

func (m *MayChuDatVe) daHetHan(kq M) {
	m.danhSo(kq)
	ghe, nguoi := kq["ghe"].([]string), kq["nguoi"].(string)
	m.So().Ghi(gdDatVe{loai: "het_han", ghe: ghe, nguoi: nguoi})
	m.mu.Lock()
	that := m.nguoiThat[nguoi]
	m.mu.Unlock()
	if that {
		m.ThongBao(fmt.Sprintf("Hết hạn giữ %s (%s), ghế trống lại", ghe[0], nguoi))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuDatVe) trangThai() M {
	phien := m.phien.Load() // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
	bayGio := GioDonDieu()
	var soDo []string
	chu, conLai := M{}, M{}
	var trong, giu, ban int64
	for _, r := range Hang {
		var dong strings.Builder
		for c := 1; c <= Cot; c++ {
			g := fmt.Sprintf("%c%d", r, c)
			v, _ := m.Kho.Doc(g).(Ghe)
			switch {
			case v.TT == "giu" && bayGio < v.HetHan:
				dong.WriteByte('g')
				chu[g] = v.Nguoi
				conLai[g] = LamTron((v.HetHan - bayGio) * 1000)
				giu++
			case v.TT == "ban":
				dong.WriteByte('b')
				chu[g] = v.Nguoi
				ban++
			default:
				dong.WriteByte('.')
				trong++
			}
		}
		soDo = append(soDo, dong.String())
	}
	ts := m.ThamSo()
	lay := func(k string) any { v, _ := ts.Lay(k); return v }
	return M{"phien": phien, "phim": lay("phim"), "suat": lay("suat"), "gia_ve": lay("gia_ve"), "ttl_s": lay("ttl_s"),
		"ghe_nong": lay("ghe_nong"), "hang": Hang, "cot": int64(Cot), "so_do": soDo, "chu": chu,
		"con_lai_ms": conLai, "dem": M{"trong": trong, "giu": giu, "ban": ban}}
}

func (m *MayChuDatVe) tomTat() string {
	var ban, giu int
	bayGio := GioDonDieu()
	for _, g := range TatCaGhe {
		v, _ := m.Kho.Doc(g).(Ghe)
		if v.TT == "ban" {
			ban++
		}
		if v.TT == "giu" && bayGio < v.HetHan {
			giu++
		}
	}
	m.mu.Lock()
	luat := m.luat
	m.mu.Unlock()
	return fmt.Sprintf("Đã bán %d/%d ghế, đang giữ %d, sổ %d vé", ban, len(TatCaGhe), giu, luat.soVe.Load())
}
