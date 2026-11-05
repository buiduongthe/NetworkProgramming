package tranhchap

// NGHIỆP VỤ MR6.10 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – danh sách mã lớp (từ cap_nhat đầu
// tiên); nguoiAo – kịch bản của MỘT sinh viên ảo: muốn `lan` lớp (mặc định 1). Thứ tự ưu tiên các lớp
// xáo ngẫu nhiên theo tên (cùng tên -> cùng thứ tự); đăng ký lớp đầu, "lớp đã đủ" thì thử lớp kế tiếp;
// được nhận thì 10 % "đổi ý" hủy ngay lớp đó (nhả chỗ cho người khác). Sinh viên thứ 100, 200, … dùng
// TÊN của người liền trước (một sinh viên bấm trên hai thiết bị cùng lúc) – máy chủ phải từ chối lượt
// thứ hai ("da_dang_ky").

import (
	"fmt"
	"hash/fnv"
	"math/rand/v2"
	"strconv"
)

// SoLanMacDinh – mỗi sinh viên muốn 1 lớp.
const SoLanMacDinh = 1

// ThamSoDatLai – {} = giữ số lớp, sĩ số hiện tại của máy chủ.
var ThamSoDatLai = TS{}

const (
	HaiThietBi = 100 // sinh viên thứ 100, 200, … dùng tên người liền trước
	TiLeHuy    = 0.1 // được nhận rồi hủy ngay ("đổi ý")
)

// nguCanh – danh sách mã lớp mà một kết nối biết.
type nguCanh struct{ maLop []string }

func moiNguCanh(tt M) *nguCanh {
	nc := &nguCanh{}
	for _, x := range DSM(tt["lop"]) {
		nc.maLop = append(nc.maLop, Chuoi(x["ma"]))
	}
	return nc
}

func (nc *nguCanh) capNhat(M) {}

func (nc *nguCanh) nhanKq(M) {}

// nguoiAo – đăng ký đủ `lan` lớp theo thứ tự ưu tiên; lớp đủ thì thử lớp sau; 10 % hủy ngay lớp vừa được.
type nguoiAo struct {
	ten     string
	thuTu   []string
	can     int
	doiY    bool
	dangHuy bool
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo {
	if i, err := strconv.Atoi(ten[1:]); err == nil && i%HaiThietBi == 0 {
		ten = fmt.Sprintf("u%d", i-1)
	}
	h := fnv.New64a() // cùng tên -> cùng thứ tự ưu tiên
	h.Write([]byte(ten))
	ngauNhien := rand.New(rand.NewPCG(h.Sum64(), 0x4d52363130))
	thuTu := append([]string(nil), nc.maLop...)
	ngauNhien.Shuffle(len(thuTu), func(i, j int) { thuTu[i], thuTu[j] = thuTu[j], thuTu[i] })
	return &nguoiAo{ten: ten, thuTu: thuTu, can: lan, doiY: ngauNhien.Float64() < TiLeHuy}
}

func (na *nguoiAo) tiep() *YeuCau {
	if na.can <= 0 || len(na.thuTu) == 0 {
		return nil
	}
	ma := na.thuTu[0]
	na.thuTu = na.thuTu[1:]
	return &YeuCau{"dang_ky", TS{{"nguoi", na.ten}, {"ma_lop", ma}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.tiep() }

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	if na.dangHuy { // vừa hủy xong: thôi
		return nil
	}
	if ok, _ := tin["ok"].(bool); ok {
		if na.doiY {
			na.dangHuy = true
			return &YeuCau{"huy", TS{{"nguoi", na.ten}, {"ma_lop", Chuoi(tin["ma_lop"])}}}
		}
		na.can--
	}
	return na.tiep()
}
