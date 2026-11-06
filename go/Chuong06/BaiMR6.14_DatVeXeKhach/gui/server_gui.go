package main

// Bảng điều khiển máy chủ đặt vé xe khách theo chặng. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số
// trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ
// vẽ khung "Tài nguyên": sơ đồ xe thu nhỏ (mỗi chỗ có dải vạch theo 3 chặng, ghế nóng viền đỏ), số chỗ-chặng
// mỗi loại, số chỗ đã bán theo chặng và tham số đặt lại (thời hạn giữ chỗ, loại xe giuong/ghe, ghế nóng).

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

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.14_DatVeXeKhach/tranhchap"
)

type cuaSoServerDatVeXe struct {
	*cuaSoServerCoSo
	sv        *tc.MayChuDatVeXe
	soDo      *soDoXe
	khungXe   *fyne.Container
	nhanXe    *canvas.Text
	nhanDem   map[string]*canvas.Text
	nhanChang *widget.Label
	nhanNong  *widget.Label
	oTS       map[string]*widget.Entry
	veCu      string
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerDatVeXe {
	s := &cuaSoServerDatVeXe{nhanDem: map[string]*canvas.Text{}, oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ đặt vé xe khách MR6.14",
		"Tài nguyên: chỗ × chặng (Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang)",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, nil)
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 500, 200, tc.SoLanMacDinh)
	return s
}

var demCho = [][2]string{{"trong", "Trống"}, {"giu", "Đang giữ"}, {"ban", "Đã bán"}}

func (s *cuaSoServerDatVeXe) veTaiNguyen() fyne.CanvasObject {
	s.khungXe = container.NewStack()
	soDo := coDinh(container.NewStack(canvas.NewRectangle(nenXe), s.khungXe), 184, 212)
	cot := container.NewVBox()
	s.nhanXe = chu("", Den, 12, true)
	cot.Add(s.nhanXe)
	for i, d := range demCho {
		o := canvas.NewRectangle([]color.NRGBA{mTrong, mKhacGiu, mDaBan}[i])
		o.StrokeColor, o.StrokeWidth = Den, 1
		s.nhanDem[d[0]] = chu(" "+d[1]+": –", Den, 13, true)
		cot.Add(container.NewHBox(coDinh(o, 16, 16), s.nhanDem[d[0]]))
	}
	s.nhanChang = widget.NewLabel("")
	s.nhanChang.TextStyle.Monospace = true
	s.nhanNong = widget.NewLabel("")
	cot.Add(s.nhanChang)
	cot.Add(s.nhanNong)
	ts := s.sv.ThamSo()
	hang := container.NewHBox()
	for _, d := range [][3]string{{"ttl_s", "Giữ chỗ (s):", "50"}, {"loai_xe", "Loại xe:", "64"}, {"ghe_nong", "Ghế nóng:", "48"}} {
		v, _ := ts.Lay(d[0])
		gt := tc.StrPy(v)
		if f, la := v.(float64); la {
			gt = tc.FmtG(f)
		}
		s.oTS[d[0]] = oNhap(gt)
		w, _ := strconv.Atoi(d[2])
		hang.Add(widget.NewLabel(d[1]))
		hang.Add(coDinh(s.oTS[d[0]], float32(w), 30))
	}
	return container.NewVBox(container.NewHBox(soDo, cot, layout.NewSpacer()), hang)
}

func (s *cuaSoServerDatVeXe) thamSoDatLai() (tc.TS, error) {
	t, err := strconv.ParseFloat(strings.TrimSpace(s.oTS["ttl_s"].Text), 64)
	if err != nil {
		return nil, fmt.Errorf("thời hạn giữ chỗ phải là số")
	}
	loai := strings.ToLower(strings.TrimSpace(s.oTS["loai_xe"].Text))
	ts := tc.T("ttl_s", t, "loai_xe", loai)
	nong := strings.ToUpper(strings.TrimSpace(s.oTS["ghe_nong"].Text))
	if !(loai != s.sv.TSStr("loai_xe") && nong == s.sv.GheNong()) { // đổi loại xe mà không sửa ghế nóng: máy chủ tự chọn chỗ đầu
		ts = ts.Dat("ghe_nong", nong)
	}
	return ts, nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerDatVeXe) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerDatVeXe) capNhatTaiNguyen(tt, _ tc.M) {
	dem, _ := tt["dem"].(tc.M)
	for _, d := range demCho {
		datChu(s.nhanDem[d[0]], fmt.Sprintf(" %s: %d", d[1], tc.SoNguyen(dem[d[0]])), Den)
	}
	datChu(s.nhanXe, fmt.Sprintf("%s, chuyến %s", tc.Chuoi(tt["ten_xe"]), tc.Chuoi(tt["chuyen"])), Den)
	ghe, soDo := tc.DSChuoi(tt["ghe"]), tc.DSChuoi(tt["so_do"])
	var dong []string
	for k := 0; k < tc.SoChang; k++ {
		ban := 0
		for _, d := range soDo {
			if k < len(d) && d[k] == 'b' {
				ban++
			}
		}
		dong = append(dong, fmt.Sprintf("%s-%s %2d/%d", tc.MaTram[k], tc.MaTram[k+1], ban, len(ghe)))
	}
	s.nhanChang.SetText("Đã bán theo chặng:\n" + strings.Join(dong, "\n"))
	nong := tc.Chuoi(tt["ghe_nong"])
	if nong != "" {
		d := dongCua(tt, nong)
		var p []string
		for k := 0; k < tc.SoChang; k++ {
			if d[k] != '.' {
				ai := chuCua(tt, fmt.Sprintf("%s#%d", nong, k))
				p = append(p, fmt.Sprintf("%s-%s: %s", tc.MaTram[k], tc.MaTram[k+1], ai))
			}
		}
		chu := strings.Join(p, ", ")
		if chu == "" {
			chu = "trống cả tuyến"
		}
		s.nhanNong.SetText(fmt.Sprintf("Ghế nóng (mô phỏng): %s\n%s", nong, chu))
	} else {
		s.nhanNong.SetText("Ghế nóng (mô phỏng): ngẫu nhiên")
	}
	if loai := tc.Chuoi(tt["loai_xe"]); s.soDo == nil || s.soDo.loaiXe != loai {
		s.soDo = moiSoDoXe(loai, 0.5, false)
		s.khungXe.Objects = []fyne.CanvasObject{s.soDo.C}
		s.khungXe.Refresh()
		s.veCu = ""
		s.datThamSo(tc.T("loai_xe", loai, "ghe_nong", nong)) // đổi loại xe: ô ghế nóng theo máy chủ
	}
	if k := strings.Join(soDo, "") + nong; k == s.veCu {
		return
	} else {
		s.veCu = k
	}
	var vien map[string]color.NRGBA
	if nong != "" {
		vien = map[string]color.NRGBA{nong: mau("#ff3030")}
	}
	s.soDo.ve(tt, "", 0, tc.SoChang, nil, vien)
}
