package main

// Bảng điều khiển máy chủ đặt vé xem phim. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp,
// SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ
// khung "Tài nguyên": sơ đồ phòng chiếu thu nhỏ (trống / đang giữ / đã bán, ghế nóng viền đỏ), số ghế mỗi
// loại và tham số đặt lại (thời hạn giữ chỗ, ghế nóng của bộ mô phỏng, giá vé).

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

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

type cuaSoServerDatVe struct {
	*cuaSoServerCoSo
	sv       *tc.MayChuDatVe
	soDo     *soDoGhe
	nhanDem  map[string]*canvas.Text
	nhanNong *widget.Label
	oTS      map[string]*widget.Entry
	veCu     string
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerDatVe {
	s := &cuaSoServerDatVe{nhanDem: map[string]*canvas.Text{}, oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ đặt vé xem phim MR6.8", "Tài nguyên: phòng chiếu 8 × 12 ghế",
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

var demGhe = [][2]string{{"trong", "Trống"}, {"giu", "Đang giữ"}, {"ban", "Đã bán"}}

func (s *cuaSoServerDatVe) veTaiNguyen() fyne.CanvasObject {
	s.soDo = moiSoDoGhe(13, 2, 6, 16, false)
	soDo := coDinh(container.NewStack(canvas.NewRectangle(nenRap), s.soDo.C), 206, 124)
	cot := container.NewVBox()
	for i, d := range demGhe {
		o := canvas.NewRectangle([]color.NRGBA{mTrong, mKhacGiu, mDaBan}[i])
		o.StrokeColor, o.StrokeWidth = Den, 1
		s.nhanDem[d[0]] = chu(" "+d[1]+": –", Den, 13, true)
		cot.Add(container.NewHBox(coDinh(o, 16, 16), s.nhanDem[d[0]]))
	}
	s.nhanNong = widget.NewLabel("")
	cot.Add(s.nhanNong)
	ts := s.sv.ThamSo()
	hang := container.NewHBox()
	for _, d := range [][3]string{{"ttl_s", "Giữ chỗ (s):", "50"}, {"ghe_nong", "Ghế nóng:", "48"}, {"gia_ve", "Giá vé:", "72"}} {
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

func (s *cuaSoServerDatVe) thamSoDatLai() (tc.TS, error) {
	t, e1 := strconv.ParseFloat(strings.TrimSpace(s.oTS["ttl_s"].Text), 64)
	g, e2 := strconv.ParseInt(strings.TrimSpace(s.oTS["gia_ve"].Text), 10, 64)
	if e1 != nil || e2 != nil {
		return nil, fmt.Errorf("thời hạn và giá vé phải là số")
	}
	return tc.T("ttl_s", t, "ghe_nong", strings.ToUpper(strings.TrimSpace(s.oTS["ghe_nong"].Text)), "gia_ve", g), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerDatVe) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerDatVe) capNhatTaiNguyen(tt, _ tc.M) {
	dem, _ := tt["dem"].(tc.M)
	for _, d := range demGhe {
		datChu(s.nhanDem[d[0]], fmt.Sprintf(" %s: %d", d[1], tc.SoNguyen(dem[d[0]])), Den)
	}
	nong := tc.Chuoi(tt["ghe_nong"])
	chu, _ := tt["chu"].(tc.M)
	if nong != "" {
		ai, _ := chu[nong].(string)
		if ai == "" {
			ai = "trống"
		}
		s.nhanNong.SetText(fmt.Sprintf("Ghế nóng (mô phỏng): %s\n%s: %s", nong, nong, ai))
	} else {
		s.nhanNong.SetText("Ghế nóng (mô phỏng): ngẫu nhiên")
	}
	soDo := tc.DSChuoi(tt["so_do"])
	if k := strings.Join(soDo, "") + nong; k == s.veCu {
		return
	} else {
		s.veCu = k
	}
	m := map[string]color.NRGBA{}
	for i, r := range []rune(tc.Chuoi(tt["hang"])) {
		if i >= len(soDo) {
			break
		}
		for c, x := range soDo[i] {
			if x == 'g' {
				m[fmt.Sprintf("%c%d", r, c+1)] = mKhacGiu
			} else if x == 'b' {
				m[fmt.Sprintf("%c%d", r, c+1)] = mDaBan
			}
		}
	}
	var vien map[string]color.NRGBA
	if nong != "" {
		vien = map[string]color.NRGBA{nong: mau("#ff3030")}
	}
	s.soDo.ve(m, vien)
}
