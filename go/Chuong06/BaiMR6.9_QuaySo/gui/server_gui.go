package main

// Bảng điều khiển máy chủ quay số. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI
// PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ khung
// "Tài nguyên": 10 món quà (ảnh, còn / đã trao cho ai), số quà còn lại, số lượt đã quay, số lượt trúng
// theo sổ kiểm toán, tham số đặt lại (số quà, xác suất p).

import (
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.9_QuaySo/tranhchap"
)

type cuaSoServerQuaySo struct {
	*cuaSoServerCoSo
	sv       *tc.MayChuQuaySo
	nhanCon  *canvas.Text
	nhanLuot *canvas.Text
	oQua     []*canvas.Image
	vienQua  []*canvas.Rectangle
	nhanQua  []*canvas.Text
	daVe     string
	oTS      map[string]*widget.Entry
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerQuaySo {
	s := &cuaSoServerQuaySo{oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ quay số MR6.9", "Tài nguyên: kho quà",
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

func (s *cuaSoServerQuaySo) veTaiNguyen() fyne.CanvasObject {
	s.nhanCon = chu("–", mau("#8b0000"), 17, true)
	s.nhanLuot = chu("–", Den, 12, false)
	luoi := container.New(layout.NewGridLayout(5))
	for i := range tc.TenQua {
		img := oAnhTinh(anhQua(i, 40, 34, 14, false, Trang), 40, 34)
		vien := canvas.NewRectangle(color.Transparent)
		vien.StrokeColor, vien.StrokeWidth = XanhDam, 2
		t := chu("", Den, 9, false)
		t.Alignment = fyne.TextAlignCenter
		s.oQua, s.vienQua, s.nhanQua = append(s.oQua, img), append(s.vienQua, vien), append(s.nhanQua, t)
		luoi.Add(container.NewVBox(container.NewCenter(coDinh(container.NewStack(img, vien), 40, 34)), t))
	}
	ts := s.sv.ThamSo()
	so, _ := ts.Lay("so_qua")
	p, _ := ts.Lay("p")
	s.oTS["so_qua"], s.oTS["p"] = oNhap(tc.StrPy(so)), oNhap(tc.FmtG(tc.SoThuc(p)))
	hang := container.NewHBox(widget.NewLabel("Số quà:"), coDinh(s.oTS["so_qua"], 44, 30),
		widget.NewLabel("Xác suất p:"), coDinh(s.oTS["p"], 80, 30))
	return container.NewVBox(s.nhanCon, s.nhanLuot, coDinh(luoi, 280, 120), hang)
}

func (s *cuaSoServerQuaySo) thamSoDatLai() (tc.TS, error) {
	n, e1 := strconv.ParseInt(strings.TrimSpace(s.oTS["so_qua"].Text), 10, 64)
	p, e2 := strconv.ParseFloat(strings.TrimSpace(s.oTS["p"].Text), 64)
	if e1 != nil || e2 != nil {
		return nil, fmt.Errorf("tham số phải là số")
	}
	return tc.T("so_qua", n, "p", p), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerQuaySo) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerQuaySo) capNhatTaiNguyen(tt, _ tc.M) {
	soQua, soTrung := tc.SoNguyen(tt["so_qua"]), tc.SoNguyen(tt["so_trung"])
	m := mau("#8b0000")
	if soTrung > soQua {
		m = DoDam
	}
	datChu(s.nhanCon, fmt.Sprintf("Còn %d/%d quà · trao %d", tc.SoNguyen(tt["con_lai"]), soQua, soTrung), m)
	datChu(s.nhanLuot, fmt.Sprintf("Đã quay %s lượt · p = %s", soNghin(tc.SoNguyen(tt["da_quay"])),
		tc.ChuP(tc.SoThuc(tt["p"]))), Den)
	qua := tc.DSM(tt["qua"])
	khoa := fmt.Sprint(soQua)
	for _, q := range qua {
		khoa += fmt.Sprint(q["con"])
	}
	if khoa != s.daVe {
		s.daVe = khoa
		for i, img := range s.oQua {
			if i < int(soQua) && i < len(qua) {
				con, _ := qua[i]["con"].(bool)
				nen, vien := Trang, XanhDam
				if !con {
					nen, vien = mau("#e4e4e4"), mau("#b0b0b0")
				}
				img.Image = anhQua(i, 40, 34, 14, !con, nen)
				img.Show()
				s.vienQua[i].StrokeColor = vien
			} else {
				img.Hide()
				s.vienQua[i].StrokeColor = mau("#b0b0b0")
			}
			img.Refresh()
			s.vienQua[i].Refresh()
		}
	}
	for i, t := range s.nhanQua {
		if i >= int(soQua) || i >= len(qua) {
			datChu(t, "", Den)
			continue
		}
		q := qua[i]
		if con, _ := q["con"].(bool); con {
			datChu(t, tc.Chuoi(q["ten"]), Den)
		} else {
			ai, _ := q["nguoi"].(string)
			if ai == "" {
				ai = "?"
			}
			datChu(t, "→"+ai, mau("#a00000"))
		}
	}
}
