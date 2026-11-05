package tranhchap

// NGHIỆP VỤ MR6.12 – lõi máy chủ ngân hàng: chuyển khoản + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// 5–20 tài khoản, mỗi tài khoản là MỘT ô của KhoTranhChap (giá trị = số dư, int64 đồng). Thao tác ghi:
//
//	{"loai":"chuyen","ma_yc":k,"tu":"TK01","den":"TK02","so_tien":300000[,"noi_dung":"..."][,"nguoi":"u17"]}
//
// Luật: trừ tài khoản nguồn, cộng tài khoản đích trong CÙNG một thao tác (hai ô); số dư nguồn không đủ
// thì từ chối. Nhiều người chuyển chéo cùng lúc (A→B và B→A) nên một thao tác luôn đụng HAI tài nguyên:
//
//	khoa            khóa hai tài khoản theo THỨ TỰ MÃ (TK01 trước TK02) -> mọi goroutine xin khóa cùng thứ
//	                tự, không thể có vòng chờ -> không khóa chết;
//	khoa_ngay_tho=1 khóa nguồn rồi mới khóa đích: A→B giữ A chờ B, B→A giữ B chờ A -> KHÓA CHẾT (deadlock).
//	                Khung phát hiện bằng hết thời gian chờ khóa (2 s): giao dịch "nạn nhân" bị hủy, nhả
//	                khóa, giao dịch kia đi tiếp; máy chủ báo "khóa chết: …";
//	lac_quan        đọc (số dư, phiên bản) của cả hai tài khoản, rồi so sánh-và-đổi CẢ HAI một cách nguyên tử.
//
// Sổ kiểm toán ghi mọi giao dịch được chấp nhận và tự tính số dư theo sổ; đếm ba sai phạm:
//
//	so_du_am  "thấu chi"  – theo sổ, một giao dịch làm tài khoản nguồn âm (hai goroutine cùng thấy "đủ
//	                        tiền" rồi cùng trừ);
//	so_du_sai "lệch TK"   – số tài khoản có số dư cuối khác số dư theo sổ (mất cập nhật);
//	tong_lech "tổng lệch" – tổng tiền toàn hệ thống khác tổng ban đầu (tiền tự sinh ra / mất đi).

import (
	"fmt"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"
)

const TenMayChu = "Ngân hàng MR6.12"

var LoaiYeuCau = []string{"chuyen"}

var ThamSoMacDinh = TS{{"so_tk", int64(10)}, {"so_du_dau", int64(5_000_000)}, {"khoa_ngay_tho", int64(0)}}

var ChuTK = []string{"An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Lan", "Minh", "Nam",
	"Oanh", "Phúc", "Quân", "Sơn", "Tâm", "Uyên", "Vy", "Xuân", "Yến", "Thảo"}

func MaTK(i int) string { return fmt.Sprintf("TK%02d", i+1) }

// TienDau – +300.000 đ / -300.000 đ / 0 đ.
func TienDau(x int64) string {
	if x > 0 {
		return "+" + Tien(x)
	}
	return Tien(x)
}

// ------------------------------------------------------------ sổ kiểm toán

type gdChuyen struct {
	nguoi, tu, den string
	soTien         int64
}

type luatNganHang struct {
	soDu    map[string]int64 // số dư theo sổ (cộng trừ đúng từng giao dịch đã chấp nhận)
	tongDau int64
}

func (l *luatNganHang) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdChuyen)
	l.soDu[gd.tu] -= gd.soTien
	l.soDu[gd.den] += gd.soTien
	if l.soDu[gd.tu] < 0 {
		so.SaiPham("so_du_am", fmt.Sprintf("%s %s→%s: %s còn %s", gd.nguoi, gd.tu, gd.den, gd.tu, Tien(l.soDu[gd.tu])))
	}
}

func (l *luatNganHang) KiemTraCuoi(_ *SoKiemToan, doc M) map[string]int64 {
	var sai, tong int64
	for k, v := range l.soDu {
		gt, co := doc[k].(int64)
		if !co {
			return nil
		}
		if gt != v {
			sai++
		}
		tong += gt
	}
	lech := int64(0)
	if tong != l.tongDau {
		lech = 1
	}
	return map[string]int64{"so_du_sai": sai, "tong_lech": lech}
}

// ------------------------------------------------------------ máy chủ

type MayChuNganHang struct {
	*MayChuCoSo
	mu        sync.Mutex
	dsTK      []string
	chu       map[string]string
	tongDau   int64
	lichSu    []M          // giao dịch đã chấp nhận, mới nhất trước, tối đa 50
	maGD      atomic.Int64 // mã giao dịch tăng mãi, kể cả qua các lần đặt lại
	soGD      atomic.Int64
	khoaChet  int64
	kcChuaBao int64
	lucBaoKC  float64
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuNganHang, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuNganHang{MayChuCoSo: co}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại

func (m *MayChuNganHang) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("so_tk"); co {
		if n := SoNguyen(v); n < 5 || n > int64(len(ChuTK)) {
			return fmt.Errorf("số tài khoản 5–%d", len(ChuTK))
		}
	}
	if v, co := ts.Lay("so_du_dau"); co && SoNguyen(v) < 0 {
		return fmt.Errorf("số dư ban đầu ≥ 0")
	}
	if v, co := ts.Lay("khoa_ngay_tho"); co {
		if n := SoNguyen(v); n != 0 && n != 1 {
			return fmt.Errorf("khoa_ngay_tho = 0 hoặc 1")
		}
	}
	return nil
}

func (m *MayChuNganHang) khoiTao() {
	soTK, soDu := int(m.TSInt("so_tk")), m.TSInt("so_du_dau")
	ds := make([]string, soTK)
	chu := map[string]string{}
	luat := &luatNganHang{soDu: map[string]int64{}, tongDau: int64(soTK) * soDu}
	for i := range ds {
		ds[i] = MaTK(i)
		m.Kho.DatO(ds[i], soDu)
		chu[ds[i]] = ChuTK[i]
		luat.soDu[ds[i]] = soDu
	}
	m.Kho.KhoaNgayTho.Store(m.TSInt("khoa_ngay_tho") != 0)
	m.DatSo(MoiSoKiemToan(TS{{"tong_lech", "tổng lệch"}, {"so_du_sai", "lệch TK"}, {"so_du_am", "thấu chi"}}, luat))
	m.mu.Lock()
	m.dsTK, m.chu, m.tongDau, m.lichSu = ds, chu, luat.tongDau, nil
	m.khoaChet, m.kcChuaBao, m.lucBaoKC = 0, 0, -1e9
	m.mu.Unlock()
	m.soGD.Store(0)
}

// ------------------------------------------------------------ chuyển khoản

func (m *MayChuNganHang) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	x, la := tin["so_tien"].(int64)
	if !la || x <= 0 {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "số tiền phải là số nguyên dương"})
		return
	}
	tu, _ := tin["tu"].(string)
	den, _ := tin["den"].(string)
	m.mu.Lock()
	_, coTu := m.chu[tu]
	_, coDen := m.chu[den]
	m.mu.Unlock()
	if !coTu || !coDen {
		traLoi(M{"ok": false, "ma_loi": "khong_co", "ly_do": "tài khoản không tồn tại"})
		return
	}
	if tu == den {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "tài khoản nguồn và đích trùng nhau"})
		return
	}
	noiDung := ""
	if v, co := tin["noi_dung"]; co {
		s, la := v.(string)
		if !la || utf8.RuneCountInString(s) > 40 {
			traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "nội dung tối đa 40 ký tự"})
			return
		}
		noiDung = s
	}
	xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc hai số dư, trả về (kq, ghi)
		a, b := doc[tu].(int64), doc[den].(int64)
		if a < x {
			return M{"ok": false, "ma_loi": "khong_du", "ly_do": fmt.Sprintf("không đủ số dư (%s còn %s)", tu, Tien(a)),
				"so_du": a}, nil
		}
		return M{"ok": true, "tu": tu, "den": den, "so_tien": x, "so_du": a - x, "nguoi": nguoi, "noi_dung": noiDung},
			M{tu: a - x, den: b + x}
	}
	tra := func(kq M) {
		if kq["ma_loi"] == "khoa_cho_lau" {
			kq = m.baoKhoaChet(nguoi, tu, den, kq)
		}
		traLoi(kq)
	}
	// thứ tự [nguồn, đích]: chế độ khoa sắp xếp lại theo mã, khóa ngây thơ thì giữ nguyên
	m.Kho.ThucHien([]string{tu, den}, xuLy, tra, m.daChuyen)
}

// daChuyen – gọi trong vùng găng ngay sau khi ghi hai số dư: ghi sổ kiểm toán đúng thứ tự ghi.
func (m *MayChuNganHang) daChuyen(kq M) {
	ma := m.maGD.Add(1)
	kq["ma_gd"] = ma
	m.soGD.Add(1)
	nguoi, tu, den, x := kq["nguoi"].(string), kq["tu"].(string), kq["den"].(string), kq["so_tien"].(int64)
	m.So().Ghi(gdChuyen{nguoi, tu, den, x})
	m.mu.Lock()
	m.lichSu = append([]M{{"ma": ma, "gio": time.Now().Format("15:04:05"), "nguoi": nguoi, "tu": tu, "den": den,
		"so_tien": x, "noi_dung": kq["noi_dung"]}}, m.lichSu...)
	if len(m.lichSu) > 50 {
		m.lichSu = m.lichSu[:50]
	}
	m.mu.Unlock()
	m.BaoDoi()
}

// baoKhoaChet – chờ khóa quá 2 s: với khóa ngây thơ đó là khóa chết. Báo (tối đa 1 dòng/giây) và hủy
// giao dịch.
func (m *MayChuNganHang) baoKhoaChet(nguoi, tu, den string, kq M) M {
	k := ""
	if p := strings.Fields(Chuoi(kq["ly_do"])); len(p) > 2 { // "chờ khóa TK02 quá 2 s (khóa chết?)"
		k = p[2]
	}
	ngayTho := m.Kho.KhoaNgayTho.Load()
	m.mu.Lock()
	m.khoaChet++
	bayGio := GioDonDieu()
	bao := bayGio-m.lucBaoKC >= 1.0
	var them int64
	if bao {
		m.lucBaoKC, them, m.kcChuaBao = bayGio, m.kcChuaBao, 0
	} else {
		m.kcChuaBao++
	}
	m.mu.Unlock()
	if bao {
		loai := "chờ khóa quá hạn"
		if ngayTho {
			loai = "khóa chết"
		}
		text := fmt.Sprintf("%s: %s %s→%s, chờ %s quá %s s", loai, nguoi, tu, den, k, FmtG(ChoKhoaToiDaS))
		if them > 0 {
			text += fmt.Sprintf(" (+%d vụ)", them)
		}
		m.ThongBao(text)
	}
	lyDo := fmt.Sprintf("chờ %s quá %s s, đã hủy giao dịch", k, FmtG(ChoKhoaToiDaS))
	if ngayTho {
		lyDo = "khóa chết: " + lyDo
	}
	ra := M{}
	for x, v := range kq {
		ra[x] = v
	}
	ra["ly_do"] = lyDo
	return ra
}

func (m *MayChuNganHang) dinhKy(float64) {}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuNganHang) trangThai() M {
	snap := m.Kho.AnhChup()
	m.mu.Lock()
	ds, chu, tongDau := m.dsTK, m.chu, m.tongDau
	ls := append([]M(nil), m.lichSu...)
	khoaChet := m.khoaChet
	m.mu.Unlock()
	var tk []M
	var tong int64
	for _, ma := range ds {
		so, _ := snap[ma].(int64)
		tk = append(tk, M{"ma": ma, "chu": chu[ma], "so_du": so})
		tong += so
	}
	if len(ls) > 20 {
		ls = ls[:20]
	}
	if ls == nil {
		ls = []M{}
	}
	return M{"tai_khoan": tk, "tong": tong, "tong_dau": tongDau, "khoa_ngay_tho": m.Kho.KhoaNgayTho.Load(),
		"khoa_chet": khoaChet, "so_gd": m.soGD.Load(), "lich_su": ls}
}

func (m *MayChuNganHang) tomTat() string {
	snap := m.Kho.AnhChup()
	m.mu.Lock()
	ds, tongDau := m.dsTK, m.tongDau
	m.mu.Unlock()
	var tong int64
	for _, ma := range ds {
		so, _ := snap[ma].(int64)
		tong += so
	}
	if lech := tong - tongDau; lech != 0 {
		return fmt.Sprintf("Tổng tiền %s, LỆCH %s", Tien(tong), TienDau(lech))
	}
	return fmt.Sprintf("Tổng tiền %s (khớp), %d tài khoản", Tien(tong), len(ds))
}
