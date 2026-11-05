package tranhchap

// NGHIỆP VỤ MR6.7 – lõi máy chủ đặt xe + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// m chiếc xe (mặc định 1), mỗi xe MỘT chỗ; n người cùng bấm "Đặt xe". Hai thao tác ghi:
//
//	{"loai":"dat_xe","ma_yc":k[,"don":[x,y],"den":[x,y]][,"nguoi":"u17"]}
//	{"loai":"huy","ma_yc":k[,"nguoi":"u17"]}
//
// Luật: mỗi người chỉ giữ một chuyến; máy chủ giao xe TRỐNG gần điểm đón nhất; hết xe trống -> "hết chỗ".
// Hủy chuyến trả xe về trạng thái trống.
//
// Bản đồ là lưới 5 × 5 đường (tọa độ 0–1, 1 đơn vị = 5 km); điểm đón/đến là ngã tư. Mỗi xe là MỘT ô
// "xe1", "xe2"… của KhoTranhChap (struct Xe). Vì phải biết người đặt đã có chuyến chưa và xe nào trống gần
// nhất, đặt xe đọc–ghi CẢ m ô trong một thao tác (khoa: giữ khóa mọi xe theo thứ tự tên ô; lac_quan: so
// sánh phiên bản mọi xe).
//
// Sổ kiểm toán đếm một sai phạm:
//
//	trung_cho – một lần đặt được chấp nhận cho một xe mà (theo sổ) đang chở người khác: hai người cùng
//	            đọc "xe trống", cùng được báo "Đặt thành công – tài xế Hùng", người ghi sau đè người ghi
//	            trước, người kia đứng đường chờ một chiếc xe không đến.

import (
	"fmt"
	"math"
	"sync"
	"sync/atomic"
	"time"
)

const TenMayChu = "Đặt xe MR6.7"

var LoaiYeuCau = []string{"dat_xe", "huy"}

var ThamSoMacDinh = TS{{"so_xe", int64(1)}}

// Bản đồ: 5 đường dọc / 5 đường ngang.
var (
	Duong    = []float64{0.1, 0.3, 0.5, 0.7, 0.9}
	TenDoc   = []string{"Lê Lợi", "Hai Bà Trưng", "Nguyễn Huệ", "Pasteur", "Đồng Khởi"}
	TenNgang = []string{"Lý Tự Trọng", "Lê Thánh Tôn", "Nguyễn Du", "Võ Văn Tần", "Điện Biên Phủ"}
	NgaTu    [][2]float64
	taiXe    = [][3]string{{"Hùng", "51A-123.45", "trắng"}, {"Lan", "51G-678.90", "đỏ"}, {"Minh", "51F-246.80", "xanh"},
		{"Tuấn", "51H-135.79", "bạc"}, {"Hoa", "51K-864.20", "vàng"}, {"Phúc", "51A-975.31", "đen"},
		{"Quang", "51G-112.23", "cam"}, {"Thảo", "51F-445.56", "tím"}, {"Sơn", "51H-778.89", "xám"},
		{"Vy", "51K-990.01", "hồng"}}
	viTriXe = [][2]float64{{0.9, 0.1}, {0.1, 0.9}, {0.5, 0.3}, {0.3, 0.7}, {0.7, 0.5}, {0.1, 0.3}, {0.9, 0.7},
		{0.5, 0.9}, {0.3, 0.1}, {0.7, 0.9}}
)

const KmMoiDonVi = 5.0

func init() {
	for _, y := range Duong {
		for _, x := range Duong {
			NgaTu = append(NgaTu, [2]float64{x, y})
		}
	}
}

func chiSoDuong(v float64) int {
	for i, d := range Duong {
		if math.Abs(d-v) < 1e-9 {
			return i
		}
	}
	return 0
}

// TenDiem – (0.1, 0.5) -> "Lê Lợi × Nguyễn Du".
func TenDiem(p [2]float64) string {
	return TenDoc[chiSoDuong(p[0])] + " × " + TenNgang[chiSoDuong(p[1])]
}

// Km – đường đi trên lưới phố (Manhattan) giữa hai điểm, km.
func Km(a, b [2]float64) float64 { return (math.Abs(a[0]-b[0]) + math.Abs(a[1]-b[1])) * KmMoiDonVi }

// Cuoc – giá cước: 12.000 đ mở cửa + 10.000 đ/km, làm tròn tới 1.000 đ.
func Cuoc(a, b [2]float64) int64 { return LamTron((12_000+10_000*Km(a, b))/1000) * 1000 }

// phutDen – thời gian xe tới điểm đón (30 km/h = 0,5 km/phút), tối thiểu 1 phút.
func phutDen(xe, don [2]float64) int64 { return max(1, LamTron(Km(xe, don)/0.5)) }

// diemNgaTu – [x, y] là một ngã tư trên bản đồ? (làm tròn 2 chữ số như bản Python)
func diemNgaTu(v any) ([2]float64, bool) {
	ds, la := v.([]any)
	if !la || len(ds) != 2 {
		return [2]float64{}, false
	}
	var p [2]float64
	for i, x := range ds {
		switch t := x.(type) {
		case int64:
			p[i] = float64(t)
		case float64:
			p[i] = t
		default:
			return p, false
		}
		p[i] = math.RoundToEven(p[i]*100) / 100
	}
	for _, q := range NgaTu {
		if math.Abs(q[0]-p[0]) < 1e-9 && math.Abs(q[1]-p[1]) < 1e-9 {
			return q, true
		}
	}
	return p, false
}

func diemJSON(p [2]float64) []any { return []any{p[0], p[1]} }

// Xe – giá trị của một ô "xeK".
type Xe struct {
	Ma                 int64
	TaiXe, BienSo, Mau string
	ViTri              [2]float64
	Khach              string // "" = trống
	Don, Den           [2]float64
	Gio                string
	Cuoc, Phut         int64
}

// json – xe như bản Python gửi trong cap_nhat (null khi xe trống).
func (x Xe) json() M {
	m := M{"ma": x.Ma, "tai_xe": x.TaiXe, "bien_so": x.BienSo, "mau": x.Mau, "vi_tri": diemJSON(x.ViTri),
		"khach": nil, "don": nil, "den": nil, "gio": nil, "cuoc": nil, "phut": nil}
	if x.Khach != "" {
		m["khach"], m["don"], m["den"], m["gio"], m["cuoc"], m["phut"] = x.Khach, diemJSON(x.Don), diemJSON(x.Den),
			x.Gio, x.Cuoc, x.Phut
	}
	return m
}

// ------------------------------------------------------------ sổ kiểm toán

type gdDatXe struct {
	loai  string
	xe    int64
	nguoi string
}

type luatDatXe struct {
	trenXe map[int64][]string // theo sổ: ai đang "có" chỗ trên xe i
	soDat  atomic.Int64       // số lần đặt được chấp nhận
}

func (l *luatDatXe) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdDatXe)
	ds := l.trenXe[gd.xe]
	if gd.loai == "dat" {
		l.soDat.Add(1)
		if len(ds) > 0 {
			so.SaiPham("trung_cho", fmt.Sprintf("xe %d: %s khi %s đang đặt", gd.xe, gd.nguoi, ds[len(ds)-1]))
		}
		l.trenXe[gd.xe] = append(ds, gd.nguoi)
		return
	}
	for i, n := range ds { // hủy
		if n == gd.nguoi {
			l.trenXe[gd.xe] = append(ds[:i:i], ds[i+1:]...)
			break
		}
	}
}

func (l *luatDatXe) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// ------------------------------------------------------------ máy chủ

type MayChuDatXe struct {
	*MayChuCoSo
	mu     sync.Mutex // cacO, lichSu, luat (khiGhi có thể chạy song song ở khong_khoa)
	cacO   []string
	lichSu []M
	luat   *luatDatXe
	demGhi atomic.Int64 // số thứ tự lần ghi: tăng mãi, kể cả qua các lần đặt lại
	phien  atomic.Int64
}

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuDatXe, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuDatXe{MayChuCoSo: co}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại

func (m *MayChuDatXe) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("so_xe"); co {
		if n := SoNguyen(v); n < 1 || n > 50 {
			return fmt.Errorf("số xe 1–50")
		}
	}
	return nil
}

func (m *MayChuDatXe) khoiTao() {
	n := int(m.TSInt("so_xe"))
	var cacO []string
	for i := 0; i < n; i++ {
		tx := taiXe[i%len(taiXe)]
		ten := tx[0]
		if i >= len(taiXe) {
			ten = fmt.Sprintf("%s %d", ten, i/len(taiXe)+1)
		}
		vt := NgaTu[(i*7)%len(NgaTu)]
		if i < len(viTriXe) {
			vt = viTriXe[i]
		}
		kh := fmt.Sprintf("xe%d", i+1)
		cacO = append(cacO, kh)
		m.Kho.DatO(kh, Xe{Ma: int64(i + 1), TaiXe: ten, BienSo: tx[1], Mau: tx[2], ViTri: vt})
	}
	luat := &luatDatXe{trenXe: map[int64][]string{}}
	m.DatSo(MoiSoKiemToan(TS{{"trung_cho", "trùng chỗ"}}, luat))
	m.mu.Lock()
	m.cacO, m.lichSu, m.luat = cacO, nil, luat
	m.mu.Unlock()
}

func (m *MayChuDatXe) dsO() []string {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.cacO
}

// ------------------------------------------------------------ đặt xe / hủy

func (m *MayChuDatXe) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	that := k.SoNguoi == 0 // người dùng thật (không phải bộ mô phỏng): ghi nhật ký
	cacO := m.dsO()
	if tin["loai"] == "dat_xe" {
		don, den, okDon, okDen := [2]float64{0.5, 0.5}, [2]float64{0.9, 0.9}, true, true
		if v := tin["don"]; v != nil {
			don, okDon = diemNgaTu(v)
		}
		if v := tin["den"]; v != nil {
			den, okDen = diemNgaTu(v)
		}
		if !okDon || !okDen {
			traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "điểm đón/đến phải là ngã tư [x, y]"})
			return
		}
		xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc mọi xe, chọn xe trống gần nhất
			var chon *Xe
			for _, kh := range cacO {
				xe := doc[kh].(Xe)
				if xe.Khach == nguoi {
					return M{"ok": false, "ma_loi": "da_dat", "ly_do": "bạn đã đặt xe " + xe.TaiXe}, nil
				}
			}
			for _, kh := range cacO {
				xe := doc[kh].(Xe)
				if xe.Khach != "" {
					continue
				}
				if chon == nil || Km(xe.ViTri, don) < Km(chon.ViTri, don) ||
					(Km(xe.ViTri, don) == Km(chon.ViTri, don) && xe.Ma < chon.Ma) {
					x := xe
					chon = &x
				}
			}
			if chon == nil {
				return M{"ok": false, "ma_loi": "het_cho", "ly_do": "hết chỗ – không còn xe trống"}, nil
			}
			moi := *chon
			moi.Khach, moi.Don, moi.Den, moi.Gio = nguoi, don, den, time.Now().Format("15:04:05")
			moi.Cuoc, moi.Phut = Cuoc(don, den), phutDen(chon.ViTri, don)
			kq := M{"ok": true, "xe": chon.Ma, "khach": nguoi, "tai_xe": chon.TaiXe, "bien_so": chon.BienSo,
				"mau": chon.Mau, "phut": moi.Phut, "cuoc": moi.Cuoc}
			return kq, M{fmt.Sprintf("xe%d", chon.Ma): moi}
		}
		m.Kho.ThucHien(cacO, xuLy, traLoi, func(kq M) { m.daGhi("dat", kq, that) })
		return
	}
	xuLy := func(doc M) (M, M) {
		for _, kh := range cacO {
			xe := doc[kh].(Xe)
			if xe.Khach == nguoi {
				moi := Xe{Ma: xe.Ma, TaiXe: xe.TaiXe, BienSo: xe.BienSo, Mau: xe.Mau, ViTri: xe.ViTri}
				return M{"ok": true, "xe": xe.Ma, "khach": nguoi, "tai_xe": xe.TaiXe}, M{kh: moi}
			}
		}
		return M{"ok": false, "ma_loi": "chua_dat", "ly_do": "bạn chưa đặt xe"}, nil
	}
	m.Kho.ThucHien(cacO, xuLy, traLoi, func(kq M) { m.daGhi("huy", kq, that) })
}

// daGhi – gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi. kq["phien"] = số thứ
// tự lần ghi; cap_nhat mang "phien" lúc chụp -> client biết một cap_nhat chụp trước hay sau lần ghi của mình.
func (m *MayChuDatXe) daGhi(loai string, kq M, that bool) {
	p := m.demGhi.Add(1)
	m.phien.Store(p)
	kq["phien"] = p
	xe, khach, tx := kq["xe"].(int64), kq["khach"].(string), kq["tai_xe"].(string)
	m.So().Ghi(gdDatXe{loai, xe, khach})
	m.mu.Lock()
	m.lichSu = append([]M{{"gio": time.Now().Format("15:04:05"), "loai": loai, "xe": xe, "tai_xe": tx,
		"nguoi": khach}}, m.lichSu...)
	if len(m.lichSu) > 30 {
		m.lichSu = m.lichSu[:30]
	}
	m.mu.Unlock()
	if that {
		if loai == "dat" {
			m.ThongBao(fmt.Sprintf("Xe %d (%s) nhận khách %s", xe, tx, khach))
		} else {
			m.ThongBao(fmt.Sprintf("%s hủy chuyến, xe %d trống", khach, xe))
		}
	}
	m.BaoDoi()
}

func (m *MayChuDatXe) dinhKy(float64) {}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuDatXe) docXe() []Xe {
	var ds []Xe
	for _, kh := range m.dsO() {
		if x, co := m.Kho.Doc(kh).(Xe); co {
			ds = append(ds, x)
		}
	}
	return ds
}

func (m *MayChuDatXe) trangThai() M {
	phien := m.phien.Load() // đọc TRƯỚC các ô: ảnh chụp ít nhất mới tới lần ghi này
	ds := m.docXe()
	var xe []M
	trong := 0
	for _, x := range ds {
		xe = append(xe, x.json())
		if x.Khach == "" {
			trong++
		}
	}
	m.mu.Lock()
	ls := append([]M(nil), m.lichSu[:min(8, len(m.lichSu))]...)
	m.mu.Unlock()
	return M{"phien": phien, "so_xe": int64(len(ds)), "trong": int64(trong), "xe": xe, "lich_su": ls}
}

func (m *MayChuDatXe) tomTat() string {
	ds := m.docXe()
	da := 0
	for _, x := range ds {
		if x.Khach != "" {
			da++
		}
	}
	m.mu.Lock()
	luat := m.luat
	m.mu.Unlock()
	return fmt.Sprintf("Đã đặt %d/%d xe, %d lượt báo đặt được", da, len(ds), luat.soDat.Load())
}
