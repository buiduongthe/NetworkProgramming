package main

// Bảng điều khiển máy chủ đặt xe. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI
// PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ khung
// "Tài nguyên": bản đồ thu nhỏ với các xe (xe đã có khách mờ đi, ghi tên khách), số xe trống, danh
// sách xe và tham số đặt lại (số xe).

import (
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.7_DatXe/tranhchap"
)

type cuaSoServerDatXe struct {
	*cuaSoServerCoSo
	sv        *tc.MayChuDatXe
	bd        *banDo
	nhanTrong *canvas.Text
	ls        *dsChu
	oSoXe     *widget.Entry
	veCu      string
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerDatXe {
	s := &cuaSoServerDatXe{}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ đặt xe MR6.7", "Tài nguyên: các xe (mỗi xe 1 chỗ)",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, tc.T("so_xe", int64(1)))
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 500, 200, tc.SoLanMacDinh)
	return s
}

func (s *cuaSoServerDatXe) veTaiNguyen() fyne.CanvasObject {
	s.bd = moiBanDo(200, 150, true, nil, nil)
	vien := canvas.NewRectangle(color.Transparent)
	vien.StrokeColor, vien.StrokeWidth = mau("#808080"), 1
	s.nhanTrong = chu("–", Den, 18, true)
	s.ls = moiDsChu(6, 10, 230, 100)
	s.oSoXe = oNhap(tc.StrPy(s.sv.TS("so_xe")))
	return container.NewVBox(container.NewHBox(container.NewStack(s.bd.O, vien), container.NewVBox(s.nhanTrong, s.ls.O)),
		container.NewHBox(widget.NewLabel("Số xe (1–50):"), coDinh(s.oSoXe, 60, 30)))
}

func (s *cuaSoServerDatXe) thamSoDatLai() (tc.TS, error) {
	n, err := strconv.ParseInt(strings.TrimSpace(s.oSoXe.Text), 10, 64)
	if err != nil {
		return nil, fmt.Errorf("số xe phải là số nguyên")
	}
	return tc.T("so_xe", n), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerDatXe) datThamSo(soXe int) { s.oSoXe.SetText(strconv.Itoa(soXe)) }

func (s *cuaSoServerDatXe) capNhatTaiNguyen(tt, _ tc.M) {
	trong := tc.SoNguyen(tt["trong"])
	m := XanhDam
	if trong == 0 {
		m = DoDam
	}
	datChu(s.nhanTrong, fmt.Sprintf("Trống %d/%d xe", trong, tc.SoNguyen(tt["so_xe"])), m)
	ds := tc.DSM(tt["xe"])
	var khoa []string
	for _, x := range ds {
		khoa = append(khoa, fmt.Sprintf("%d:%v", tc.SoNguyen(x["ma"]), x["khach"]))
	}
	if k := strings.Join(khoa, ","); k == s.veCu {
		return
	} else {
		s.veCu = k
	}
	s.bd.xoa()
	var dong []string
	var coKhach []bool
	for _, x := range ds {
		p := viTri(x)
		khach, _ := x["khach"].(string)
		nhan := khach
		if nhan == "" {
			nhan = tc.Chuoi(x["tai_xe"])
		}
		s.bd.xe(p[0], p[1], tc.Chuoi(x["mau"]), nhan, "ngang", khach != "", false)
		if khach == "" {
			khach = "·"
		}
		dong = append(dong, fmt.Sprintf("%2d %-6s%-11s%s", tc.SoNguyen(x["ma"]), tc.Chuoi(x["tai_xe"]),
			tc.Chuoi(x["bien_so"]), khach))
		coKhach = append(coKhach, x["khach"] != nil)
	}
	s.bd.lamMoi()
	s.ls.dat(dong, func(i int) color.Color {
		if coKhach[i] {
			return mau("#a00000")
		}
		return Den
	})
}
