package tranhchap

// NGHIỆP VỤ MR6.9 – lõi máy chủ quay số trúng thưởng + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Chương trình khuyến mãi "Vòng quay may mắn": 10 món quà, hàng chục nghìn lượt quay. Thao tác ghi duy nhất:
//
//	{"loai":"quay","ma_yc":k[,"nguoi":"u17"]}
//
// Luật: mỗi người quay ĐÚNG MỘT lần (quay lần nữa – dù bằng ma_yc khác, từ thiết bị khác – bị từ chối
// "da_quay" kèm kết quả cũ: idempotency theo tên); mỗi lượt trúng với xác suất p (mặc định 1/2000) nếu
// còn quà; trúng thì nhận ngẫu nhiên một món còn lại.
//
// Tài nguyên trong KhoTranhChap:
//   - ô "qua"      : khoQua{con: mã các món còn lại} – ô "nóng" mà mọi lượt TRÚNG cùng tranh;
//   - ô "ng:<tên>" : nil (chưa quay) | luotQuay{qua: mã hoặc -1} – một ô cho mỗi người, tạo khi người đó
//     quay lần đầu. Lượt không trúng chỉ chạm ô của chính mình nên các lượt chạy song song được (khóa
//     theo tài nguyên); chỉ lượt trúng mới khóa thêm ô "qua".
//
// Rút thăm (rand < p) làm TRƯỚC, ngoài vùng găng: kết quả rút thăm không phụ thuộc trạng thái.
//
// Sổ kiểm toán đếm hai sai phạm:
//
//	thua_qua   – một món quà đã trao lại được trao lần nữa (tổng số quà trao > số quà có);
//	quay_2_lan – một người có hai lượt quay được chấp nhận (hai thiết bị cùng bấm, ô của người đó bị ghi đè).

import (
	"fmt"
	"math"
	"math/rand/v2"
	"net"
	"sync"
	"sync/atomic"
	"time"
)

const TenMayChu = "Quay số MR6.9"

var LoaiYeuCau = []string{"quay"}

var ThamSoMacDinh = TS{{"so_qua", int64(10)}, {"p", 0.0005}}

var TenQua = []string{"Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm điện",
	"Loa", "Phiếu 500k"}

// ChuP – 0.0005 -> "1/2000"; 0.3 -> "0.3".
func ChuP(p float64) string {
	if p > 0 && math.Abs(1/p-math.RoundToEven(1/p)) < 1e-6 && math.RoundToEven(1/p) > 1 {
		return fmt.Sprintf("1/%d", int64(math.RoundToEven(1/p)))
	}
	return FmtG(p)
}

// khoQua – giá trị ô "qua" (lát cắt không bao giờ sửa tại chỗ: mỗi lần ghi tạo lát cắt mới).
type khoQua struct{ con []int }

// luotQuay – giá trị ô "ng:<tên>" sau khi người đó đã quay (qua = -1: không trúng).
type luotQuay struct{ qua int }

func maQua(q int) any {
	if q < 0 {
		return nil
	}
	return int64(q)
}

// ------------------------------------------------------------ sổ kiểm toán

type gdQuay struct {
	nguoi string
	qua   int
}

type luatQuaySo struct {
	daQuay map[string]bool // người đã có lượt quay được chấp nhận (theo sổ)
	daTrao map[int]string  // mã quà -> người nhận đầu tiên
	soTrao atomic.Int64    // số lượt trúng được chấp nhận
	soLuot atomic.Int64    // số lượt quay được chấp nhận
}

func (l *luatQuaySo) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdQuay)
	l.soLuot.Add(1)
	if l.daQuay[gd.nguoi] {
		so.SaiPham("quay_2_lan", gd.nguoi+" quay lần 2")
	}
	l.daQuay[gd.nguoi] = true
	if gd.qua >= 0 {
		l.soTrao.Add(1)
		if ai, co := l.daTrao[gd.qua]; co {
			so.SaiPham("thua_qua", fmt.Sprintf("%s: %s rồi %s", TenQua[gd.qua], ai, gd.nguoi))
		} else {
			l.daTrao[gd.qua] = gd.nguoi
		}
	}
}

func (l *luatQuaySo) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// ------------------------------------------------------------ máy chủ

type MayChuQuaySo struct {
	*MayChuCoSo
	mu         sync.Mutex
	daTao      map[string]bool // ô "ng:<tên>" đã tạo (tạo đúng một lần)
	nguoiTrung []M             // mới nhất trước, tối đa 50 (để hiển thị)
	luat       *luatQuaySo
	baoHet     atomic.Bool
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuQuaySo, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuQuaySo{MayChuCoSo: co, daTao: map[string]bool{}}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại chương trình

func (m *MayChuQuaySo) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("so_qua"); co {
		if n := SoNguyen(v); n < 1 || n > int64(len(TenQua)) {
			return fmt.Errorf("số quà 1–%d", len(TenQua))
		}
	}
	if v, co := ts.Lay("p"); co {
		if p := SoThuc(v); !(p >= 0 && p <= 1) {
			return fmt.Errorf("xác suất p trong [0, 1]")
		}
	}
	return nil
}

func (m *MayChuQuaySo) khoiTao() {
	soQua := int(m.TSInt("so_qua"))
	con := make([]int, soQua)
	for i := range con {
		con[i] = i
	}
	m.Kho.DatO("qua", khoQua{con})
	luat := &luatQuaySo{daQuay: map[string]bool{}, daTrao: map[int]string{}}
	m.DatSo(MoiSoKiemToan(TS{{"thua_qua", "thừa quà"}, {"quay_2_lan", "quay 2 lần"}}, luat))
	m.mu.Lock()
	m.daTao, m.nguoiTrung, m.luat = map[string]bool{}, nil, luat
	m.mu.Unlock()
	m.baoHet.Store(false)
}

func (m *MayChuQuaySo) oNguoi(nguoi string) string {
	k := "ng:" + nguoi
	m.mu.Lock()
	defer m.mu.Unlock()
	if !m.daTao[k] {
		m.daTao[k] = true
		m.Kho.DatO(k, nil)
	}
	return k
}

// ------------------------------------------------------------ quay

func (m *MayChuQuaySo) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	kNg := m.oNguoi(nguoi)
	rutTrung := rand.Float64() < m.TSFloat("p") // kim dừng ở ô quà với xác suất p
	cacKhoa := []string{kNg}
	if q, co := m.Kho.Doc("qua").(khoQua); rutTrung && co && len(q.con) > 0 { // hết quà thì khỏi chạm ô "qua"
		cacKhoa = append(cacKhoa, "qua")
	}
	xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc doc, trả về (kq, ghi)
		if cu, co := doc[kNg].(luotQuay); co {
			truoc := "không trúng"
			if cu.qua >= 0 {
				truoc = "trúng " + TenQua[cu.qua]
			}
			return M{"ok": false, "ma_loi": "da_quay", "ly_do": "đã quay rồi (" + truoc + ")", "qua": maQua(cu.qua)}, nil
		}
		if q, co := doc["qua"].(khoQua); co && len(q.con) > 0 {
			chon := q.con[rand.IntN(len(q.con))]
			con := make([]int, 0, len(q.con)-1)
			for _, x := range q.con {
				if x != chon {
					con = append(con, x)
				}
			}
			return M{"ok": true, "trung": true, "qua": int64(chon), "ten_qua": TenQua[chon], "nguoi": nguoi,
				"con_lai": int64(len(con))}, M{kNg: luotQuay{chon}, "qua": khoQua{con}}
		}
		return M{"ok": true, "trung": false, "qua": nil, "nguoi": nguoi}, M{kNg: luotQuay{-1}}
	}
	gop := k.SoNguoi > 0 // người ảo của bộ mô phỏng: không báo từng người
	m.Kho.ThucHien(cacKhoa, xuLy, traLoi, func(kq M) { m.daQuay(kq, gop) })
}

// daQuay – gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
func (m *MayChuQuaySo) daQuay(kq M, gop bool) {
	nguoi := kq["nguoi"].(string)
	qua := -1
	if q, la := kq["qua"].(int64); la {
		qua = int(q)
	}
	m.So().Ghi(gdQuay{nguoi, qua})
	if trung, _ := kq["trung"].(bool); trung {
		m.mu.Lock()
		m.nguoiTrung = append([]M{{"nguoi": nguoi, "qua": int64(qua), "gio": time.Now().Format("15:04:05")}},
			m.nguoiTrung...)
		if len(m.nguoiTrung) > 50 {
			m.nguoiTrung = m.nguoiTrung[:50]
		}
		luat := m.luat
		m.mu.Unlock()
		if !gop {
			m.ThongBao(fmt.Sprintf("%s trúng %s", nguoi, kq["ten_qua"]))
		}
		if SoNguyen(kq["con_lai"]) == 0 && m.baoHet.CompareAndSwap(false, true) {
			m.ThongBao(fmt.Sprintf("Đã trao hết quà sau %d lượt", luat.soLuot.Load()))
		}
	}
	m.BaoDoi()
}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuQuaySo) trangThai() M {
	soQua := int(m.TSInt("so_qua"))
	con := map[int]bool{}
	if q, co := m.Kho.Doc("qua").(khoQua); co {
		for _, x := range q.con {
			con[x] = true
		}
	}
	m.mu.Lock()
	trung := append([]M(nil), m.nguoiTrung...)
	luat := m.luat
	m.mu.Unlock()
	nhan := map[int64]string{}
	for i := len(trung) - 1; i >= 0; i-- { // người nhận gần nhất của mỗi món
		nhan[SoNguyen(trung[i]["qua"])] = trung[i]["nguoi"].(string)
	}
	ds := make([]M, soQua)
	for i := range ds {
		var ai any
		if !con[i] {
			if s, co := nhan[int64(i)]; co {
				ai = s
			}
		}
		ds[i] = M{"ma": int64(i), "ten": TenQua[i], "con": con[i], "nguoi": ai}
	}
	if len(trung) > 20 {
		trung = trung[:20]
	}
	if trung == nil {
		trung = []M{}
	}
	return M{"so_qua": int64(soQua), "p": m.TS("p"), "con_lai": int64(len(con)), "qua": ds,
		"da_quay": luat.soLuot.Load(), "so_trung": luat.soTrao.Load(), "nguoi_trung": trung}
}

func (m *MayChuQuaySo) tomTat() string {
	m.mu.Lock()
	luat := m.luat
	m.mu.Unlock()
	return fmt.Sprintf("Trao %d/%d quà, %d lượt, p %s", luat.soTrao.Load(), m.TSInt("so_qua"), luat.soLuot.Load(),
		ChuP(m.TSFloat("p")))
}

func (m *MayChuQuaySo) dinhKy(float64) {}

// ------------------------------------------------------------ tiện ích cho client dòng lệnh

// GuiTho – gửi nguyên một thông điệp (không đăng ký chờ trả lời): lệnh "gui_lai" của người quay dòng
// lệnh gửi lại ĐÚNG một ma_yc cũ, như client gửi lại khi mạng chập chờn.
func (kn *KetNoiKhach) GuiTho(v any) error {
	kn.khoa.Lock()
	defer kn.khoa.Unlock()
	if kn.conn == nil {
		return net.ErrClosed
	}
	return GuiJSON(kn.conn, v)
}
