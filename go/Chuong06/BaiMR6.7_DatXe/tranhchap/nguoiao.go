package tranhchap

// NGHIỆP VỤ MR6.7 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – số xe, số xe còn trống mà một
// kết nối biết; nguoiAo – kịch bản của MỘT người dùng ảo; tham số đặt lại trước mỗi chế độ.

import "math/rand/v2"

// SoLanMacDinh – mỗi người bấm "Đặt xe" một lần.
const SoLanMacDinh = 1

// ThamSoDatLai – {} = giữ số xe hiện tại của máy chủ (ví dụ đã đặt bằng "khach … dat_lai so_xe=10").
var ThamSoDatLai = TS{}

// nguCanh – số xe còn trống mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ).
type nguCanh struct{ soXe, trong int64 }

func moiNguCanh(tt M) *nguCanh { return &nguCanh{SoNguyen(tt["so_xe"]), SoNguyen(tt["trong"])} }

func (nc *nguCanh) capNhat(tin M) { nc.soXe, nc.trong = SoNguyen(tin["so_xe"]), SoNguyen(tin["trong"]) }

func (nc *nguCanh) nhanKq(tin M) {
	if tin["ma_loi"] == "het_cho" {
		nc.trong = 0
	}
}

// nguoiAo – bấm "Đặt xe" từ một ngã tư ngẫu nhiên (máy chủ chọn xe trống gần nhất). Bị từ chối vì xung
// đột (lac_quan hết lượt thử) thì bấm lại, tối đa `lan` lần; "hết chỗ" hay đặt được thì thôi.
type nguoiAo struct {
	ten string
	nc  *nguCanh
	con int
}

func moiNguoiAo(ten string, nc *nguCanh, lan int) *nguoiAo { return &nguoiAo{ten, nc, lan} }

func (na *nguoiAo) dat() *YeuCau {
	if na.con <= 0 {
		return nil
	}
	na.con--
	i := rand.IntN(len(NgaTu))
	j := rand.IntN(len(NgaTu) - 1)
	if j >= i {
		j++
	}
	return &YeuCau{"dat_xe", TS{{"nguoi", na.ten}, {"don", diemJSON(NgaTu[i])}, {"den", diemJSON(NgaTu[j])}}}
}

func (na *nguoiAo) yeuCauDau() *YeuCau { return na.dat() }

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	if ok, _ := tin["ok"].(bool); ok || tin["ma_loi"] == "het_cho" || tin["ma_loi"] == "da_dat" {
		return nil
	}
	return na.dat()
}
