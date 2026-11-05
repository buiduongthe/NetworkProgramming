package tranhchap

// NGHIỆP VỤ MR6.12 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – danh sách mã tài khoản (từ
// cap_nhat đầu tiên); nguoiAo – chuyển `lan` lần (mặc định 5) qua lại giữa một cặp tài khoản ngẫu
// nhiên: A→B, B→A, A→B… – nhiều người chuyển chéo cùng lúc nên hai tài nguyên luôn bị tranh theo hai
// chiều (điều kiện để khóa ngây thơ khóa chết).

import "math/rand/v2"

// SoLanMacDinh – mỗi người chuyển 5 lần.
const SoLanMacDinh = 5

// ThamSoDatLai – so sánh 4 chế độ: khóa theo thứ tự mã tài khoản.
var ThamSoDatLai = TS{{"khoa_ngay_tho", int64(0)}}

var MucTien = []int64{100_000, 200_000, 300_000, 500_000, 1_000_000, 2_000_000}

// nguCanh – danh sách mã tài khoản mà một kết nối biết.
type nguCanh struct{ ds []string }

func moiNguCanh(tt M) *nguCanh {
	nc := &nguCanh{}
	for _, t := range DSM(tt["tai_khoan"]) {
		nc.ds = append(nc.ds, Chuoi(t["ma"]))
	}
	return nc
}

func (nc *nguCanh) capNhat(M) {}

func (nc *nguCanh) nhanKq(M) {}

type nguoiAo struct {
	ten  string
	con  int
	a, b string
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo {
	p := rand.Perm(len(nc.ds))
	return &nguoiAo{ten, lan, nc.ds[p[0]], nc.ds[p[1]]}
}

func (na *nguoiAo) chuyen() *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	na.a, na.b = na.b, na.a // đổi chiều mỗi lần
	return &YeuCau{"chuyen", TS{{"nguoi", na.ten}, {"tu", na.a}, {"den", na.b}, {"so_tien", MucTien[rand.IntN(len(MucTien))]}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.chuyen() }

func (na *nguoiAo) sauKq(M) *YeuCau { return na.chuyen() }
