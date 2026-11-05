package tranhchap

// NGHIỆP VỤ MR6.6 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – giá hiện tại, bước giá mà một
// kết nối biết; nguoiAo – kịch bản của MỘT người dùng ảo; tham số đặt lại trước mỗi chế độ.

import "math/rand/v2"

// SoLanMacDinh – mỗi người trả giá 5 lần.
const SoLanMacDinh = 5

// ThamSoDatLai – mô phỏng: phiên không giới hạn thời gian.
var ThamSoDatLai = TS{{"thoi_gian_s", int64(0)}}

// nguCanh – giá hiện tại mà một kết nối biết – đọc từ cap_nhat và từ kq (có thể đã cũ khi tới máy chủ).
type nguCanh struct{ gia, buoc int64 }

func moiNguCanh(tt M) *nguCanh {
	return &nguCanh{gia: SoNguyen(tt["gia"]), buoc: SoNguyen(tt["mon"].(M)["buoc_gia"])}
}

func (nc *nguCanh) capNhat(tin M) { nc.gia = SoNguyen(tin["gia"]) }

func (nc *nguCanh) nhanKq(tin M) {
	if v, co := tin["gia"]; co {
		nc.gia = SoNguyen(v)
	}
}

// nguoiAo – trả giá `lan` lần; mỗi lần: giá hiện tại đã biết + bước giá × ngẫu nhiên 1–3.
type nguoiAo struct {
	ten string
	nc  *nguCanh
	con int
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo { return &nguoiAo{ten, nc, lan} }

func (na *nguoiAo) tra() *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	return &YeuCau{"tra_gia", TS{{"nguoi", na.ten}, {"gia", na.nc.gia + na.nc.buoc*int64(1+rand.IntN(3))}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.tra() }

func (na *nguoiAo) sauKq(M) *YeuCau { return na.tra() }
