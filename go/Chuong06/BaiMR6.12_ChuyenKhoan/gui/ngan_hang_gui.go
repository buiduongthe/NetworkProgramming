package main

// Ứng dụng ngân hàng số (người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở
// chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ ứng dụng ngân hàng: thẻ tài khoản (nền chuyển màu, góc bo,
// chip vẽ bằng bút ảnh; số tài khoản, chủ tài khoản, SỐ DƯ cỡ lớn là chữ Fyne đặt lên trên), chọn "tài
// khoản của tôi", form chuyển tiền (tài khoản đích, số tiền + nút chọn nhanh, nội dung), dòng thông báo
// ("Chuyển thành công…", "Không đủ số dư", "Giao dịch bị hủy: khóa chết…", "+ Nhận … từ …") và lịch sử giao
// dịch của tài khoản (tiền ra đỏ, tiền vào xanh). Số dư và tiền đến lấy từ cap_nhat của máy chủ (≤ 10 lần/giây).

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

const tenNganHang = "NGÂN HÀNG SỐ LTM"

var nhanh = []struct {
	x int64
	s string
}{{100_000, "100K"}, {500_000, "500K"}, {1_000_000, "1 triệu"}, {2_000_000, "2 triệu"}}

var nenCuaSo = color.NRGBA{0xC4, 0xD3, 0xE8, 0xFF} // màu nền chủ đề (temSang)

func tron(c1, c2 color.NRGBA, t float64) color.NRGBA {
	f := func(a, b uint8) uint8 { return uint8(math.Round(float64(a) + (float64(b)-float64(a))*t)) }
	return color.NRGBA{f(c1.R, c2.R), f(c1.G, c2.G), f(c1.B, c2.B), 0xFF}
}

// cung – cung tròn tâm (cx, cy), bán kính r, từ góc a0 tới a1 (độ, ngược chiều kim đồng hồ như Tk).
func (b *but) cung(cx, cy, r, a0, a1, day float64, m color.NRGBA) {
	var p []diem
	for i := 0; i <= 24; i++ {
		g := (a0 + (a1-a0)*float64(i)/24) * math.Pi / 180
		p = append(p, diem{cx + r*math.Cos(g), cy - r*math.Sin(g)})
	}
	b.gapKhuc(p, day, m, false)
}

// anhThe – nền thẻ tài khoản w×h: chuyển màu, góc bo, họa tiết, chip, biểu tượng không tiếp xúc.
func anhThe(w, h int) image.Image {
	tren, duoi := mau("#0b3d91"), mau("#1e88e5")
	b := moiBut(w, h)
	W, H, r := float64(w), float64(h), 16.0
	b.chuNhat(0, 0, W, H, nenCuaSo)
	for y := 0; y < h*sieuMau; y++ { // nền chuyển màu từ trên xuống, góc bo bán kính r
		yy := (float64(y) + 0.5) / sieuMau
		m := tron(tren, duoi, yy/H)
		lech := 0.0
		if yy < r {
			lech = r - math.Sqrt(r*r-(r-yy)*(r-yy))
		} else if yy > H-r {
			lech = r - math.Sqrt(math.Max(0, r*r-(yy-(H-r))*(yy-(H-r))))
		}
		for x := int(lech * sieuMau); x < int((W-lech)*sieuMau); x++ {
			b.cham(x, y, m)
		}
	}
	b.elip(W-150, -60, W+60, 150, nil, pm(mau("#5c9ce6")), 2, false) // họa tiết
	b.elip(W-110, -30, W+90, 170, nil, pm(mau("#4a8ad8")), 1, false)
	b.chuNhat(20, 44, 62, 74, mau("#e8c766")) // chip
	for _, y := range []float64{54, 64} {
		b.duong(20, y, 62, y, 1, mau("#a8862e"))
	}
	b.duong(41, 44, 41, 74, 1, mau("#a8862e"))
	b.gapKhuc([]diem{{20, 44}, {62, 44}, {62, 74}, {20, 74}, {20, 44}}, 1, mau("#a8862e"), false)
	for i, rr := range []float64{8, 14, 20} { // biểu tượng không tiếp xúc
		day := 1.0
		if i == 0 {
			day = 2
		}
		b.cung(70, 59, rr, -40, 40, day, Trang)
	}
	a := b.anh()
	for _, g := range [][2]int{{0, 0}, {w - 1, 0}, {0, h - 1}, {w - 1, h - 1}} { // góc ngoài thẻ: nền cửa sổ
		a.SetNRGBA(g[0], g[1], nenCuaSo)
	}
	return a
}

// theTK – thẻ tài khoản: ảnh nền + các dòng chữ.
type theTK struct {
	C               *fyne.Container
	w, h            float32
	ma, soDu, chuTK *canvas.Text
}

func datChuTai(c *fyne.Container, t *canvas.Text, x, y float32, neo string) {
	kt := t.MinSize()
	switch neo {
	case "e":
		x -= kt.Width
	}
	t.Move(fyne.NewPos(x, y-kt.Height/2))
	t.Resize(kt)
	if c != nil {
		c.Add(t)
	}
}

func moiTheTK(w, h float32) *theTK {
	t := &theTK{w: w, h: h, C: container.NewWithoutLayout()}
	nen := oAnhTinh(anhThe(int(w), int(h)), w, h)
	nen.Resize(fyne.NewSize(w, h))
	t.C.Add(nen)
	xanh := mau("#cfe3ff")
	datChuTai(t.C, chu(tenNganHang, Trang, 15, true), 18, 20, "w")
	datChuTai(t.C, chu("THANH TOÁN", xanh, 11, true), w-18, 20, "e")
	datChuTai(t.C, chu("Số tài khoản", xanh, 11, false), 110, 50, "w")
	t.ma = chu("", Trang, 16, true)
	t.ma.TextStyle.Monospace = true
	datChuTai(t.C, chu("Số dư khả dụng", xanh, 12, false), 20, 98, "w")
	t.soDu = chu("", Trang, 29, true)
	t.chuTK = chu("", Trang, 12, true)
	datChuTai(t.C, chu("VND", xanh, 12, true), w-18, h-22, "e")
	t.C.Add(t.ma)
	t.C.Add(t.soDu)
	t.C.Add(t.chuTK)
	t.ve("TK??", "–", 0)
	return t
}

func (t *theTK) ve(ma, chuTK string, soDu int64) {
	t.ma.Text, t.soDu.Text, t.chuTK.Text = ma+"  ••••  6125", tc.Tien(soDu), "CHỦ TÀI KHOẢN: "+strings.ToUpper(chuTK)
	datChuTai(nil, t.ma, 110, 68, "w")
	datChuTai(nil, t.soDu, 20, 126, "w")
	datChuTai(nil, t.chuTK, 20, t.h-22, "w")
	t.ma.Refresh()
	t.soDu.Refresh()
	t.chuTK.Refresh()
}

// oChuNhieuDong – ô chữ có nền màu, tự xuống dòng theo bề rộng (thông báo của ứng dụng ngân hàng).
type oChuNhieuDong struct {
	nen  *canvas.Rectangle
	dong []*canvas.Text
	rong float32
	co   float32
	dam  bool
	van  string
	O    fyne.CanvasObject
}

func moiOChuNhieuDong(soDong int, co, rong, cao float32, dam bool) *oChuNhieuDong {
	o := &oChuNhieuDong{nen: canvas.NewRectangle(XamNhat), rong: rong, co: co, dam: dam}
	hop := container.NewVBox()
	for i := 0; i < soDong; i++ {
		t := chu("", Den, co, dam)
		o.dong = append(o.dong, t)
		hop.Add(t)
	}
	o.O = coDinh(container.NewStack(o.nen, container.New(&leTrai{6}, hop)), rong, cao)
	return o
}

// ngat – chia đoạn văn thành các dòng vừa bề rộng (ngắt ở khoảng trắng).
func (o *oChuNhieuDong) ngat(van string) []string {
	var ra []string
	for _, doan := range strings.Split(van, "\n") {
		dong := ""
		for _, tu := range strings.Fields(doan) {
			thu := strings.TrimSpace(dong + " " + tu)
			if dong != "" && fyne.MeasureText(thu, o.co, fyne.TextStyle{Bold: o.dam}).Width > o.rong-14 {
				ra = append(ra, dong)
				dong = tu
			} else {
				dong = thu
			}
		}
		ra = append(ra, dong)
	}
	return ra
}

func (o *oChuNhieuDong) dat(van string, nen, mauChu color.Color) {
	o.van = van
	ds := o.ngat(van)
	for i, t := range o.dong {
		s := ""
		if i < len(ds) {
			s = ds[i]
		}
		t.Text, t.Color = s, mauChu
		t.Refresh()
	}
	o.nen.FillColor = nen
	o.nen.Refresh()
}

func (o *oChuNhieuDong) text() string { return o.van }

// leTrai – bố cục: nội dung cách lề trái l điểm ảnh, căn giữa theo chiều dọc.
type leTrai struct{ l float32 }

func (b *leTrai) Layout(o []fyne.CanvasObject, kt fyne.Size) {
	for _, x := range o {
		m := x.MinSize()
		x.Move(fyne.NewPos(b.l, (kt.Height-m.Height)/2))
		x.Resize(fyne.NewSize(kt.Width-b.l, m.Height))
	}
}

func (b *leTrai) MinSize(o []fyne.CanvasObject) fyne.Size {
	var w, h float32
	for _, x := range o {
		w, h = max(w, x.MinSize().Width+b.l), max(h, x.MinSize().Height)
	}
	return fyne.NewSize(w, h)
}

// ===================================================================== cửa sổ

type dongLS struct {
	s  string
	ra bool
}

type cuaSoNganHang struct {
	*cuaSoNguoiDungCoSo
	tk                                  string // mã tài khoản của tôi
	daThay                              map[int64]bool
	soChuyen, soThanhCong, soNhan, soKq int
	soGDCu                              int64
	theCu                               string
	the                                 *theTK
	chonTK, chonDen                     *widget.Select
	ls                                  *dsChu
	dongLs                              []dongLS
	oTien, oNoiDung                     *widget.Entry
	nutChuyen                           *widget.Button
	thongBao                            *oChuNhieuDong
	nhanDen                             *oChuNhieuDong
	nhanDem                             *canvas.Text
	nhanHeThong                         *widget.Label
}

func moCuaSoNganHang(a fyne.App, ten, host string, cong int) *cuaSoNganHang {
	x := &cuaSoNganHang{daThay: map[int64]bool{}}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Ngân hàng số", ten, host, cong, x)
	x.hoanTat(720)
	return x
}

func (x *cuaSoNganHang) ve() fyne.CanvasObject {
	x.the = moiTheTK(340, 190)
	x.chonTK = widget.NewSelect(nil, func(string) { x.doiTK() })
	x.chonTK.PlaceHolder = "–"
	x.ls = moiDsChu(9, 11, 340, 168)
	trai := container.NewVBox(coDinh(x.the.C, 340, 190),
		container.NewHBox(widget.NewLabel("Tài khoản của tôi:"), coDinh(x.chonTK, 170, 34)),
		nhom("Lịch sử giao dịch (mới nhất trên cùng)", x.ls.O))

	x.chonDen = widget.NewSelect(nil, nil)
	x.oTien = oNhap("500.000")
	x.oTien.TextStyle.Bold = true
	nutNhanh := container.NewHBox()
	for _, n := range nhanh {
		n := n
		nutNhanh.Add(nut(n.s, widget.MediumImportance, func() { x.datTien(n.x) }))
	}
	x.oNoiDung = oNhap("Chuyển tiền")
	x.nutChuyen = nut("Chuyển tiền", widget.WarningImportance, x.bamChuyen)
	form := container.New(layout.NewFormLayout(), widget.NewLabel("Đến tài khoản:"), coDinh(x.chonDen, 200, 34),
		widget.NewLabel("Số tiền (đ):"), coDinh(x.oTien, 140, 34))
	chuyen := nhom("Chuyển tiền", form, nutNhanh,
		container.New(layout.NewFormLayout(), widget.NewLabel("Nội dung:"), coDinh(x.oNoiDung, 220, 34)),
		container.NewCenter(coDinh(x.nutChuyen, 180, 40)))
	x.thongBao = moiOChuNhieuDong(3, 13, 350, 62, true)
	x.thongBao.dat("Chưa kết nối", XamNhat, Den)
	x.nhanDen = moiOChuNhieuDong(2, 12, 350, 40, true)
	x.nhanDen.dat("", nenCuaSo, mau("#0050a0"))
	x.nhanDem = chu("Đã chuyển 0 lần · thành công 0 · nhận 0", Den, 11, false)
	x.nhanHeThong = widget.NewLabel("")
	x.nhanHeThong.Wrapping = fyne.TextWrapWord
	phai := container.NewVBox(chuyen, x.thongBao.O, x.nhanDen.O, x.nhanDem, coDinh(x.nhanHeThong, 350, 52))
	x.nutChuyen.Disable()
	return container.NewHBox(trai, phai)
}

// ------------------------------------------------------------ tiện ích

func (x *cuaSoNganHang) chu(ma string) string {
	if x.tt != nil {
		for _, t := range tc.DSM(x.tt["tai_khoan"]) {
			if tc.Chuoi(t["ma"]) == ma {
				return tc.Chuoi(t["chu"])
			}
		}
	}
	return "?"
}

func (x *cuaSoNganHang) soDu() int64 {
	for _, t := range tc.DSM(x.tt["tai_khoan"]) {
		if tc.Chuoi(t["ma"]) == x.tk {
			return tc.SoNguyen(t["so_du"])
		}
	}
	return 0
}

func (x *cuaSoNganHang) datTien(v int64) { x.oTien.SetText(strings.TrimSuffix(tc.Tien(v), " đ")) }

func (x *cuaSoNganHang) bao(text string, nen color.Color, mauChu ...color.Color) {
	m := color.Color(Den)
	if len(mauChu) > 0 {
		m = mauChu[0]
	}
	x.thongBao.dat(text, nen, m)
}

// themLs – thêm một giao dịch vào đầu lịch sử: tiền ra (đỏ, dấu −) hoặc tiền vào (xanh, dấu +).
func (x *cuaSoNganHang) themLs(g tc.M, ra bool, ghiChu string) {
	doi, dau, tu := tc.Chuoi(g["tu"]), "+", "từ "
	if ra {
		doi, dau, tu = tc.Chuoi(g["den"]), "−", "tới"
	}
	dong := fmt.Sprintf("%s %s%13s %s %s %s", tc.Chuoi(g["gio"]), dau, tc.Tien(tc.SoNguyen(g["so_tien"])), tu, doi, x.chu(doi))
	if nd := tc.Chuoi(g["noi_dung"]); nd != "" {
		dong += " · " + nd
	}
	x.dongLs = append([]dongLS{{dong + ghiChu, ra}}, x.dongLs...)
	if len(x.dongLs) > 100 {
		x.dongLs = x.dongLs[:100]
	}
	x.veLs()
}

func (x *cuaSoNganHang) veLs() {
	var ds []string
	for _, d := range x.dongLs {
		ds = append(ds, d.s)
	}
	x.ls.dat(ds, func(i int) color.Color {
		if x.dongLs[i].ra {
			return DoDam
		}
		return XanhDam
	})
}

func (x *cuaSoNganHang) xoaLs() {
	x.dongLs = nil
	x.veLs()
}

// ------------------------------------------------------------ chọn tài khoản

func (x *cuaSoNganHang) doiTK() {
	ma, _, _ := strings.Cut(x.chonTK.Selected, " ")
	if ma == x.tk || ma == "" || x.tt == nil {
		return
	}
	x.tk = ma
	x.xoaLs()
	x.daThay = map[int64]bool{}
	for _, g := range tc.DSM(x.tt["lich_su"]) {
		x.daThay[tc.SoNguyen(g["ma"])] = true
	}
	x.dienDen()
	x.veLaiThe()
	x.log.them(fmt.Sprintf("Đăng nhập %s – %s, số dư %s", ma, x.chu(ma), tc.Tien(x.soDu())))
}

func (x *cuaSoNganHang) dienDen() {
	var ds []string
	for _, t := range tc.DSM(x.tt["tai_khoan"]) {
		if tc.Chuoi(t["ma"]) != x.tk {
			ds = append(ds, tc.Chuoi(t["ma"])+" – "+tc.Chuoi(t["chu"]))
		}
	}
	if strings.Join(ds, "|") != strings.Join(x.chonDen.Options, "|") {
		x.chonDen.Options = ds
		x.chonDen.Refresh()
	}
	co := false
	for _, d := range ds {
		co = co || d == x.chonDen.Selected
	}
	if !co && len(ds) > 0 {
		x.chonDen.SetSelected(ds[0])
	}
}

// chonNguoiNhan – chọn tài khoản đích (kịch bản demo) – như người dùng chọn trong danh sách.
func (x *cuaSoNganHang) chonNguoiNhan(ma string) { x.chonDen.SetSelected(ma + " – " + x.chu(ma)) }

func (x *cuaSoNganHang) veLaiThe() {
	if k := fmt.Sprint(x.tk, x.chu(x.tk), x.soDu()); k != x.theCu { // chỉ vẽ lại khi số dư/tài khoản đổi
		x.theCu = k
		x.the.ve(x.tk, x.chu(x.tk), x.soDu())
	}
}

// ------------------------------------------------------------ chuyển tiền

func (x *cuaSoNganHang) bamChuyen() {
	den, _, _ := strings.Cut(x.chonDen.Selected, " ")
	var v int64
	s := strings.NewReplacer(".", "", " ", "", "đ", "").Replace(x.oTien.Text)
	if _, err := fmt.Sscanf(s, "%d", &v); err != nil || fmt.Sprint(v) != s {
		x.bao("Số tiền không hợp lệ", mau("#ffb347"))
		return
	}
	x.soChuyen++
	x.log.them(fmt.Sprintf("Chuyển %s %s→%s…", tc.Tien(v), x.tk, den))
	x.bao(fmt.Sprintf("Đang xử lý: %s tới %s…", tc.Tien(v), den), mau("#fff3c4"))
	nd := []rune(strings.TrimSpace(x.oNoiDung.Text))
	if len(nd) > 40 {
		nd = nd[:40]
	}
	x.guiYC("chuyen", tc.T("tu", x.tk, "den", den, "so_tien", v, "noi_dung", string(nd)))
}

func (x *cuaSoNganHang) ketQua(_ string, truong tc.TS, tin tc.M, rtt float64) {
	lay := func(k string) any { v, _ := truong.Lay(k); return v }
	v, den := tc.SoNguyen(lay("so_tien")), tc.Chuoi(lay("den"))
	x.soKq++
	if ok, _ := tin["ok"].(bool); ok {
		x.soThanhCong++
		x.daThay[tc.SoNguyen(tin["ma_gd"])] = true
		if tc.Chuoi(lay("tu")) == x.tk {
			x.themLs(tc.M{"gio": time.Now().Format("15:04:05"), "tu": lay("tu"), "den": den, "so_tien": v,
				"noi_dung": lay("noi_dung")}, true, "")
		}
		x.log.them(fmt.Sprintf("  thành công (%.1f ms), mã GD %d", rtt, tc.SoNguyen(tin["ma_gd"])))
		x.bao(fmt.Sprintf("Chuyển thành công %s tới %s – %s\nSố dư mới: %s", tc.Tien(v), den, x.chu(den),
			tc.Tien(tc.SoNguyen(tin["so_du"]))), mau("#70d070"))
	} else {
		lyDo := tc.Chuoi(tin["ly_do"])
		x.log.them(fmt.Sprintf("  thất bại (%.0f ms): %s", rtt, lyDo))
		switch tin["ma_loi"] {
		case "khoa_cho_lau":
			r := []rune(lyDo)
			if len(r) > 0 {
				lyDo = strings.ToUpper(string(r[0])) + string(r[1:])
			}
			x.bao(lyDo, mau("#ff6060"), Trang)
		case "khong_du":
			x.bao("Không đủ số dư để chuyển "+tc.Tien(v), mau("#ffb347"))
		default:
			x.bao("Giao dịch thất bại: "+lyDo, mau("#ffb347"))
		}
	}
	x.capNhatDem()
}

func (x *cuaSoNganHang) capNhatDem() {
	datChu(x.nhanDem, fmt.Sprintf("Đã chuyển %d lần · thành công %d · nhận %d", x.soChuyen, x.soThanhCong, x.soNhan), Den)
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoNganHang) capNhat(tt tc.M) {
	tk := tc.DSM(tt["tai_khoan"])
	var ds []string
	coTK := false
	for _, t := range tk {
		ds = append(ds, tc.Chuoi(t["ma"])+" – "+tc.Chuoi(t["chu"]))
		coTK = coTK || tc.Chuoi(t["ma"]) == x.tk
	}
	if strings.Join(ds, "|") != strings.Join(x.chonTK.Options, "|") {
		x.chonTK.Options = ds
		x.chonTK.Refresh()
	}
	soGD := tc.SoNguyen(tt["so_gd"])
	if soGD < x.soGDCu { // máy chủ vừa đặt lại: số dư về ban đầu
		x.xoaLs()
		x.nhanDen.dat("", nenCuaSo, mau("#0050a0"))
		x.bao("Máy chủ đặt lại: số dư về ban đầu", XamNhat)
		x.log.them("Máy chủ đặt lại: số dư về ban đầu")
	}
	x.soGDCu = soGD
	if (x.tk == "" || !coTK) && len(tk) > 0 {
		x.tk = tc.Chuoi(tk[0]["ma"])
		for _, t := range tk {
			if tc.Chuoi(t["chu"]) == x.ten {
				x.tk = tc.Chuoi(t["ma"])
				break
			}
		}
		x.daThay = map[int64]bool{}
		for _, g := range tc.DSM(tt["lich_su"]) {
			x.daThay[tc.SoNguyen(g["ma"])] = true
		}
		x.xoaLs()
		x.log.them(fmt.Sprintf("Đăng nhập %s – %s, số dư %s", x.tk, x.chu(x.tk), tc.Tien(x.soDu())))
		x.bao("Xin chào! Chọn người nhận và số tiền để chuyển.", XamNhat)
	}
	if nh := x.tk + " – " + x.chu(x.tk); x.chonTK.Selected != nh {
		x.chonTK.SetSelected(nh)
	}
	x.dienDen()
	x.veLaiThe()
	x.nutChuyen.Enable()
	ls := tc.DSM(tt["lich_su"])
	for i := len(ls) - 1; i >= 0; i-- { // giao dịch mới liên quan tới tài khoản của tôi
		g := ls[i]
		ma := tc.SoNguyen(g["ma"])
		if x.daThay[ma] {
			continue
		}
		x.daThay[ma] = true
		tu := tc.Chuoi(g["tu"])
		switch {
		case tc.Chuoi(g["den"]) == x.tk:
			x.soNhan++
			x.themLs(g, false, "")
			x.log.them(fmt.Sprintf("+ Nhận %s từ %s (%s)", tc.Tien(tc.SoNguyen(g["so_tien"])), tu, x.chu(tu)))
			x.nhanDen.dat(fmt.Sprintf("+ Nhận %s từ %s – %s lúc %s", tc.Tien(tc.SoNguyen(g["so_tien"])), tu, x.chu(tu),
				tc.Chuoi(g["gio"])), mau("#bfe8ff"), mau("#0050a0"))
		case tu == x.tk: // tiền ra do phiên khác của cùng tài khoản
			x.themLs(g, true, " ("+tc.Chuoi(g["nguoi"])+")")
		}
	}
	lech := tc.SoNguyen(tt["tong"]) - tc.SoNguyen(tt["tong_dau"])
	s := fmt.Sprintf("Máy chủ: %d tài khoản, %d giao dịch", len(tk), soGD)
	if ngay, _ := tt["khoa_ngay_tho"].(bool); ngay {
		s += " – khóa ngây thơ"
	}
	if lech != 0 {
		s += "\nTổng tiền hệ thống LỆCH " + tc.Tien(lech) + "!"
		x.nhanHeThong.Importance = widget.DangerImportance
	} else {
		x.nhanHeThong.Importance = widget.MediumImportance
	}
	x.nhanHeThong.SetText(s)
	x.capNhatDem()
}

func (x *cuaSoNganHang) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoNganHang) moiNhip() {}

func (x *cuaSoNganHang) dongDinhKy() string {
	return fmt.Sprintf("%s số dư %s; chuyển %d, thành công %d, nhận %d", x.tk, tc.Tien(x.soDu()), x.soChuyen,
		x.soThanhCong, x.soNhan)
}

func (x *cuaSoNganHang) daKetNoi()  {}
func (x *cuaSoNganHang) matKetNoi() {}
