package tranhchap

// NGHIỆP VỤ MR6.11 – lõi máy chủ flash sale + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// "Giờ vàng" flash sale: kho có 100 sản phẩm, mở bán lúc T (đếm ngược), hàng nghìn người cùng bấm
// "MUA NGAY" đúng lúc mở. Thao tác ghi duy nhất:
//
//	{"loai":"mua","ma_yc":k,"so_luong":1|2[,"nguoi":"u17"]}
//
// Luật: chưa tới giờ mở bán -> "chua_mo" (kèm con_ms); mỗi đơn 1–toi_da (mặc định 2) sản phẩm; tồn kho
// phải đủ (còn 1 mà mua 2 -> "khong_du"); hết -> "het_hang".
//
// Trạng thái là MỘT ô "kho" của KhoTranhChap (struct khoHang: tồn, đã bán, số đơn) – điểm nóng mà mọi
// đơn hàng cùng tranh. Hai việc KHÔNG cần đi qua vùng găng: kiểm tra giờ mở bán (chỉ đọc đồng hồ) và
// "hết hàng nhanh" (đọc tồn kho không khóa: tồn chỉ giảm, nên đã thấy 0 thì chắc chắn là 0) – giúp hàng
// nghìn yêu cầu đến sau khi cháy hàng được trả lời ngay mà không phải xếp hàng chờ khóa.
//
// Sổ kiểm toán (tổng đã bán theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//
//	ton_am  – một đơn được chấp nhận làm tổng đã bán vượt tồn đầu kỳ (tồn kho thật bị âm – bán vượt);
//	lech_so – cuối buổi: tổng đã bán theo sổ ≠ tồn đầu kỳ − tồn hiện tại (đếm số sản phẩm lệch).

import (
	"fmt"
	"sync"
	"sync/atomic"
	"time"
)

const TenMayChu = "Flash sale MR6.11"

var LoaiYeuCau = []string{"mua"}

var ThamSoMacDinh = TS{{"ten_sp", "Tai nghe không dây TWS Pro"}, {"gia_goc", int64(1_990_000)},
	{"gia_sale", int64(199_000)}, {"ton_dau", int64(100)}, {"mo_sau_s", 30.0}, {"toi_da", int64(2)}}

// khoHang – giá trị ô "kho" (sao chép theo giá trị).
type khoHang struct{ ton, daBan, soDon int64 }

// ------------------------------------------------------------ sổ kiểm toán

type gdMua struct {
	nguoi, maDon string
	soLuong      int64
}

type luatFlashSale struct {
	tonDau  int64
	tongBan atomic.Int64 // tổng đã bán theo sổ
	soDon   atomic.Int64 // số đơn được chấp nhận theo sổ
}

func (l *luatFlashSale) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdMua)
	l.soDon.Add(1)
	if t := l.tongBan.Add(gd.soLuong); t > l.tonDau {
		so.SaiPham("ton_am", fmt.Sprintf("%s mua %d: bán %d/%d", gd.nguoi, gd.soLuong, t, l.tonDau))
	}
}

func (l *luatFlashSale) KiemTraCuoi(_ *SoKiemToan, doc M) map[string]int64 {
	k, co := doc["kho"].(khoHang)
	if !co {
		return nil
	}
	lech := (l.tonDau - k.ton) - l.tongBan.Load()
	if lech < 0 {
		lech = -lech
	}
	if lech == 0 {
		return nil
	}
	return map[string]int64{"lech_so": lech}
}

// ------------------------------------------------------------ máy chủ

type MayChuFlashSale struct {
	*MayChuCoSo
	mu       sync.Mutex
	moLuc    float64 // thời điểm mở bán theo GioDonDieu()
	donGan   []M     // đơn mới nhất trước, tối đa 30 (để hiển thị)
	luat     *luatFlashSale
	daBaoMo  bool
	daBaoHet atomic.Bool
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuFlashSale, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuFlashSale{MayChuCoSo: co}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại đợt sale

func (m *MayChuFlashSale) kiemThamSoNV(ts TS) error {
	lay := func(k string, md float64) float64 {
		if v, co := ts.Lay(k); co {
			return SoThuc(v)
		}
		return md
	}
	if t := lay("ton_dau", 1); t < 1 || t > 1_000_000 {
		return fmt.Errorf("tồn đầu kỳ 1–1 000 000")
	}
	if lay("mo_sau_s", 0) < 0 {
		return fmt.Errorf("mo_sau_s ≥ 0")
	}
	if t := lay("toi_da", 1); t < 1 || t > 10 {
		return fmt.Errorf("tối đa mỗi đơn 1–10")
	}
	if lay("gia_sale", 1) <= 0 || lay("gia_goc", 1) <= 0 {
		return fmt.Errorf("giá > 0")
	}
	return nil
}

func (m *MayChuFlashSale) khoiTao() {
	tonDau := m.TSInt("ton_dau")
	m.Kho.DatO("kho", khoHang{ton: tonDau})
	luat := &luatFlashSale{tonDau: tonDau}
	m.DatSo(MoiSoKiemToan(TS{{"ton_am", "tồn âm"}, {"lech_so", "lệch sổ"}}, luat))
	m.mu.Lock()
	m.moLuc = GioDonDieu() + m.TSFloat("mo_sau_s")
	m.donGan, m.luat = nil, luat
	m.daBaoMo = m.TSFloat("mo_sau_s") <= 0
	m.mu.Unlock()
	m.daBaoHet.Store(false)
}

func (m *MayChuFlashSale) layMoLuc() float64 {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.moLuc
}

// ------------------------------------------------------------ mua

func (m *MayChuFlashSale) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	toiDa := m.TSInt("toi_da")
	sl, la := tin["so_luong"].(int64)
	if !la || sl < 1 || sl > toiDa {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": fmt.Sprintf("số lượng 1–%d", toiDa)})
		return
	}
	if con := m.layMoLuc() - GioDonDieu(); con > 0 { // chưa tới giờ: chỉ đọc đồng hồ, không chạm kho
		traLoi(M{"ok": false, "ma_loi": "chua_mo", "ly_do": fmt.Sprintf("chưa mở bán, còn %.1f s", con),
			"con_ms": LamTron(con * 1000)})
		return
	}
	if kh, co := m.Kho.Doc("kho").(khoHang); co && kh.ton <= 0 { // hết hàng nhanh: tồn chỉ giảm, đọc không khóa
		traLoi(M{"ok": false, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": int64(0)})
		return
	}
	gia := m.TSInt("gia_sale")
	xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc doc, trả về (kq, ghi)
		kh := doc["kho"].(khoHang)
		if kh.ton <= 0 {
			return M{"ok": false, "ma_loi": "het_hang", "ly_do": "hết hàng", "ton": int64(0)}, nil
		}
		if kh.ton < sl {
			return M{"ok": false, "ma_loi": "khong_du", "ly_do": fmt.Sprintf("chỉ còn %d sản phẩm", kh.ton),
				"ton": kh.ton}, nil
		}
		moi := khoHang{kh.ton - sl, kh.daBan + sl, kh.soDon + 1}
		return M{"ok": true, "ma_don": fmt.Sprintf("FS%05d", moi.soDon), "so_luong": sl, "ton": moi.ton,
			"nguoi": nguoi, "thanh_tien": sl * gia}, M{"kho": moi}
	}
	gop := k.SoNguoi > 0
	m.Kho.ThucHien([]string{"kho"}, xuLy, traLoi, func(kq M) { m.daBan(kq, gop) })
}

// daBan – gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
func (m *MayChuFlashSale) daBan(kq M, gop bool) {
	nguoi, sl, maDon := kq["nguoi"].(string), kq["so_luong"].(int64), kq["ma_don"].(string)
	m.So().Ghi(gdMua{nguoi, maDon, sl})
	m.mu.Lock()
	m.donGan = append([]M{{"nguoi": nguoi, "so_luong": sl, "gio": time.Now().Format("15:04:05")}}, m.donGan...)
	if len(m.donGan) > 30 {
		m.donGan = m.donGan[:30]
	}
	moLuc := m.moLuc
	m.mu.Unlock()
	if !gop {
		m.ThongBao(fmt.Sprintf("%s mua %d – đơn %s, còn %d", nguoi, sl, maDon, SoNguyen(kq["ton"])))
	}
	if SoNguyen(kq["ton"]) == 0 && m.daBaoHet.CompareAndSwap(false, true) {
		m.ThongBao(fmt.Sprintf("HẾT HÀNG sau %.2f s mở bán", GioDonDieu()-moLuc))
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ đồng hồ mở bán (goroutine nền, 100 ms)

func (m *MayChuFlashSale) dinhKy(bayGio float64) {
	m.mu.Lock()
	bao := !m.daBaoMo && bayGio >= m.moLuc
	if bao {
		m.daBaoMo = true
	}
	m.mu.Unlock()
	if bao {
		m.ThongBao(fmt.Sprintf("MỞ BÁN! %d sản phẩm", m.TSInt("ton_dau")))
		m.BaoDoi()
	}
}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuFlashSale) trangThai() M {
	kh, _ := m.Kho.Doc("kho").(khoHang)
	m.mu.Lock()
	con := m.moLuc - GioDonDieu()
	don := append([]M(nil), m.donGan...)
	luat := m.luat
	m.mu.Unlock()
	if len(don) > 10 {
		don = don[:10]
	}
	if don == nil {
		don = []M{}
	}
	return M{"sp": M{"ten": m.TS("ten_sp"), "gia_goc": m.TS("gia_goc"), "gia_sale": m.TS("gia_sale"),
		"toi_da": m.TS("toi_da")}, "ton_dau": m.TS("ton_dau"), "ton": kh.ton, "da_ban": kh.daBan, "so_don": kh.soDon,
		"ban_theo_so": luat.tongBan.Load(), "mo_ban": con <= 0, "mo_con_ms": max(0, LamTron(con*1000)), "don_gan": don}
}

func (m *MayChuFlashSale) tomTat() string {
	kh, _ := m.Kho.Doc("kho").(khoHang)
	m.mu.Lock()
	luat := m.luat
	m.mu.Unlock()
	return fmt.Sprintf("Bán %d/%d sp (%d đơn), tồn %d", luat.tongBan.Load(), m.TSInt("ton_dau"), luat.soDon.Load(), kh.ton)
}
