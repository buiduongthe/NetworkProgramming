package tranhchap

// NGHIỆP VỤ MR6.14 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – chỗ nào bận ở chặng nào, ghế "nóng"
// mà một kết nối biết; nguoiAo – kịch bản của MỘT người dùng ảo; tham số đặt lại trước mỗi chế độ.

import (
	"fmt"
	"math/rand/v2"
)

// SoLanMacDinh – số lần thử giữ chỗ của mỗi người.
const SoLanMacDinh = 1

// ThamSoDatLai – {} = giữ tham số hiện tại của máy chủ (loại xe, ghế nóng, TTL).
var ThamSoDatLai = TS{}

// nguCanh – chỗ nào bận ở chặng nào mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
type nguCanh struct {
	ghe     []string
	soTram  int
	gheNong string
	ban     map[string]bool // ô "chỗ#chặng" không trống (đang giữ / đã bán)
}

func moiNguCanh(tt M) *nguCanh {
	nc := &nguCanh{ghe: DSChuoi(tt["ghe"]), soTram: len(DSChuoi(tt["tram"])), gheNong: Chuoi(tt["ghe_nong"])}
	nc.capNhat(tt)
	return nc
}

func (nc *nguCanh) capNhat(tin M) {
	nc.ban = map[string]bool{}
	soDo := DSChuoi(tin["so_do"])
	for i, g := range nc.ghe {
		if i >= len(soDo) {
			break
		}
		for k, x := range soDo[i] {
			if x != '.' {
				nc.ban[fmt.Sprintf("%s#%d", g, k)] = true
			}
		}
	}
}

func (nc *nguCanh) nhanKq(tin M) {
	if ok, _ := tin["ok"].(bool); !ok {
		if _, co := tin["tu"]; co {
			for _, o := range oCua(DSChuoi(tin["ghe_ban"]), int(SoNguyen(tin["tu"])), int(SoNguyen(tin["den"]))) {
				nc.ban[o] = true
			}
		}
	}
}

// doan – một đoạn đường ngẫu nhiên [tu, den).
func (nc *nguCanh) doan() (int, int) {
	tu := rand.IntN(nc.soTram - 1)
	return tu, tu + 1 + rand.IntN(nc.soTram-1-tu)
}

// chon – 1–2 chỗ liền nhau (theo thứ tự mã) còn trống cả đoạn [tu, den) theo những gì kết nối này biết.
func (nc *nguCanh) chon(tu, den int) []string {
	so := 1 + rand.IntN(2)
	var nhom [][]string
	for i := 0; i+so <= len(nc.ghe); i++ {
		x := nc.ghe[i : i+so]
		trong := true
		for _, o := range oCua(x, tu, den) {
			trong = trong && !nc.ban[o]
		}
		if trong {
			nhom = append(nhom, append([]string(nil), x...))
		}
	}
	if len(nhom) > 0 {
		return nhom[rand.IntN(len(nhom))]
	}
	return []string{nc.ghe[rand.IntN(len(nc.ghe))]}
}

// nguoiAo – lần đầu giữ ghế nóng (ví dụ D01) cho một đoạn ngẫu nhiên (nếu máy chủ không đặt ghế nóng thì
// chọn chỗ trống); giữ được thì thanh toán ngay; bị từ chối thì đổi đoạn và chọn chỗ trống khác, tối đa
// `lan` lần giữ chỗ. Vì nhiều đoạn không chồng nhau, ghế nóng có thể bán đúng cho 2–3 người.
type nguoiAo struct {
	ten    string
	nc     *nguCanh
	con    int
	lanDau bool
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo { return &nguoiAo{ten, nc, lan, true} }

func (na *nguoiAo) giu() *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	tu, den := na.nc.doan()
	var ghe []string
	if na.lanDau && na.nc.gheNong != "" {
		ghe = []string{na.nc.gheNong}
	} else {
		ghe = na.nc.chon(tu, den)
	}
	na.lanDau = false
	return &YeuCau{"giu_cho", TS{{"nguoi", na.ten}, {"ghe", ghe}, {"tu", int64(tu)}, {"den", int64(den)}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.giu() }

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	ok, _ := tin["ok"].(bool)
	if _, co := tin["het_han_ms"]; ok && co { // giữ được: thanh toán ngay
		return &YeuCau{"thanh_toan", TS{{"nguoi", na.ten}, {"ghe", DSChuoi(tin["ghe"])}, {"tu", SoNguyen(tin["tu"])},
			{"den", SoNguyen(tin["den"])}}}
	}
	if ok { // đã thanh toán: xong
		return nil
	}
	return na.giu()
}
