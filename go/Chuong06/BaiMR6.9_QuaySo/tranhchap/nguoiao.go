package tranhchap

// NGHIỆP VỤ MR6.9 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – số quà còn lại mà một kết nối
// biết; nguoiAo – kịch bản của MỘT người dùng ảo: quay `lan` lần (mặc định 1). Người thứ 100, 200, 300…
// dùng TÊN của người liền trước (một người quay trên hai thiết bị cùng lúc) – máy chủ phải từ chối lượt
// thứ hai ("da_quay"); lan > 1 thì các lượt sau cũng phải bị từ chối.

import (
	"fmt"
	"strconv"
)

// SoLanMacDinh – mỗi người quay 1 lần.
const SoLanMacDinh = 1

// ThamSoDatLai – {} = giữ số quà, p hiện tại của máy chủ (đặt bằng "server --p", "khach … dat_lai p=…",
// hoặc ô tham số của server_gui).
var ThamSoDatLai = TS{}

// HaiThietBi – người thứ 100, 200, … quay bằng tên người liền trước.
const HaiThietBi = 100

// nguCanh – số quà còn lại mà một kết nối biết (chỉ để tham khảo – máy chủ mới quyết định).
type nguCanh struct{ conLai int64 }

func moiNguCanh(tt M) *nguCanh { return &nguCanh{SoNguyen(tt["con_lai"])} }

func (nc *nguCanh) capNhat(tin M) { nc.conLai = SoNguyen(tin["con_lai"]) }

func (nc *nguCanh) nhanKq(M) {}

// nguoiAo – quay `lan` lần; người thứ 100, 200, … mang tên người liền trước (thiết bị thứ hai).
type nguoiAo struct {
	ten string
	nc  *nguCanh
	con int
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo {
	if i, err := strconv.Atoi(ten[1:]); err == nil && i%HaiThietBi == 0 {
		ten = fmt.Sprintf("u%d", i-1)
	}
	return &nguoiAo{ten, nc, lan}
}

func (na *nguoiAo) quay() *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	return &YeuCau{"quay", TS{{"nguoi", na.ten}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.quay() }

func (na *nguoiAo) sauKq(M) *YeuCau { return na.quay() }
