package main

// Bảng điều khiển máy chủ đăng ký học phần. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực
// tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ
// vẽ khung "Tài nguyên": tổng số chỗ đã đăng ký theo sổ, mỗi lớp một thanh sĩ số (xanh, đỏ khi đủ; số theo
// sổ kiểm toán tô đỏ khi vượt sĩ số) và tham số đặt lại (số lớp, sĩ số).

import (
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.10_DangKyHocPhan/tranhchap"
)

const rongThanh = 150 // bề rộng thanh sĩ số (điểm ảnh)

type hangServer struct {
	ma         *canvas.Text
	nen, thanh *canvas.Rectangle
	so         *canvas.Text
}

type cuaSoServerDangKy struct {
	*cuaSoServerCoSo
	sv       *tc.MayChuDangKy
	nhanTong *canvas.Text
	hang     []*hangServer
	oTS      map[string]*widget.Entry
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerDangKy {
	s := &cuaSoServerDangKy{oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ đăng ký học phần MR6.10", "Tài nguyên: lớp học phần",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, nil)
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 2000, 200, tc.SoLanMacDinh)
	return s
}

func (s *cuaSoServerDangKy) veTaiNguyen() fyne.CanvasObject {
	s.nhanTong = chu("–", mau("#8b0000"), 15, true)
	cot := container.NewVBox(s.nhanTong)
	for range tc.DanhMuc {
		h := &hangServer{ma: chu("", Den, 11, false), nen: canvas.NewRectangle(mau("#dddddd")),
			thanh: canvas.NewRectangle(mau("#3cb44b")), so: chu("", Den, 11, true)}
		h.ma.TextStyle.Monospace = true
		thanh := container.NewWithoutLayout(h.nen, h.thanh)
		h.nen.Resize(fyne.NewSize(rongThanh, 14))
		h.thanh.Resize(fyne.NewSize(0, 14))
		cot.Add(container.New(cotCoDinh{78, rongThanh + 8, 110}, container.NewCenter(h.ma),
			container.NewCenter(coDinh(thanh, rongThanh, 14)), container.NewCenter(h.so)))
		s.hang = append(s.hang, h)
	}
	ts := s.sv.ThamSo()
	lop, _ := ts.Lay("so_lop")
	siSo, _ := ts.Lay("si_so")
	s.oTS["so_lop"], s.oTS["si_so"] = oNhap(tc.StrPy(lop)), oNhap(tc.StrPy(siSo))
	cot.Add(container.NewHBox(widget.NewLabel("Số lớp:"), coDinh(s.oTS["so_lop"], 40, 30), widget.NewLabel("Sĩ số:"),
		coDinh(s.oTS["si_so"], 56, 30)))
	return cot
}

func (s *cuaSoServerDangKy) thamSoDatLai() (tc.TS, error) {
	l, e1 := strconv.ParseInt(strings.TrimSpace(s.oTS["so_lop"].Text), 10, 64)
	n, e2 := strconv.ParseInt(strings.TrimSpace(s.oTS["si_so"].Text), 10, 64)
	if e1 != nil || e2 != nil {
		return nil, fmt.Errorf("tham số phải là số nguyên")
	}
	return tc.T("so_lop", l, "si_so", n), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerDangKy) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerDangKy) capNhatTaiNguyen(tt, tk tc.M) {
	datChu(s.nhanTong, tc.Chuoi(tk["tom_tat"]), mau("#8b0000"))
	lop := tc.DSM(tt["lop"])
	for i, h := range s.hang {
		if i >= len(lop) {
			datChu(h.ma, "", Den)
			datChu(h.so, "", Den)
			h.nen.Hide()
			h.thanh.Hide()
			continue
		}
		x := lop[i]
		h.nen.Show()
		h.thanh.Show()
		datChu(h.ma, tc.Chuoi(x["ma"]), Den)
		daDK, siSo, theoSo := tc.SoNguyen(x["da_dk"]), tc.SoNguyen(x["si_so"]), tc.SoNguyen(x["theo_so"])
		m := mau("#3cb44b")
		if daDK >= siSo {
			m = DoDam
		}
		h.thanh.FillColor = m
		h.thanh.Resize(fyne.NewSize(float32(rongThanh*min(1.0, float64(daDK)/float64(siSo))), 14))
		h.thanh.Refresh()
		text, mChu := fmt.Sprintf("%d/%d", daDK, siSo), color.Color(Den)
		if theoSo > siSo {
			text, mChu = text+fmt.Sprintf(" (sổ %d)", theoSo), DoDam
		}
		datChu(h.so, text, mChu)
	}
}
