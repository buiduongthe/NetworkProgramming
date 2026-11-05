package tranhchap

// NGHIỆP VỤ MR6.8 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – sơ đồ ghế, ghế "nóng" mà một
// kết nối biết; nguoiAo – kịch bản của MỘT người dùng ảo; tham số đặt lại trước mỗi chế độ.

import (
	"fmt"
	"math/rand/v2"
)

// SoLanMacDinh – số lần thử giữ chỗ của mỗi người.
const SoLanMacDinh = 1

// ThamSoDatLai – {} = giữ tham số hiện tại của máy chủ (ví dụ "khach … dat_lai ghe_nong=" để mọi người
// chọn ghế ngẫu nhiên).
var ThamSoDatLai = TS{}

// nguCanh – sơ đồ ghế mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
type nguCanh struct {
	hang    string
	cot     int
	gheNong string
	ban     map[string]bool // ghế không trống (đang giữ / đã bán)
}

func moiNguCanh(tt M) *nguCanh {
	nc := &nguCanh{hang: Chuoi(tt["hang"]), cot: int(SoNguyen(tt["cot"])), gheNong: Chuoi(tt["ghe_nong"])}
	nc.capNhat(tt)
	return nc
}

func (nc *nguCanh) capNhat(tin M) {
	nc.ban = map[string]bool{}
	soDo := DSChuoi(tin["so_do"])
	for i, r := range []rune(nc.hang) {
		if i >= len(soDo) {
			break
		}
		for c, x := range soDo[i] {
			if x != '.' {
				nc.ban[fmt.Sprintf("%c%d", r, c+1)] = true
			}
		}
	}
}

func (nc *nguCanh) nhanKq(tin M) {
	if ok, _ := tin["ok"].(bool); !ok {
		for _, g := range DSChuoi(tin["ghe_ban"]) {
			nc.ban[g] = true
		}
	}
}

// chon – một nhóm 1–2 ghế liền nhau ngẫu nhiên, còn trống theo những gì kết nối này biết.
func (nc *nguCanh) chon() []string {
	so := 1 + rand.IntN(2)
	var nhom [][]string
	for _, r := range nc.hang {
		for c := 1; c <= nc.cot-so+1; c++ {
			var x []string
			trong := true
			for i := 0; i < so; i++ {
				g := fmt.Sprintf("%c%d", r, c+i)
				x = append(x, g)
				trong = trong && !nc.ban[g]
			}
			if trong {
				nhom = append(nhom, x)
			}
		}
	}
	if len(nhom) > 0 {
		return nhom[rand.IntN(len(nhom))]
	}
	h := []rune(nc.hang)
	return []string{fmt.Sprintf("%c%d", h[rand.IntN(len(h))], 1+rand.IntN(nc.cot))}
}

// nguoiAo – lần đầu giữ ghế nóng (ví dụ E7; nếu máy chủ không đặt ghế nóng thì chọn nhóm ghế trống); giữ
// được thì thanh toán ngay; bị từ chối thì chọn nhóm ghế trống khác, tối đa `lan` lần giữ chỗ.
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
	var ghe []string
	if na.lanDau && na.nc.gheNong != "" {
		ghe = []string{na.nc.gheNong}
	} else {
		ghe = na.nc.chon()
	}
	na.lanDau = false
	return &YeuCau{"giu_cho", TS{{"nguoi", na.ten}, {"ghe", ghe}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.giu() }

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	ok, _ := tin["ok"].(bool)
	if _, co := tin["het_han_ms"]; ok && co { // giữ được: thanh toán ngay
		return &YeuCau{"thanh_toan", TS{{"nguoi", na.ten}, {"ghe", DSChuoi(tin["ghe"])}}}
	}
	if ok { // đã thanh toán: xong
		return nil
	}
	return na.giu()
}
