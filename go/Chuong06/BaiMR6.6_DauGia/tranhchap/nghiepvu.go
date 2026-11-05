package tranhchap

// NGHIỆP VỤ MR6.6 – lõi máy chủ đấu giá + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Một món hàng, n người cùng trả giá. Thao tác ghi duy nhất:
//
//	{"loai":"tra_gia","ma_yc":k,"gia":5300000[,"nguoi":"u17"]}
//
// Luật: giá ≥ giá hiện tại + bước giá; phiên còn hạn. Trả giá khi phiên còn < gia_han_s giây (mặc định
// 10 s) thì phiên được gia hạn tới "bây giờ + gia_han_s" ("đóng mềm" – soft close, chống "bắn tỉa" phút
// cuối – anti-sniping). Hết giờ: máy chủ đóng phiên, công bố người thắng.
//
// Trạng thái là MỘT ô "mon" của KhoTranhChap (struct Mon, sao chép theo giá trị): mọi thay đổi tạo
// giá trị mới (xuLy thuần) nên cả 4 chế độ dùng chung đúng một hàm nghiệp vụ.
//
// Sổ kiểm toán đếm hai sai phạm:
//
//	gia_thap_hon – một giá được chấp nhận mà thấp hơn (giá cao nhất đã chấp nhận trước + bước giá):
//	               hai người cùng đọc giá cũ, cùng "thắng" kiểm tra, người ghi sau đè người ghi trước;
//	gia_cuoi_sai – giá/người dẫn đầu cuối cùng khác giá cao nhất đã chấp nhận (giá cao bị ghi đè mất).

import (
	"fmt"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"
)

const TenMayChu = "Đấu giá MR6.6"

var LoaiYeuCau = []string{"tra_gia"}

// ThamSoMacDinh – thoi_gian_s = 0: không giới hạn.
var ThamSoMacDinh = TS{{"ten_mon", "Bình gốm hoa lam"}, {"gia_khoi_diem", int64(5_000_000)},
	{"buoc_gia", int64(100_000)}, {"thoi_gian_s", 0.0}, {"gia_han_s", 10.0}}

// Mon – giá trị của ô "mon".
type Mon struct {
	Gia     int64
	Nguoi   string // "" = chưa ai trả giá
	SoLan   int64
	HetHan  float64 // theo GioDonDieu()
	CoHan   bool    // false = không giới hạn thời gian
	KetThuc bool
}

func chuoiHoacNil(s string) any {
	if s == "" {
		return nil
	}
	return s
}

// ------------------------------------------------------------ sổ kiểm toán

type gdDauGia struct {
	nguoi string
	gia   int64
}

type luatDauGia struct {
	buoc         int64
	caoNhat      int64 // giá cao nhất đã chấp nhận (theo sổ)
	coCaoNhat    bool
	nguoiCaoNhat string
}

func (l *luatDauGia) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdDauGia)
	if l.coCaoNhat && gd.gia < l.caoNhat+l.buoc {
		so.SaiPham("gia_thap_hon", fmt.Sprintf("%s %s sau %s", gd.nguoi, Tien(gd.gia), Tien(l.caoNhat)))
	}
	if !l.coCaoNhat || gd.gia > l.caoNhat {
		l.caoNhat, l.coCaoNhat, l.nguoiCaoNhat = gd.gia, true, gd.nguoi
	}
}

func (l *luatDauGia) KiemTraCuoi(_ *SoKiemToan, doc M) map[string]int64 {
	m, co := doc["mon"].(Mon)
	if !co || !l.coCaoNhat {
		return nil
	}
	if m.Gia != l.caoNhat || m.Nguoi != l.nguoiCaoNhat {
		return map[string]int64{"gia_cuoi_sai": 1}
	}
	return nil
}

// ------------------------------------------------------------ máy chủ

type MayChuDauGia struct {
	*MayChuCoSo
	mu           sync.Mutex // lịch sử, lần báo gia hạn (khiGhi có thể chạy song song ở khong_khoa)
	lichSu       []M        // giá đã chấp nhận, mới nhất trước (để hiển thị), tối đa 50
	lucBaoGiaHan float64
	dangDong     atomic.Bool
}

// MoiMayChu – máy chủ đấu giá với tham số ban đầu thamSo (khóa trong ThamSoMacDinh).
func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuDauGia, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuDauGia{MayChuCoSo: co}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại phiên

func (m *MayChuDauGia) kiemThamSoNV(ts TS) error {
	lay := func(k string, md float64) float64 {
		if v, co := ts.Lay(k); co {
			return SoThuc(v)
		}
		return md
	}
	if lay("buoc_gia", 1) <= 0 || lay("gia_khoi_diem", 0) < 0 {
		return fmt.Errorf("bước giá > 0, giá khởi điểm ≥ 0")
	}
	if lay("thoi_gian_s", 0) < 0 || lay("gia_han_s", 0) < 0 {
		return fmt.Errorf("thời gian ≥ 0")
	}
	if v, co := ts.Lay("ten_mon"); co {
		if n := utf8.RuneCountInString(StrPy(v)); n < 1 || n > 40 {
			return fmt.Errorf("tên món 1–40 ký tự")
		}
	}
	return nil
}

func (m *MayChuDauGia) khoiTao() {
	tg := m.TSFloat("thoi_gian_s")
	m.Kho.DatO("mon", Mon{Gia: m.TSInt("gia_khoi_diem"), HetHan: GioDonDieu() + tg, CoHan: tg > 0})
	m.DatSo(MoiSoKiemToan(TS{{"gia_thap_hon", "giá thấp hơn"}, {"gia_cuoi_sai", "giá cuối sai"}},
		&luatDauGia{buoc: m.TSInt("buoc_gia")}))
	m.mu.Lock()
	m.lichSu, m.lucBaoGiaHan = nil, 0
	m.mu.Unlock()
	m.dangDong.Store(false)
}

// ------------------------------------------------------------ trả giá

func (m *MayChuDauGia) xuLyYeuCau(_ *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	gia, la := tin["gia"].(int64)
	if !la || gia <= 0 {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "giá phải là số nguyên dương"})
		return
	}
	buoc, giaHanS := m.TSInt("buoc_gia"), m.TSFloat("gia_han_s")
	xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc doc, trả về (kq, ghi)
		mon := doc["mon"].(Mon)
		bayGio := GioDonDieu()
		if mon.KetThuc || (mon.CoHan && bayGio >= mon.HetHan) {
			return M{"ok": false, "ma_loi": "het_phien", "ly_do": "phiên đã kết thúc", "gia": mon.Gia,
				"nguoi_dan_dau": chuoiHoacNil(mon.Nguoi)}, nil
		}
		if gia < mon.Gia+buoc {
			return M{"ok": false, "ma_loi": "gia_thap", "ly_do": "giá phải ≥ " + Tien(mon.Gia+buoc), "gia": mon.Gia,
				"nguoi_dan_dau": chuoiHoacNil(mon.Nguoi)}, nil
		}
		moi := mon // bản sao
		moi.Gia, moi.Nguoi, moi.SoLan = gia, nguoi, mon.SoLan+1
		giaHan := mon.CoHan && mon.HetHan-bayGio < giaHanS
		if giaHan {
			moi.HetHan = bayGio + giaHanS
		}
		return M{"ok": true, "gia": gia, "nguoi_dan_dau": nguoi, "gia_han": giaHan}, M{"mon": moi}
	}
	m.Kho.ThucHien([]string{"mon"}, xuLy, traLoi, m.daChapNhan)
}

// daChapNhan – gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
func (m *MayChuDauGia) daChapNhan(kq M) {
	nguoi, gia, giaHan := kq["nguoi_dan_dau"].(string), kq["gia"].(int64), kq["gia_han"].(bool)
	m.So().Ghi(gdDauGia{nguoi, gia})
	m.mu.Lock()
	m.lichSu = append([]M{{"nguoi": nguoi, "gia": gia, "gio": time.Now().Format("15:04:05"), "gia_han": giaHan}},
		m.lichSu...)
	if len(m.lichSu) > 50 {
		m.lichSu = m.lichSu[:50]
	}
	bao := giaHan && GioDonDieu()-m.lucBaoGiaHan > 1.0
	if bao {
		m.lucBaoGiaHan = GioDonDieu()
	}
	m.mu.Unlock()
	if bao {
		m.ThongBao(fmt.Sprintf("Gia hạn: %s trả giá phút cuối, còn %s s", nguoi, FmtG(m.TSFloat("gia_han_s"))))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ đồng hồ phiên (goroutine nền, 100 ms)

func (m *MayChuDauGia) dinhKy(bayGio float64) {
	mon, co := m.Kho.Doc("mon").(Mon)
	if !co || mon.KetThuc || !mon.CoHan || bayGio < mon.HetHan || !m.dangDong.CompareAndSwap(false, true) {
		return
	}
	xuLy := func(doc M) (M, M) { // đóng phiên cũng là một thao tác ghi
		mon := doc["mon"].(Mon)
		if mon.KetThuc || !mon.CoHan || GioDonDieu() < mon.HetHan {
			return M{"ok": false}, nil // vừa được gia hạn hoặc đã đóng
		}
		moi := mon
		moi.KetThuc = true
		return M{"ok": true, "nguoi": mon.Nguoi, "gia": mon.Gia}, M{"mon": moi}
	}
	m.Kho.ThucHien([]string{"mon"}, xuLy, func(M) { m.dangDong.Store(false) }, m.daDong)
}

func (m *MayChuDauGia) daDong(kq M) {
	if ai := kq["nguoi"].(string); ai != "" {
		m.ThongBao(fmt.Sprintf("Kết thúc phiên: %s thắng với %s", ai, Tien(kq["gia"].(int64))))
	} else {
		m.ThongBao("Kết thúc phiên: không có ai trả giá")
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuDauGia) trangThai() M {
	mon, _ := m.Kho.Doc("mon").(Mon)
	var conLai any
	if mon.CoHan {
		conLai = max(0, LamTron((mon.HetHan-GioDonDieu())*1000))
	}
	m.mu.Lock()
	ls := append([]M(nil), m.lichSu[:min(12, len(m.lichSu))]...)
	m.mu.Unlock()
	return M{"mon": M{"ten": m.TSStr("ten_mon"), "gia_khoi_diem": m.TSInt("gia_khoi_diem"),
		"buoc_gia": m.TSInt("buoc_gia"), "gia_han_s": m.TSFloat("gia_han_s")},
		"gia": mon.Gia, "nguoi_dan_dau": chuoiHoacNil(mon.Nguoi), "so_lan": mon.SoLan,
		"con_lai_ms": conLai, "ket_thuc": mon.KetThuc, "lich_su": ls}
}

func (m *MayChuDauGia) tomTat() string {
	mon, _ := m.Kho.Doc("mon").(Mon)
	ai := mon.Nguoi
	if ai == "" {
		ai = "–"
	}
	trang := "dẫn đầu"
	if mon.KetThuc {
		trang = "thắng"
	}
	return fmt.Sprintf("Giá cuối %s, %s %s", Tien(mon.Gia), trang, ai)
}
