package tranhchap

// NGHIỆP VỤ MR6.10 – lõi máy chủ đăng ký học phần + sổ kiểm toán (khung ở các tệp còn lại của gói).
//
// Cổng đăng ký học phần ngày mở đăng ký: vài lớp học phần (mặc định 3 lớp, sĩ số 50), hàng nghìn sinh
// viên cùng bấm "Đăng ký". Hai thao tác ghi:
//
//	{"loai":"dang_ky","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
//	{"loai":"huy","ma_yc":k,"ma_lop":"123013-01"[,"nguoi":"u17"]}
//
// Luật: lớp còn chỗ (số đã đăng ký < sĩ số) mới nhận; sinh viên đã có trong lớp thì không đăng ký lại
// ("da_dang_ky"); chỉ hủy được lớp mình đã đăng ký ("chua_dang_ky").
//
// Mỗi lớp là MỘT ô của KhoTranhChap: lopHP{sv: tập tên sinh viên}; ghi = tạo tập mới (xuLy thuần). Khóa
// theo tài nguyên: hai lớp khác nhau được xử lý song song; một lớp "hot" là điểm nóng.
//
// Sổ kiểm toán (danh sách lớp theo sổ, ghi trong vùng găng) đếm hai sai phạm:
//
//	vuot_si_so – một lượt đăng ký được chấp nhận khi lớp (theo sổ) đã đủ sĩ số;
//	dk_trung   – một sinh viên được chấp nhận đăng ký một lớp mà sổ ghi là đã có tên (hai thiết bị cùng
//	             bấm, hoặc bấm đúp, cùng đọc "chưa có tên").

import (
	"fmt"
	"sort"
)

const TenMayChu = "Đăng ký học phần MR6.10"

var LoaiYeuCau = []string{"dang_ky", "huy"}

var ThamSoMacDinh = TS{{"so_lop", int64(3)}, {"si_so", int64(50)}}

// LopHP – một dòng danh mục: mã lớp, tên học phần, giảng viên, lịch học, số tín chỉ.
type LopHP struct {
	Ma, Ten, GV, Lich string
	TC                int64
}

var DanhMuc = []LopHP{
	{"123013-01", "Lập trình mạng", "TS. Lê Hoàng Nam", "T2, tiết 1–3, E301", 3},
	{"123005-01", "Quản trị mạng", "ThS. Phạm Thu Hà", "T3, tiết 4–6, E302", 3},
	{"006124-01", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T4, tiết 7–9, E303", 2},
	{"123013-02", "Lập trình mạng", "TS. Lê Hoàng Nam", "T5, tiết 1–3, E301", 3},
	{"123005-02", "Quản trị mạng", "ThS. Phạm Thu Hà", "T6, tiết 4–6, E302", 3},
	{"006124-02", "Tin học cơ bản", "ThS. Vũ Minh Đức", "T7, tiết 7–9, E303", 2},
}

// lopO – giá trị ô "lop:<mã>": tập sinh viên (không sửa tại chỗ – mỗi lần ghi tạo tập mới).
type lopO struct{ sv map[string]bool }

// ------------------------------------------------------------ sổ kiểm toán

type gdDangKy struct{ thaoTac, nguoi, maLop string }

type luatDangKy struct {
	siSo int
	lop  map[string]map[string]bool // mã lớp -> sinh viên theo sổ
}

func (l *luatDangKy) KiemTra(so *SoKiemToan, x any) {
	gd := x.(gdDangKy)
	ds := l.lop[gd.maLop]
	if ds == nil {
		ds = map[string]bool{}
		l.lop[gd.maLop] = ds
	}
	if gd.thaoTac == "huy" {
		delete(ds, gd.nguoi)
		return
	}
	if ds[gd.nguoi] {
		so.SaiPham("dk_trung", fmt.Sprintf("%s ĐK lại %s", gd.nguoi, gd.maLop))
	} else if len(ds) >= l.siSo {
		so.SaiPham("vuot_si_so", fmt.Sprintf("%s: %s là SV thứ %d", gd.maLop, gd.nguoi, len(ds)+1))
	}
	ds[gd.nguoi] = true
}

func (l *luatDangKy) KiemTraCuoi(*SoKiemToan, M) map[string]int64 { return nil }

// soLop, tongSo – đọc sổ (dưới khóa của sổ).
func (m *MayChuDangKy) soLop(ma string) int64 {
	so := m.So()
	so.mu.Lock()
	defer so.mu.Unlock()
	return int64(len(so.luat.(*luatDangKy).lop[ma]))
}

func (m *MayChuDangKy) tongSo() int64 {
	so := m.So()
	so.mu.Lock()
	defer so.mu.Unlock()
	var n int64
	for _, x := range so.luat.(*luatDangKy).lop {
		n += int64(len(x))
	}
	return n
}

// ------------------------------------------------------------ máy chủ

type MayChuDangKy struct{ *MayChuCoSo }

func MoiMayChu(cheDo string, treMs float64, bao func(string), thamSo TS) (*MayChuDangKy, error) {
	co, err := MoiMayChuCoSo(TenMayChu, LoaiYeuCau, ThamSoMacDinh, cheDo, treMs, bao)
	if err != nil {
		return nil, err
	}
	m := &MayChuDangKy{co}
	return m, co.Gan(m, thamSo)
}

// ------------------------------------------------------------ tạo / đặt lại đợt đăng ký

func (m *MayChuDangKy) kiemThamSoNV(ts TS) error {
	if v, co := ts.Lay("so_lop"); co {
		if n := SoNguyen(v); n < 1 || n > int64(len(DanhMuc)) {
			return fmt.Errorf("số lớp 1–%d", len(DanhMuc))
		}
	}
	if v, co := ts.Lay("si_so"); co {
		if n := SoNguyen(v); n < 1 || n > 500 {
			return fmt.Errorf("sĩ số 1–500")
		}
	}
	return nil
}

// dsLop – các lớp của đợt hiện tại.
func (m *MayChuDangKy) dsLop() []LopHP { return DanhMuc[:m.TSInt("so_lop")] }

func (m *MayChuDangKy) khoiTao() {
	for _, l := range m.dsLop() {
		m.Kho.DatO("lop:"+l.Ma, lopO{map[string]bool{}})
	}
	m.DatSo(MoiSoKiemToan(TS{{"vuot_si_so", "vượt sĩ số"}, {"dk_trung", "ĐK trùng"}},
		&luatDangKy{siSo: int(m.TSInt("si_so")), lop: map[string]map[string]bool{}}))
}

// ------------------------------------------------------------ đăng ký / hủy

func (m *MayChuDangKy) xuLyYeuCau(k *KhachKetNoi, nguoi string, tin M, traLoi func(M)) {
	ma, _ := tin["ma_lop"].(string)
	co := false
	for _, l := range m.dsLop() {
		co = co || l.Ma == ma
	}
	if !co {
		traLoi(M{"ok": false, "ma_loi": "sai_du_lieu", "ly_do": "không có lớp " + StrPy(tin["ma_lop"])})
		return
	}
	kh, siSo, thaoTac := "lop:"+ma, int(m.TSInt("si_so")), tin["loai"].(string)
	xuLy := func(doc M) (M, M) { // hàm THUẦN: đọc doc, trả về (kq, ghi)
		sv := doc[kh].(lopO).sv
		moi := make(map[string]bool, len(sv)+1)
		for x := range sv {
			moi[x] = true
		}
		if thaoTac == "dang_ky" {
			if sv[nguoi] {
				return M{"ok": false, "ma_loi": "da_dang_ky", "ly_do": "đã đăng ký lớp này", "ma_lop": ma,
					"da_dk": int64(len(sv))}, nil
			}
			if len(sv) >= siSo {
				return M{"ok": false, "ma_loi": "lop_day", "ly_do": "lớp đã đủ", "ma_lop": ma, "da_dk": int64(len(sv))}, nil
			}
			moi[nguoi] = true
		} else {
			if !sv[nguoi] {
				return M{"ok": false, "ma_loi": "chua_dang_ky", "ly_do": "chưa đăng ký lớp này", "ma_lop": ma,
					"da_dk": int64(len(sv))}, nil
			}
			delete(moi, nguoi)
		}
		return M{"ok": true, "thao_tac": thaoTac, "ma_lop": ma, "nguoi": nguoi, "da_dk": int64(len(moi))},
			M{kh: lopO{moi}}
	}
	gop := k.SoNguoi > 0
	m.Kho.ThucHien([]string{kh}, xuLy, traLoi, func(kq M) { m.daGhi(kq, gop) })
}

// daGhi – gọi trong vùng găng ngay sau khi ghi: ghi sổ kiểm toán đúng thứ tự ghi.
func (m *MayChuDangKy) daGhi(kq M, gop bool) {
	thaoTac, nguoi, ma := kq["thao_tac"].(string), kq["nguoi"].(string), kq["ma_lop"].(string)
	m.So().Ghi(gdDangKy{thaoTac, nguoi, ma})
	if !gop { // người dùng thật: báo lên nhật ký
		viec := "hủy"
		if thaoTac == "dang_ky" {
			viec = "đăng ký"
		}
		m.ThongBao(fmt.Sprintf("%s %s %s (%d/%d)", nguoi, viec, ma, SoNguyen(kq["da_dk"]), m.TSInt("si_so")))
	}
	m.BaoDoi()
}

func (m *MayChuDangKy) dinhKy(float64) {}

// ------------------------------------------------------------ trạng thái gửi client

func (m *MayChuDangKy) trangThai() M {
	siSo := m.TSInt("si_so")
	var ds []M
	for _, l := range m.dsLop() {
		var ten []string
		if o, la := m.Kho.Doc("lop:" + l.Ma).(lopO); la {
			for x := range o.sv {
				ten = append(ten, x)
			}
		}
		sort.Strings(ten)
		if ten == nil {
			ten = []string{}
		}
		ds = append(ds, M{"ma": l.Ma, "ten": l.Ten, "gv": l.GV, "lich": l.Lich, "tc": l.TC, "si_so": siSo,
			"da_dk": int64(len(ten)), "theo_so": m.soLop(l.Ma), "sv": ten})
	}
	return M{"si_so": siSo, "lop": ds}
}

func (m *MayChuDangKy) tomTat() string {
	soLop, siSo := m.TSInt("so_lop"), m.TSInt("si_so")
	return fmt.Sprintf("Đã ĐK %d/%d chỗ (%d lớp × %d)", m.tongSo(), soLop*siSo, soLop, siSo)
}
