package main

// Bảng điều khiển máy chủ đấu giá. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI
// PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ khung
// "Tài nguyên": món hàng, giá hiện tại, người dẫn đầu, đồng hồ phiên, vài giá gần nhất và tham số đặt
// lại phiên.

import (
	"fmt"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.6_DauGia/tranhchap"
)

type cuaSoServerDauGia struct {
	*cuaSoServerCoSo
	sv         *tc.MayChuDauGia
	nhanMon    *canvas.Text
	nhanGia    *canvas.Text
	nhanDanDau *canvas.Text
	nhanDongHo *canvas.Text
	ls         *dsChu
	oTS        map[string]*widget.Entry
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerDauGia {
	s := &cuaSoServerDauGia{oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ đấu giá MR6.6", "Tài nguyên: phiên đấu giá",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, tc.T("thoi_gian_s", 120.0))
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 200, 200, tc.SoLanMacDinh)
	return s
}

func (s *cuaSoServerDauGia) veTaiNguyen() fyne.CanvasObject {
	anh := oAnhTinh(anhBinhGom(120, 130, 60, 14, 0.62), 120, 130)
	s.nhanMon = chu("–", Den, 13, true)
	s.nhanGia = chu("–", mau("#8b0000"), 22, true)
	s.nhanDanDau = chu("Dẫn đầu: –", Den, 12, false)
	s.nhanDongHo = chu("Còn lại: –", Den, 14, true)
	s.nhanDongHo.TextStyle.Monospace = true
	s.ls = moiDsChu(3, 10, 250, 56)
	cot := container.NewVBox(s.nhanMon, s.nhanGia, s.nhanDanDau, s.nhanDongHo, s.ls.O)
	ts := s.sv.ThamSo()
	luoi := container.New(layout.NewGridLayout(4))
	for _, d := range [][2]string{{"gia_khoi_diem", "Giá khởi điểm:"}, {"buoc_gia", "Bước giá:"},
		{"thoi_gian_s", "Phiên (s, 0 = ∞):"}, {"gia_han_s", "Gia hạn (s):"}} {
		v, _ := ts.Lay(d[0])
		gt := tc.StrPy(v)
		if f, la := v.(float64); la {
			gt = tc.FmtG(f)
		}
		s.oTS[d[0]] = oNhap(gt)
		luoi.Add(widget.NewLabel(d[1]))
		luoi.Add(s.oTS[d[0]])
	}
	return container.NewVBox(container.NewHBox(anh, cot), coDinh(luoi, 400, 64))
}

func (s *cuaSoServerDauGia) thamSoDatLai() (tc.TS, error) {
	g, e1 := strconv.ParseInt(strings.TrimSpace(s.oTS["gia_khoi_diem"].Text), 10, 64)
	b, e2 := strconv.ParseInt(strings.TrimSpace(s.oTS["buoc_gia"].Text), 10, 64)
	t, e3 := strconv.ParseFloat(strings.TrimSpace(s.oTS["thoi_gian_s"].Text), 64)
	h, e4 := strconv.ParseFloat(strings.TrimSpace(s.oTS["gia_han_s"].Text), 64)
	if e1 != nil || e2 != nil || e3 != nil || e4 != nil {
		return nil, fmt.Errorf("tham số phải là số")
	}
	return tc.T("gia_khoi_diem", g, "buoc_gia", b, "thoi_gian_s", t, "gia_han_s", h), nil
}

// datThamSo – điền ô tham số (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerDauGia) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		s.oTS[kv.K].SetText(tc.StrPy(kv.V))
	}
}

func (s *cuaSoServerDauGia) capNhatTaiNguyen(tt, _ tc.M) {
	mon := tt["mon"].(tc.M)
	datChu(s.nhanMon, fmt.Sprintf("%s (bước %s)", tc.Chuoi(mon["ten"]), tc.Tien(tc.SoNguyen(mon["buoc_gia"]))), Den)
	datChu(s.nhanGia, tc.Tien(tc.SoNguyen(tt["gia"])), s.nhanGia.Color)
	ai, _ := tt["nguoi_dan_dau"].(string)
	if ai == "" {
		ai = "–"
	}
	soLan := tc.SoNguyen(tt["so_lan"])
	if kt, _ := tt["ket_thuc"].(bool); kt {
		datChu(s.nhanDanDau, fmt.Sprintf("THẮNG: %s  (%d lượt)", ai, soLan), DoDam)
		datChu(s.nhanDongHo, "Kết thúc phiên", XamChu)
	} else {
		datChu(s.nhanDanDau, fmt.Sprintf("Dẫn đầu: %s  (%d lượt)", ai, soLan), Den)
		if tt["con_lai_ms"] == nil {
			datChu(s.nhanDongHo, "Còn lại: ∞ (không giới hạn)", XanhDam)
		} else {
			con := tc.SoThuc(tt["con_lai_ms"])
			m := XanhDam
			if con < 10_000 {
				m = DoDam
			}
			datChu(s.nhanDongHo, "Còn lại: "+dongHo(con/1000), m)
		}
	}
	var ds []string
	for i, x := range tc.DSM(tt["lich_su"]) {
		if i >= 3 {
			break
		}
		d := fmt.Sprintf("%-6s%14s", tc.Chuoi(x["nguoi"]), tc.Tien(tc.SoNguyen(x["gia"])))
		if gh, _ := x["gia_han"].(bool); gh {
			d += " GH"
		}
		ds = append(ds, d)
	}
	s.ls.dat(ds, nil)
}
