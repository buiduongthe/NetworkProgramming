package main

// Ứng dụng đặt vé xem phim (người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu)
// ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ ứng dụng đặt vé: áp phích phim (nền vẽ bằng bút ảnh,
// chữ Fyne), sơ đồ phòng chiếu 8 × 12 ghế với MÀN HÌNH, lối đi, nhãn hàng A–H; màu ghế: trống / đang
// chọn / người khác giữ / đã bán / bạn giữ / vé của bạn. Bấm ghế để chọn, "GIỮ CHỖ" gửi giu_cho, đồng
// hồ giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN" và "Nhả ghế". Sơ đồ cập nhật trực tiếp từ cap_nhat
// (≤ 10 lần/giây) khi người khác giữ/mua ghế; ghế đang chọn bị người khác giữ thì tự bỏ chọn và báo.

import (
	"fmt"
	"image/color"
	"math"
	"sort"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

var (
	mTrong   = mau("#ffffff")
	mChon    = mau("#b9f6ca")
	mKhacGiu = mau("#ffa726")
	mDaBan   = mau("#9e9e9e")
	mToiGiu  = mau("#2e7d32")
	mVeToi   = mau("#1565c0")
	doRap    = mau("#e71a0f")
	nenRap   = mau("#1c1c24")
)

var chuGiai = []struct {
	m color.NRGBA
	s string
}{{mTrong, "Trống"}, {mChon, "Đang chọn"}, {mKhacGiu, "Người khác giữ"}, {mDaBan, "Đã bán"}, {mToiGiu, "Bạn giữ"},
	{mVeToi, "Vé của bạn"}}

// dongHoGiu – mm:ss làm tròn lên.
func dongHoGiu(giay float64) string {
	g := max(0, int(giay+0.999))
	return fmt.Sprintf("%02d:%02d", g/60, g%60)
}

// ---------------------------------------------------------------- sơ đồ ghế

// soDoGhe – sơ đồ ghế bằng đối tượng Fyne; dùng ở ứng dụng đặt vé (ô = 30) và bảng máy chủ (ô nhỏ).
type soDoGhe struct {
	o, khe, le, tren float32
	nhan             bool
	rong, cao        float32
	viTri            map[string][4]float32
	ghe              map[string]*canvas.Rectangle
	so               map[string]*canvas.Text
	gach             map[string]*canvas.Line
	C                *fyne.Container
}

func moiSoDoGhe(o, khe, le, tren float32, nhan bool) *soDoGhe {
	loiDi := []int{2, 10}
	s := &soDoGhe{o: o, khe: khe, le: le, tren: tren, nhan: nhan, viTri: map[string][4]float32{},
		ghe: map[string]*canvas.Rectangle{}, so: map[string]*canvas.Text{}, gach: map[string]*canvas.Line{},
		C: container.NewWithoutLayout()}
	s.rong = le*2 + float32(tc.Cot)*(o+khe) + float32(len(loiDi))*o*0.5
	s.cao = tren + float32(len(tc.Hang))*(o*0.86+khe) + 4
	them := func(ob fyne.CanvasObject, x, y, w, h float32) {
		ob.Move(fyne.NewPos(x, y))
		ob.Resize(fyne.NewSize(w, h))
		s.C.Add(ob)
	}
	// màn hình: đường cong (nhiều đoạn thẳng ngắn)
	lech := tren * 0.55
	day := float32(2)
	if nhan {
		day = 4
	}
	var truoc fyne.Position
	for i := 0; i <= 24; i++ {
		t := float32(i) / 24
		x := le + (s.rong-2*le)*t
		y := lech - (lech-lech*0.35)*4*t*(1-t)
		p := fyne.NewPos(x, y)
		if i > 0 {
			l := canvas.NewLine(mau("#e8e8f0"))
			l.StrokeWidth = day
			l.Position1, l.Position2 = truoc, p
			s.C.Add(l)
		}
		truoc = p
	}
	if nhan {
		t := chu("MÀN HÌNH", mau("#b0b0c0"), 10, true)
		them(t, s.rong/2-t.MinSize().Width/2, lech+2, t.MinSize().Width, t.MinSize().Height)
	}
	for i, r := range tc.Hang {
		y := tren + float32(i)*(o*0.86+khe)
		for c := 1; c <= tc.Cot; c++ {
			x := le + float32(c-1)*(o+khe)
			for _, a := range loiDi {
				if c > a {
					x += o * 0.5
				}
			}
			g := fmt.Sprintf("%c%d", r, c)
			s.viTri[g] = [4]float32{x, y, x + o, y + o*0.86}
			hcn := canvas.NewRectangle(mTrong)
			hcn.StrokeColor, hcn.StrokeWidth = mau("#606070"), 1
			s.ghe[g] = hcn
			them(hcn, x, y, o, o*0.86)
			if nhan {
				them(canvas.NewRectangle(mau("#707080")), x+2, y+o*0.86-4, o-4, 3) // tay vịn
				gl := canvas.NewLine(color.Transparent)
				gl.Position1, gl.Position2 = fyne.NewPos(x+4, y+3), fyne.NewPos(x+o-4, y+o*0.86-6)
				s.gach[g] = gl
				s.C.Add(gl)
				t := chu(fmt.Sprint(c), mau("#303040"), 9, false)
				t.Alignment = fyne.TextAlignCenter
				s.so[g] = t
				them(t, x, y+1, o, o*0.86-6)
			}
		}
		if nhan {
			yy := y + o*0.43 - 8
			for _, x := range []float32{le/2 - 5, s.rong - le/2 - 5} {
				t := chu(string(r), mau("#c0c0d0"), 12, true)
				them(t, x, yy, 12, 16)
			}
		}
	}
	return s
}

// ve – mauGhe: {ghế: màu}; vien: {ghế: màu viền} (ghế nóng ở máy chủ).
func (s *soDoGhe) ve(mauGhe map[string]color.NRGBA, vien map[string]color.NRGBA) {
	for g, h := range s.ghe {
		m, co := mauGhe[g]
		if !co {
			m = mTrong
		}
		v, coV := vien[g]
		h.FillColor, h.StrokeColor, h.StrokeWidth = m, mau("#606070"), 1
		if coV {
			h.StrokeColor, h.StrokeWidth = v, 2
		}
		h.Refresh()
		if t := s.so[g]; t != nil {
			toi := m == mToiGiu || m == mVeToi || m == mDaBan
			mc := mau("#303040")
			if toi {
				mc = Trang
			}
			datChu(t, t.Text, mc)
			gl := s.gach[g]
			gl.StrokeColor = color.Transparent
			if m == mDaBan {
				gl.StrokeColor = mau("#e0e0e0")
			}
			gl.Refresh()
		}
	}
}

func (s *soDoGhe) gheTai(p fyne.Position) string {
	for g, v := range s.viTri {
		if v[0] <= p.X && p.X <= v[2] && v[1] <= p.Y && p.Y <= v[3] {
			return g
		}
	}
	return ""
}

// ---------------------------------------------------------------- áp phích

func anhApPhich() *canvas.Image {
	b := moiBut(250, 96)
	for i := 0; i < 12; i++ { // nền chuyển màu tím -> xanh
		b.chuNhat(0, float64(i*8), 250, float64(i*8+8), color.NRGBA{uint8(43 + i*2), uint8(22 + i*6), uint8(64 + i*9), 0xFF})
	}
	b.elip(196, 8, 242, 54, pm(mau("#4fc3f7")), nil, 0, false)
	// lục địa: phần hình tròn có góc 200°–340° (đo ngược chiều kim đồng hồ từ hướng 3 giờ, như cung Tk)
	for y := 8.0; y <= 54; y += 1.0 / 3 {
		for x := 196.0; x <= 242; x += 1.0 / 3 {
			dx, dy := x-219, y-31
			if dx*dx+dy*dy > 23*23 {
				continue
			}
			g := math.Atan2(-dy, dx) * 180 / math.Pi
			if g < 0 {
				g += 360
			}
			if g >= 200 && g <= 340 {
				b.cham(int(x*sieuMau), int(y*sieuMau), mau("#43a047"))
			}
		}
	}
	b.elip(196, 8, 242, 54, nil, pm(mau("#b3e5fc")), 2, false)
	for k := 0; k < 14; k++ {
		x, y := float64((k*37)%170+6), float64((k*23)%60+6)
		b.chuNhat(x, y, x+2, y+2, Trang)
	}
	return oAnhTinh(b.anh(), 250, 96)
}

// ---------------------------------------------------------------- cửa sổ đặt vé

type cuaSoDatVe struct {
	*cuaSoNguoiDungCoSo
	chon, giu, veCua  map[string]bool // ghế đang chọn / mình đang giữ (theo kq) / vé của mình
	phienGiu, phienVe int64
	han               *time.Time // thời điểm hết hạn giữ chỗ
	soGiu, soVe       int
	phimCu            string
	soDo              *soDoGhe
	tenPhim, dongPhim *canvas.Text
	giaPhim           *canvas.Text
	nhanGhe           *canvas.Text
	nhanTien          *canvas.Text
	nutGiu, nutTT     *widget.Button
	nutNha            *widget.Button
	nhanDongHo        *canvas.Text
	thongBao          *oMau
	nhanVe            *widget.Label
}

func moCuaSoDatVe(a fyne.App, ten, host string, cong int) *cuaSoDatVe {
	x := &cuaSoDatVe{chon: map[string]bool{}, giu: map[string]bool{}, veCua: map[string]bool{}}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Đặt vé xem phim", ten, host, cong, x)
	x.hoanTat(790)
	return x
}

func sapXep(m map[string]bool) []string {
	var ds []string
	for g := range m {
		ds = append(ds, g)
	}
	sort.Strings(ds)
	return ds
}

func (x *cuaSoDatVe) ve() fyne.CanvasObject {
	x.soDo = moiSoDoGhe(30, 4, 24, 46, true)
	nen := canvas.NewRectangle(nenRap)
	chuGiaiC := container.NewWithoutLayout()
	px := float32(10)
	for _, c := range chuGiai {
		o := canvas.NewRectangle(c.m)
		o.StrokeColor, o.StrokeWidth = mau("#606070"), 1
		o.Move(fyne.NewPos(px, x.soDo.cao+14))
		o.Resize(fyne.NewSize(14, 12))
		t := chu(c.s, mau("#d0d0e0"), 10, false)
		t.Move(fyne.NewPos(px+18, x.soDo.cao+12))
		chuGiaiC.Add(o)
		chuGiaiC.Add(t)
		px += 26 + t.MinSize().Width
	}
	trai := coDinh(container.NewStack(nen, x.soDo.C, chuGiaiC, moiVungBam(func(p fyne.Position) {
		if g := x.soDo.gheTai(p); g != "" {
			x.bam(g)
		}
	}, nil)), 486, 322)

	x.tenPhim = chu("", Trang, 15, true)
	x.dongPhim = chu("", mau("#e0d8f0"), 10, false)
	x.giaPhim = chu("", mau("#ffd54f"), 10, true)
	apPhich := container.NewStack(anhApPhich(), container.NewWithoutLayout(x.tenPhim, x.dongPhim, x.giaPhim))
	x.tenPhim.Move(fyne.NewPos(10, 28))
	x.dongPhim.Move(fyne.NewPos(10, 58))
	x.giaPhim.Move(fyne.NewPos(10, 74))
	x.nhanGhe = chu("Ghế đã chọn: –", Den, 13, true)
	x.nhanTien = chu("Tạm tính: 0 đ", Den, 12, false)
	x.nutGiu = nut("GIỮ CHỖ", widget.WarningImportance, x.giuCho)
	x.nhanDongHo = chu("--:--", mau("#909090"), 22, true)
	x.nhanDongHo.TextStyle.Monospace = true
	x.nutTT = nut("THANH TOÁN", widget.DangerImportance, x.thanhToan)
	x.nutNha = nut("Nhả ghế", widget.MediumImportance, x.nha)
	x.thongBao = moiOMau("Chưa kết nối", mau("#e8e8e8"), Den, 12, 270, 30, true)
	x.nhanVe = widget.NewLabel("Vé của bạn: –")
	x.nhanVe.Wrapping = fyne.TextWrapWord
	phai := container.NewStack(canvas.NewRectangle(Trang), container.NewPadded(container.NewVBox(
		coDinh(apPhich, 250, 96), x.nhanGhe, x.nhanTien, coDinh(x.nutGiu, 270, 36),
		container.NewHBox(container.NewCenter(widget.NewLabel("Giữ chỗ còn")), x.nhanDongHo),
		coDinh(x.nutTT, 270, 36), container.NewHBox(x.nutNha, layout.NewSpacer()), x.thongBao.O,
		coDinh(x.nhanVe, 270, 56))))
	x.veGhe()
	x.batNut()
	return container.NewHBox(trai, coDinh(phai, 286, 402))
}

// ------------------------------------------------------------ áp phích, sơ đồ

func (x *cuaSoDatVe) vePhim(tt tc.M) {
	datChu(x.tenPhim, strings.ToUpper(tc.Chuoi(tt["phim"])), Trang)
	datChu(x.dongPhim, fmt.Sprintf("2D Phụ đề · Suất %s · Phòng 3", tc.Chuoi(tt["suat"])), x.dongPhim.Color)
	datChu(x.giaPhim, fmt.Sprintf("%s/ghế · giữ chỗ %s s", tc.Tien(tc.SoNguyen(tt["gia_ve"])), tc.FmtG(tc.SoThuc(tt["ttl_s"]))),
		x.giaPhim.Color)
}

func (x *cuaSoDatVe) chuGhe(g string) string {
	if x.tt == nil {
		return ""
	}
	chu, _ := x.tt["chu"].(tc.M)
	s, _ := chu[g].(string)
	return s
}

func (x *cuaSoDatVe) veGhe() {
	m := map[string]color.NRGBA{}
	if x.tt != nil {
		soDo := tc.DSChuoi(x.tt["so_do"])
		for i, r := range []rune(tc.Chuoi(x.tt["hang"])) {
			if i >= len(soDo) {
				break
			}
			for c, k := range soDo[i] {
				g := fmt.Sprintf("%c%d", r, c+1)
				toi := x.chuGhe(g) == x.ten
				switch {
				case k == 'b' && toi:
					m[g] = mVeToi
				case k == 'b':
					m[g] = mDaBan
				case k == 'g' && toi:
					m[g] = mToiGiu
				case k == 'g':
					m[g] = mKhacGiu
				}
			}
		}
	}
	for g := range x.chon {
		if _, co := m[g]; !co {
			m[g] = mChon
		}
	}
	x.soDo.ve(m, nil)
}

// trangGhe – "trong" | "giu" (người khác) | "ban" | "toi_giu" | "ve" theo cap_nhat mới nhất.
func (x *cuaSoDatVe) trangGhe(g string) string {
	if x.tt == nil {
		return "trong"
	}
	i := strings.IndexRune(tc.Chuoi(x.tt["hang"]), rune(g[0]))
	soDo := tc.DSChuoi(x.tt["so_do"])
	var c int
	fmt.Sscan(g[1:], &c)
	if i < 0 || i >= len(soDo) || c < 1 || c > len(soDo[i]) {
		return "trong"
	}
	toi := x.chuGhe(g) == x.ten
	switch soDo[i][c-1] {
	case 'g':
		if toi {
			return "toi_giu"
		}
		return "giu"
	case 'b':
		if toi {
			return "ve"
		}
		return "ban"
	}
	return "trong"
}

// bam – bấm một ghế (người dùng hoặc kịch bản demo).
func (x *cuaSoDatVe) bam(g string) {
	if x.kn == nil {
		return
	}
	trang := x.trangGhe(g)
	switch {
	case x.chon[g]:
		delete(x.chon, g)
	case trang == "trong" && !x.giu[g]:
		if len(x.chon)+len(x.giu) >= 8 {
			x.bao("Tối đa 8 ghế mỗi lần", mau("#ffe0b2"), Den)
			return
		}
		x.chon[g] = true
	case trang == "giu":
		x.bao("Ghế "+g+" đang có người giữ", mau("#ffe0b2"), Den)
	case trang == "ban":
		x.bao("Ghế "+g+" đã bán", mau("#e0e0e0"), Den)
	}
	x.capNhatGio()
}

func (x *cuaSoDatVe) giaVe() int64 {
	if x.tt == nil {
		return 0
	}
	return tc.SoNguyen(x.tt["gia_ve"])
}

func (x *cuaSoDatVe) capNhatGio() {
	gia := x.giaVe()
	if len(x.giu) > 0 {
		datChu(x.nhanGhe, "Đang giữ: "+strings.Join(sapXep(x.giu), ", "), Den)
		datChu(x.nhanTien, "Tổng: "+tc.Tien(gia*int64(len(x.giu))), Den)
		x.nutTT.SetText("THANH TOÁN " + tc.Tien(gia*int64(len(x.giu))))
	} else {
		ds := strings.Join(sapXep(x.chon), ", ")
		if ds == "" {
			ds = "–"
		}
		datChu(x.nhanGhe, "Ghế đã chọn: "+ds, Den)
		datChu(x.nhanTien, "Tạm tính: "+tc.Tien(gia*int64(len(x.chon))), Den)
		x.nutTT.SetText("THANH TOÁN")
	}
	x.batNut()
	x.veGhe()
}

func (x *cuaSoDatVe) batNut() {
	co := x.kn != nil
	conHan := x.han != nil && time.Now().Before(*x.han)
	batTat(co && len(x.chon) > 0 && len(x.giu) == 0, x.nutGiu)
	batTat(co && len(x.giu) > 0 && conHan, x.nutTT)
	batTat(co && len(x.giu) > 0, x.nutNha)
}

func (x *cuaSoDatVe) bao(text string, nen, mauChu color.Color) { x.thongBao.dat(text, nen, mauChu) }

// ------------------------------------------------------------ giữ / thanh toán / nhả

func (x *cuaSoDatVe) giuCho() {
	if len(x.chon) == 0 || len(x.giu) > 0 {
		return
	}
	ghe := sapXep(x.chon)
	x.log.them(fmt.Sprintf("Giữ chỗ %s…", strings.Join(ghe, ", ")))
	x.bao("Đang giữ chỗ…", VangNhat, Den)
	x.nutGiu.Disable()
	x.guiYC("giu_cho", tc.T("ghe", ghe))
}

func (x *cuaSoDatVe) thanhToan() {
	if len(x.giu) > 0 {
		x.log.them(fmt.Sprintf("Thanh toán %s…", strings.Join(sapXep(x.giu), ", ")))
		x.guiYC("thanh_toan", tc.T("ghe", sapXep(x.giu)))
	}
}

func (x *cuaSoDatVe) nha() {
	if len(x.giu) > 0 {
		x.guiYC("nha", tc.T("ghe", sapXep(x.giu)))
	}
}

func (x *cuaSoDatVe) ketQua(loai string, truong tc.TS, tin tc.M, rtt float64) {
	gv, _ := truong.Lay("ghe")
	ghe := gv.([]string)
	ok, _ := tin["ok"].(bool)
	switch loai {
	case "giu_cho":
		if ok {
			x.soGiu++
			x.giu = map[string]bool{}
			for _, g := range tc.DSChuoi(tin["ghe"]) {
				x.giu[g] = true
				delete(x.chon, g)
			}
			x.phienGiu = tc.SoNguyen(tin["phien"])
			h := time.Now().Add(time.Duration(tc.SoThuc(tin["het_han_ms"]) * float64(time.Millisecond)))
			x.han = &h
			x.bao(fmt.Sprintf("Đã giữ %s – thanh toán trong %.0f s", strings.Join(ghe, ", "), tc.SoThuc(tin["het_han_ms"])/1000),
				mau("#c8f0d2"), XanhDam)
			x.log.them(fmt.Sprintf("  ĐÃ GIỮ %s (%.1f ms)", strings.Join(ghe, ", "), rtt))
			x.kiemGhe(x.tt)
		} else {
			bo := tc.DSChuoi(tin["ghe_ban"])
			if _, co := tin["ghe_ban"]; !co {
				bo = ghe
			}
			for _, g := range bo {
				delete(x.chon, g)
			}
			x.bao("Không giữ được: "+tc.StrPy(tin["ly_do"]), mau("#ffd0d0"), DoDam)
			x.log.them("  không giữ được: " + tc.StrPy(tin["ly_do"]))
		}
	case "thanh_toan":
		if ok {
			x.soVe += len(ghe)
			for _, g := range ghe {
				x.veCua[g] = true
				delete(x.giu, g)
			}
			x.phienVe = tc.SoNguyen(tin["phien"])
			x.han = nil
			x.bao("Đặt vé thành công! "+tc.Tien(tc.SoNguyen(tin["tien"])), mau("#c8f0d2"), XanhDam)
			x.log.them(fmt.Sprintf("  ĐẶT VÉ THÀNH CÔNG: %s (%.1f ms)", strings.Join(tc.DSChuoi(tin["ma_ve"]), ", "), rtt))
		} else {
			x.bao("Thanh toán bị từ chối: "+tc.StrPy(tin["ly_do"]), DoSP, Trang)
			x.log.them("  thanh toán bị từ chối: " + tc.StrPy(tin["ly_do"]))
		}
	default:
		if ok {
			nha := tc.DSChuoi(tin["ghe"])
			for _, g := range nha {
				delete(x.giu, g)
			}
			if len(x.giu) == 0 {
				x.han = nil
			}
			x.bao("Đã nhả "+strings.Join(nha, ", "), mau("#e8e8e8"), Den)
			x.log.them("  đã nhả " + strings.Join(nha, ", "))
		} else {
			x.log.them("  nhả bị từ chối: " + tc.StrPy(tin["ly_do"]))
		}
	}
	x.veLaiVe()
	x.capNhatGio()
}

func (x *cuaSoDatVe) veLaiVe() {
	ds := strings.Join(sapXep(x.veCua), ", ")
	if ds == "" {
		ds = "–"
	}
	x.nhanVe.SetText(fmt.Sprintf("Vé của bạn: %s\nBạn: giữ được %d lần, mua %d vé", ds, x.soGiu, x.soVe))
}

// kiemGhe – ghế mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; ghế đang chọn bị người khác giữ.
// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).
func (x *cuaSoDatVe) kiemGhe(tt tc.M) {
	if tt == nil {
		return
	}
	phien := tc.SoNguyen(tt["phien"])
	if phien >= x.phienGiu {
		for _, g := range sapXep(x.giu) {
			ai := x.chuGhe(g)
			if ai == x.ten {
				continue
			}
			delete(x.giu, g)
			if ai != "" {
				x.bao(fmt.Sprintf("Ghế %s đã bị %s lấy mất!", g, ai), DoSP, Trang)
				x.log.them(fmt.Sprintf("Ghế %s đã bị %s lấy mất!", g, ai))
			} else {
				x.bao("Hết thời gian giữ chỗ – ghế đã được nhả", mau("#ffd0d0"), DoDam)
				x.log.them("Hết thời gian giữ chỗ: " + g)
			}
		}
		if len(x.giu) == 0 {
			x.han = nil
		}
	}
	if phien >= x.phienVe {
		for _, g := range sapXep(x.veCua) {
			if ai := x.chuGhe(g); ai != x.ten {
				delete(x.veCua, g)
				if ai == "" {
					ai = "không ai"
				}
				x.bao(fmt.Sprintf("Vé %s bị ghi cho %s!", g, ai), DoSP, Trang)
				x.log.them(fmt.Sprintf("Vé %s bị ghi cho %s!", g, ai))
			}
		}
	}
	var biLay []string
	for _, g := range sapXep(x.chon) {
		if x.trangGhe(g) != "trong" {
			biLay = append(biLay, g)
			delete(x.chon, g)
		}
	}
	if len(biLay) > 0 {
		x.bao(fmt.Sprintf("Ghế %s vừa có người giữ", strings.Join(biLay, ", ")), mau("#ffe0b2"), Den)
		x.log.them(fmt.Sprintf("Ghế %s vừa có người giữ", strings.Join(biLay, ", ")))
	}
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoDatVe) capNhat(tt tc.M) {
	if k := fmt.Sprint(tt["phim"], tt["ttl_s"], tt["gia_ve"], tt["suat"]); k != x.phimCu {
		x.vePhim(tt)
		x.phimCu = k
	}
	if x.thongBao.text() == "Chưa kết nối" {
		x.bao("Chọn ghế rồi bấm GIỮ CHỖ", mau("#e8e8e8"), Den)
	}
	dem, _ := tt["dem"].(tc.M)
	chu, _ := tt["chu"].(tc.M)
	if len(chu) == 0 && (len(x.veCua) > 0 || len(x.giu) > 0) && tc.SoNguyen(dem["ban"]) == 0 && tc.SoNguyen(dem["giu"]) == 0 {
		x.veCua = map[string]bool{} // máy chủ đặt lại: rạp trống
	}
	conLai, _ := tt["con_lai_ms"].(tc.M)
	minMs := math.Inf(1)
	for g := range x.giu {
		if v, co := conLai[g]; co && chu[g] == x.ten {
			minMs = math.Min(minMs, tc.SoThuc(v))
		}
	}
	if !math.IsInf(minMs, 1) {
		h := time.Now().Add(time.Duration(minMs * float64(time.Millisecond)))
		x.han = &h
	}
	x.kiemGhe(tt)
	x.veLaiVe()
	x.capNhatGio()
}

func (x *cuaSoDatVe) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoDatVe) moiNhip() {
	if x.han == nil {
		datChu(x.nhanDongHo, "--:--", mau("#909090"))
		return
	}
	con := time.Until(*x.han).Seconds()
	m := XanhDam
	if con < 10 {
		m = DoDam
	}
	datChu(x.nhanDongHo, dongHoGiu(con), m)
	if con <= 0 && !x.nutTT.Disabled() {
		x.batNut()
		x.bao("Hết thời gian giữ chỗ", mau("#ffd0d0"), DoDam)
	}
}

func (x *cuaSoDatVe) dongDinhKy() string {
	d, _ := x.tt["dem"].(tc.M)
	con := "–"
	if x.han != nil {
		con = fmt.Sprintf("%.0f s", math.Max(0, time.Until(*x.han).Seconds()))
	}
	giu, ve := strings.Join(sapXep(x.giu), ", "), strings.Join(sapXep(x.veCua), ", ")
	if giu == "" {
		giu = "–"
	}
	if ve == "" {
		ve = "–"
	}
	return fmt.Sprintf("trống %d, giữ %d, bán %d; bạn giữ %s (còn %s), vé %s", tc.SoNguyen(d["trong"]),
		tc.SoNguyen(d["giu"]), tc.SoNguyen(d["ban"]), giu, con, ve)
}

func (x *cuaSoDatVe) daKetNoi() { x.batNut() }

func (x *cuaSoDatVe) matKetNoi() { x.batNut() }
