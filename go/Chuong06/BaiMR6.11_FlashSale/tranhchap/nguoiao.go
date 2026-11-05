package tranhchap

// NGHIỆP VỤ MR6.11 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – số sản phẩm tối đa mỗi đơn (tồn
// kho do máy chủ quyết định); nguoiAo – đặt `lan` đơn, mỗi đơn 1–2 sản phẩm; "chỉ còn k" thì mua đúng k;
// hết hàng thì thôi. Trước mỗi chế độ, so_sanh đặt lại với mo_sau_s = 0: mở bán ngay, RÀO XUẤT PHÁT
// chính là "giờ G".

import "math/rand/v2"

// SoLanMacDinh – mỗi người đặt 1 đơn.
const SoLanMacDinh = 1

// ThamSoDatLai – mô phỏng: mở bán ngay khi đặt lại (rào xuất phát = giờ G).
var ThamSoDatLai = TS{{"mo_sau_s", int64(0)}}

// nguCanh – không cần trạng thái chung: tồn kho do máy chủ quyết định.
type nguCanh struct{ toiDa int }

func moiNguCanh(tt M) *nguCanh {
	sp, _ := tt["sp"].(M)
	return &nguCanh{int(SoNguyen(sp["toi_da"]))}
}

func (nc *nguCanh) capNhat(M) {}

func (nc *nguCanh) nhanKq(M) {}

type nguoiAo struct {
	ten   string
	con   int
	toiDa int
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo {
	return &nguoiAo{ten, lan, max(1, min(2, nc.toiDa))}
}

func (na *nguoiAo) mua(sl int64) *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	if sl == 0 {
		sl = int64(1 + rand.IntN(na.toiDa))
	}
	return &YeuCau{"mua", TS{{"nguoi", na.ten}, {"so_luong", sl}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.mua(0) }

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	if tin["ma_loi"] == "het_hang" {
		return nil
	}
	if ton := SoNguyen(tin["ton"]); tin["ma_loi"] == "khong_du" && ton > 0 {
		na.con++ // đơn hỏng không tính: mua lại phần còn lại
		return na.mua(ton)
	}
	return na.mua(0)
}
