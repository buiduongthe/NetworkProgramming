package tranhchap

// SoKiemToan – ghi mọi giao dịch ĐƯỢC CHẤP NHẬN (dưới khóa riêng của sổ – không ảnh hưởng tranh chấp
// của kho) và đếm sai phạm (khung chung MR6.6–MR6.14). Nghiệp vụ đặt danh sách TenSaiPham và viết hai
// luật (giao diện LuatKiemToan):
//
//	KiemTra(so, gd)        – gọi khi ghi từng giao dịch (theo thứ tự ghi sổ), gọi so.SaiPham(ma, moTa);
//	KiemTraCuoi(so, doc)   – đối chiếu sổ với trạng thái hiện tại của kho, trả về {ma: số}.

import "sync"

type LuatKiemToan interface {
	KiemTra(so *SoKiemToan, gd any)
	KiemTraCuoi(so *SoKiemToan, doc M) map[string]int64
}

type SoKiemToan struct {
	mu         sync.Mutex
	TenSaiPham TS // mã -> tên ngắn (hiện trong bảng, dòng in), giữ thứ tự
	luat       LuatKiemToan
	SoGiaoDich int64
	dem        map[string]int64
	viDu       []string // mô tả vài sai phạm đầu tiên
}

func MoiSoKiemToan(ten TS, luat LuatKiemToan) *SoKiemToan {
	s := &SoKiemToan{TenSaiPham: ten, luat: luat, dem: map[string]int64{}}
	for _, kv := range ten {
		s.dem[kv.K] = 0
	}
	return s
}

// Ghi – một giao dịch được chấp nhận (gọi trong khiGhi, tức là trong vùng găng của chế độ).
func (s *SoKiemToan) Ghi(gd any) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.SoGiaoDich++
	s.luat.KiemTra(s, gd)
}

// SaiPham – gọi từ luật (đang giữ khóa sổ).
func (s *SoKiemToan) SaiPham(ma, moTa string) {
	s.dem[ma]++
	if len(s.viDu) < 5 {
		s.viDu = append(s.viDu, moTa)
	}
}

// TongKet – (danh sách {"ma","ten","so"}, tổng số sai phạm, vài ví dụ).
func (s *SoKiemToan) TongKet(doc M) ([]M, int64, []string) {
	s.mu.Lock()
	dem := map[string]int64{}
	for k, v := range s.dem {
		dem[k] = v
	}
	for ma, so := range s.luat.KiemTraCuoi(s, doc) {
		dem[ma] += so
	}
	viDu := append([]string(nil), s.viDu...)
	s.mu.Unlock()
	var ds []M
	var tong int64
	for _, kv := range s.TenSaiPham {
		ds = append(ds, M{"ma": kv.K, "ten": kv.V, "so": dem[kv.K]})
		tong += dem[kv.K]
	}
	return ds, tong, viDu
}
