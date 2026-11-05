package main

// Bảng điều khiển máy chủ ngân hàng. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số trực tiếp, SAI
// PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này chỉ vẽ khung
// "Tài nguyên": TỔNG TIỀN HỆ THỐNG (xanh khi khớp, đỏ khi lệch), mỗi tài khoản một thanh số dư (vạch xám =
// số dư ban đầu), số giao dịch, số vụ khóa chết phát hiện, vài giao dịch gần nhất và tham số đặt lại (số
// tài khoản, số dư ban đầu, ô "khóa ngây thơ").

import (
	"fmt"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

type thanhTK struct {
	ten, so    *canvas.Text
	nen, thanh *canvas.Rectangle
	vach       *canvas.Line
}

type cuaSoServerNganHang struct {
	*cuaSoServerCoSo
	sv         *tc.MayChuNganHang
	nhanTong   *oMau
	nhanTongCT *canvas.Text
	bieuDo     *fyne.Container
	thanh      []*thanhTK
	veCu       string
	nhanGD     *canvas.Text
	ls         *dsChu
	oSoTK      *widget.Entry
	oSoDu      *widget.Entry
	oNgayTho   *widget.Check
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerNganHang {
	s := &cuaSoServerNganHang{}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ ngân hàng MR6.12", "Tài nguyên: các tài khoản",
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

func (s *cuaSoServerNganHang) veTaiNguyen() fyne.CanvasObject {
	s.nhanTong = moiOMau("–", XamNhat, Den, 20, 300, 32, false)
	s.nhanTongCT = chu("", Den, 11, false)
	nen := canvas.NewRectangle(Trang)
	nen.StrokeColor, nen.StrokeWidth = mau("#808080"), 1
	nen.Resize(fyne.NewSize(360, 150))
	s.bieuDo = container.NewWithoutLayout(nen)
	for range tc.ChuTK {
		t := &thanhTK{ten: chu("", Den, 9, false), so: chu("", Den, 9, false), nen: canvas.NewRectangle(mau("#eeeeee")),
			thanh: canvas.NewRectangle(mau("#4a90d9")), vach: canvas.NewLine(mau("#808080"))}
		t.so.TextStyle.Monospace = true
		for _, o := range []fyne.CanvasObject{t.ten, t.nen, t.thanh, t.vach, t.so} {
			o.Hide()
			s.bieuDo.Add(o)
		}
		s.thanh = append(s.thanh, t)
	}
	s.nhanGD = chu("", Den, 12, false)
	s.ls = moiDsChu(3, 10, 360, 56)
	ts := s.sv.ThamSo()
	soTK, _ := ts.Lay("so_tk")
	soDu, _ := ts.Lay("so_du_dau")
	ngay, _ := ts.Lay("khoa_ngay_tho")
	s.oSoTK, s.oSoDu = oNhap(tc.StrPy(soTK)), oNhap(tc.StrPy(soDu))
	s.oNgayTho = widget.NewCheck("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)", nil)
	s.oNgayTho.SetChecked(tc.SoNguyen(ngay) != 0)
	return container.NewVBox(chu("TỔNG TIỀN HỆ THỐNG", mau("#404040"), 10, true), s.nhanTong.O, s.nhanTongCT,
		coDinh(s.bieuDo, 360, 150), s.nhanGD, s.ls.O,
		container.NewHBox(widget.NewLabel("Số tài khoản:"), coDinh(s.oSoTK, 44, 30), widget.NewLabel("Số dư ban đầu:"),
			coDinh(s.oSoDu, 96, 30)), s.oNgayTho)
}

func (s *cuaSoServerNganHang) thamSoDatLai() (tc.TS, error) {
	n, e1 := strconv.ParseInt(strings.TrimSpace(s.oSoTK.Text), 10, 64)
	d, e2 := strconv.ParseInt(strings.TrimSpace(s.oSoDu.Text), 10, 64)
	if e1 != nil || e2 != nil {
		return nil, fmt.Errorf("tham số phải là số")
	}
	ngay := int64(0)
	if s.oNgayTho.Checked {
		ngay = 1
	}
	return tc.T("so_tk", n, "so_du_dau", d, "khoa_ngay_tho", ngay), nil
}

// datKhoaNgayTho – bấm ô chọn "khóa ngây thơ" (kịch bản demo) – có hiệu lực khi bấm Đặt lại.
func (s *cuaSoServerNganHang) datKhoaNgayTho(bat bool) { s.oNgayTho.SetChecked(bat) }

func (s *cuaSoServerNganHang) capNhatTaiNguyen(tt, _ tc.M) {
	tong, tongDau := tc.SoNguyen(tt["tong"]), tc.SoNguyen(tt["tong_dau"])
	lech := tong - tongDau
	if lech != 0 {
		s.nhanTong.dat(tc.Tien(tong), mau("#ff6060"), Trang)
		datChu(s.nhanTongCT, "ban đầu "+tc.Tien(tongDau)+" – LỆCH "+tc.TienDau(lech), DoDam)
	} else {
		s.nhanTong.dat(tc.Tien(tong), mau("#70d070"), Den)
		datChu(s.nhanTongCT, "ban đầu "+tc.Tien(tongDau)+" – khớp", XanhDam)
	}
	s.veTaiKhoan(tt)
	gd := fmt.Sprintf("Giao dịch: %d   Khóa chết phát hiện: %d", tc.SoNguyen(tt["so_gd"]), tc.SoNguyen(tt["khoa_chet"]))
	if ngay, _ := tt["khoa_ngay_tho"].(bool); ngay {
		gd += "   [khóa ngây thơ]"
	} else {
		gd += "   [khóa theo thứ tự]"
	}
	m := Den
	if tc.SoNguyen(tt["khoa_chet"]) > 0 {
		m = DoDam
	}
	datChu(s.nhanGD, gd, m)
	var ds []string
	for i, g := range tc.DSM(tt["lich_su"]) {
		if i < 3 {
			ds = append(ds, fmt.Sprintf("%s %s→%s %13s %s", tc.Chuoi(g["gio"]), tc.Chuoi(g["tu"]), tc.Chuoi(g["den"]),
				tc.Tien(tc.SoNguyen(g["so_tien"])), tc.Chuoi(g["nguoi"])))
		}
	}
	s.ls.dat(ds, nil)
}

// veTaiKhoan – mỗi tài khoản một thanh số dư (≤ 10 dòng mỗi cột, 2 cột khi > 10 tài khoản).
func (s *cuaSoServerNganHang) veTaiKhoan(tt tc.M) {
	ds := tc.DSM(tt["tai_khoan"])
	khoa := ""
	for _, t := range ds {
		khoa += fmt.Sprint(t["ma"], t["so_du"])
	}
	if khoa == s.veCu {
		return
	}
	s.veCu = khoa
	dau := tc.SoNguyen(tt["tong_dau"]) / int64(max(1, len(ds)))
	lon := 2 * dau
	for _, t := range ds {
		lon = max(lon, tc.SoNguyen(t["so_du"]))
	}
	lon = max(lon, 1)
	cot := 1
	if len(ds) > 10 {
		cot = 2
	}
	rong, cao := float32(360/cot), float32(15)
	for i, t := range s.thanh {
		if i >= len(ds) {
			for _, o := range []fyne.CanvasObject{t.ten, t.nen, t.thanh, t.vach, t.so} {
				o.Hide()
			}
			continue
		}
		x, y := float32(i/10)*rong+4, float32(i%10)*cao+3
		so := tc.SoNguyen(ds[i]["so_du"])
		t.ten.Text = tc.Chuoi(ds[i]["ma"]) + " " + tc.Chuoi(ds[i]["chu"])
		datChuTai(nil, t.ten, x, y+6, "w")
		x0, x1 := x+62, x+rong-76
		t.nen.Move(fyne.NewPos(x0, y+2))
		t.nen.Resize(fyne.NewSize(x1-x0, 9))
		t.thanh.Move(fyne.NewPos(x0, y+2))
		t.thanh.Resize(fyne.NewSize(max(0, (x1-x0)*float32(so)/float32(lon)), 9))
		t.thanh.FillColor = mau("#4a90d9")
		if so < dau {
			t.thanh.FillColor = mau("#f0a030")
		}
		xv := x0 + (x1-x0)*float32(dau)/float32(lon)
		t.vach.Position1, t.vach.Position2 = fyne.NewPos(xv, y), fyne.NewPos(xv, y+13)
		t.so.Text = tc.Tien(so)
		datChuTai(nil, t.so, x+rong-6, y+6, "e")
		for _, o := range []fyne.CanvasObject{t.ten, t.nen, t.thanh, t.vach, t.so} {
			o.Show()
			o.Refresh()
		}
	}
}
