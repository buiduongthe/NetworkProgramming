package main

// Bảng điều khiển máy chủ flash sale. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI
// PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ khung
// "Tài nguyên": ảnh sản phẩm, tồn kho (số lớn + thanh đã bán), đã bán theo kho và theo sổ kiểm toán (đỏ khi
// lệch hoặc vượt), đồng hồ mở bán, vài đơn gần nhất và tham số đặt lại (tồn đầu kỳ, mở bán sau bao giây).

import (
	"fmt"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.11_FlashSale/tranhchap"
)

type cuaSoServerFlashSale struct {
	*cuaSoServerCoSo
	sv      *tc.MayChuFlashSale
	nhanSP  *canvas.Text
	nhanTon *canvas.Text
	thanh   *canvas.Rectangle
	nhanBan *canvas.Text
	nhanMo  *canvas.Text
	ls      *dsChu
	oTS     map[string]*widget.Entry
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerFlashSale {
	s := &cuaSoServerFlashSale{oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ flash sale MR6.11", "Tài nguyên: kho hàng",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, nil)
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 10000, 200, tc.SoLanMacDinh)
	return s
}

func (s *cuaSoServerFlashSale) veTaiNguyen() fyne.CanvasObject {
	anh := oAnhTinh(anhTaiNghe(120, 112, 60, 48, 0.55, mau("#fff1e6")), 120, 112)
	s.nhanSP = chu("–", Den, 11, true)
	s.nhanTon = chu("–", mau("#8b0000"), 21, true)
	nen := canvas.NewRectangle(mau("#dddddd"))
	s.thanh = canvas.NewRectangle(camSale)
	thanh := container.NewWithoutLayout(nen, s.thanh)
	nen.Resize(fyne.NewSize(180, 14))
	s.thanh.Resize(fyne.NewSize(0, 14))
	s.nhanBan = chu("–", Den, 12, false)
	s.nhanMo = chu("–", Den, 14, true)
	s.nhanMo.TextStyle.Monospace = true
	cot := container.NewVBox(s.nhanSP, s.nhanTon, coDinh(thanh, 180, 14), s.nhanBan, s.nhanMo)
	s.ls = moiDsChu(3, 10, 250, 56)
	ts := s.sv.ThamSo()
	ton, _ := ts.Lay("ton_dau")
	mo, _ := ts.Lay("mo_sau_s")
	s.oTS["ton_dau"], s.oTS["mo_sau_s"] = oNhap(tc.StrPy(ton)), oNhap(tc.FmtG(tc.SoThuc(mo)))
	hang := container.NewHBox(widget.NewLabel("Tồn đầu kỳ:"), coDinh(s.oTS["ton_dau"], 64, 30),
		widget.NewLabel("Mở bán sau (s):"), coDinh(s.oTS["mo_sau_s"], 50, 30))
	return container.NewVBox(container.NewHBox(anh, cot), s.ls.O, hang)
}

func (s *cuaSoServerFlashSale) thamSoDatLai() (tc.TS, error) {
	t, e1 := strconv.ParseInt(strings.TrimSpace(s.oTS["ton_dau"].Text), 10, 64)
	m, e2 := strconv.ParseFloat(strings.TrimSpace(s.oTS["mo_sau_s"].Text), 64)
	if e1 != nil || e2 != nil {
		return nil, fmt.Errorf("tham số phải là số")
	}
	return tc.T("ton_dau", t, "mo_sau_s", m), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerFlashSale) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerFlashSale) capNhatTaiNguyen(tt, _ tc.M) {
	sp, _ := tt["sp"].(tc.M)
	datChu(s.nhanSP, tc.Chuoi(sp["ten"]), Den)
	tonDau, ton := tc.SoNguyen(tt["ton_dau"]), tc.SoNguyen(tt["ton"])
	datChu(s.nhanTon, fmt.Sprintf("Tồn %d/%d", ton, tonDau), mau("#8b0000"))
	tiLe := min(1.0, float64(tonDau-ton)/float64(tonDau))
	m := camSale
	if tiLe >= 0.8 {
		m = doSale
	}
	s.thanh.FillColor = m
	s.thanh.Resize(fyne.NewSize(float32(180*max(0, tiLe)), 14))
	s.thanh.Refresh()
	daBan, theoSo := tc.SoNguyen(tt["da_ban"]), tc.SoNguyen(tt["ban_theo_so"])
	mBan := Den
	if theoSo > tonDau || theoSo != daBan {
		mBan = DoDam
	}
	datChu(s.nhanBan, fmt.Sprintf("Đã bán %d (sổ: %d) · %d đơn", daBan, theoSo, tc.SoNguyen(tt["so_don"])), mBan)
	switch mo, _ := tt["mo_ban"].(bool); {
	case !mo:
		g := tc.SoNguyen(tt["mo_con_ms"]) / 1000
		datChu(s.nhanMo, fmt.Sprintf("Mở bán sau 00:%02d:%02d", g/60, g%60), mau("#a06000"))
	case ton <= 0:
		datChu(s.nhanMo, "HẾT HÀNG", XamChu)
	default:
		datChu(s.nhanMo, "ĐANG MỞ BÁN", XanhDam)
	}
	var ds []string
	for i, x := range tc.DSM(tt["don_gan"]) {
		if i < 3 {
			ds = append(ds, fmt.Sprintf("%s %-6s ×%d", tc.Chuoi(x["gio"]), tc.Chuoi(x["nguoi"]), tc.SoNguyen(x["so_luong"])))
		}
	}
	s.ls.dat(ds, nil)
}
