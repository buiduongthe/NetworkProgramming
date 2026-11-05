package main

// Ứng dụng gọi xe (người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu) ở
// chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ ứng dụng gọi xe: bản đồ phố (nền vẽ bằng bút ảnh ở ve.go:
// đường, công viên, hồ, nhà; tên phố là chữ Fyne), điểm đón (xanh) và điểm đến (đỏ) – bấm chuột trái/phải
// lên bản đồ để chọn ngã tư –, tuyến đường, các xe (màu theo xe, xe đã có khách mờ đi), giá cước dự
// kiến, nút ĐẶT XE, trạng thái "Đang tìm tài xế…" / "Đặt thành công – tài xế …" / "Hết chỗ" / "Tài xế …
// đã nhận khách khác!", thẻ tài xế và nút Hủy chuyến. Đặt được thì xe chạy hoạt hình (40 ms/khung) tới
// điểm đón rồi tới điểm đến. Mọi số liệu về xe đến từ cap_nhat của máy chủ.

import (
	"fmt"
	"image/color"
	"math"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.7_DatXe/tranhchap"
)

var mauXe = map[string]string{"trắng": "#f4f4f4", "đỏ": "#e53935", "xanh": "#1e88e5", "bạc": "#b0bec5",
	"vàng": "#fdd835", "đen": "#37474f", "cam": "#fb8c00", "tím": "#8e24aa", "xám": "#78909c", "hồng": "#f06292"}

var (
	xanhGrab  = mau("#00a651")
	xanhTuyen = mau("#1a73e8")
	doPin     = mau("#d93025")
)

const tocDo = 0.28 // đơn vị bản đồ / giây (hoạt hình)

// duongDi – đường đi trên lưới phố từ a tới b: đi ngang trước rồi đi dọc.
func duongDi(a, b [2]float64) [][2]float64 {
	ds := [][2]float64{a}
	if a[0] != b[0] && a[1] != b[1] {
		ds = append(ds, [2]float64{b[0], a[1]})
	}
	return append(ds, b)
}

func doDai(ds [][2]float64) float64 {
	s := 0.0
	for i := 0; i+1 < len(ds); i++ {
		s += math.Abs(ds[i][0]-ds[i+1][0]) + math.Abs(ds[i][1]-ds[i+1][1])
	}
	return s
}

// diemTren – điểm cách đầu đường đi một đoạn s (đơn vị bản đồ) và hướng ("ngang"/"doc").
func diemTren(ds [][2]float64, s float64) ([2]float64, string) {
	for i := 0; i+1 < len(ds); i++ {
		p, q := ds[i], ds[i+1]
		d := math.Abs(p[0]-q[0]) + math.Abs(p[1]-q[1])
		if s <= d || i+2 == len(ds) {
			t := 0.0
			if d > 0 {
				t = math.Min(1, s/d)
			}
			huong := "doc"
			if p[1] == q[1] {
				huong = "ngang"
			}
			return [2]float64{p[0] + (q[0]-p[0])*t, p[1] + (q[1]-p[1])*t}, huong
		}
		s -= d
	}
	return ds[len(ds)-1], "ngang"
}

// doan – phần đường đi từ khoảng cách s0 tới s1 (danh sách điểm).
func doan(ds [][2]float64, s0, s1 float64) [][2]float64 {
	p0, _ := diemTren(ds, s0)
	kq, tich := [][2]float64{p0}, 0.0
	for i := 0; i+1 < len(ds); i++ {
		tich += math.Abs(ds[i][0]-ds[i+1][0]) + math.Abs(ds[i][1]-ds[i+1][1])
		if s0 < tich && tich < s1 {
			kq = append(kq, ds[i+1])
		}
	}
	p1, _ := diemTren(ds, s1)
	return append(kq, p1)
}

// ---------------------------------------------------------------- bản đồ

// banDo – bản đồ phố (dùng cả ở ứng dụng gọi xe và bảng điều khiển máy chủ): nền tĩnh là ảnh, các đối
// tượng động (tuyến, ghim, xe) là đối tượng Fyne đặt tự do trong container không bố cục.
type banDo struct {
	w, h float32
	nho  bool
	dong *fyne.Container
	O    fyne.CanvasObject
}

func moiBanDo(w, h float32, nho bool, bamTrai, bamPhai func(fyne.Position)) *banDo {
	bd := &banDo{w: w, h: h, nho: nho, dong: container.NewWithoutLayout()}
	nen := anhNenBanDo(int(w), int(h), nho)
	tinh := container.NewWithoutLayout()
	if !nho {
		for i, x := range tc.Duong { // tên phố: giữa hai ngã tư
			y := 0.6
			if i%2 == 1 {
				y = 0.4
			}
			bd.chuTai(tinh, tc.TenDoc[i], float32(x)*w+8, float32(y)*h, mau("#7a7468"), 9, false)
		}
		for i, y := range tc.Duong {
			x := 0.6
			if i%2 == 1 {
				x = 0.4
			}
			t := chu(tc.TenNgang[i], mau("#7a7468"), 9, false)
			tinh.Add(t)
			t.Move(fyne.NewPos(float32(x)*w-t.MinSize().Width/2, float32(y)*h-18))
		}
		bd.chuTai(tinh, "Công viên", 0.40*w-22, 0.88*h-16, mau("#3d7a2a"), 9, false)
		bd.chuTai(tinh, "Hồ", 0.80*w-6, 0.20*h-7, mau("#2a6fa8"), 9, false)
	}
	lop := []fyne.CanvasObject{nen, tinh, bd.dong}
	if bamTrai != nil || bamPhai != nil {
		lop = append(lop, moiVungBam(bamTrai, bamPhai))
	}
	bd.O = coDinh(container.NewStack(lop...), w, h)
	return bd
}

func (bd *banDo) chuTai(c *fyne.Container, s string, x, y float32, m color.Color, co float32, dam bool) *canvas.Text {
	t := chu(s, m, co, dam)
	c.Add(t)
	t.Move(fyne.NewPos(x, y))
	return t
}

func (bd *banDo) p(x, y float64) (float32, float32) { return float32(x) * bd.w, float32(y) * bd.h }

// anhNenBanDo – khu phố (nhà), công viên, hồ, đường dọc/ngang (đường chính màu vàng).
func anhNenBanDo(w, h int, nho bool) *canvas.Image {
	b := moiBut(w, h)
	W, H := float64(w), float64(h)
	b.chuNhat(0, 0, W, H, mau("#ebe7df"))
	bien := append(append([]float64{0}, tc.Duong...), 1)
	o := func(x0, y0, x1, y1 float64) (float64, float64, float64, float64) {
		le := 0.022
		return (x0 + le) * W, (y0 + le) * H, (x1 - le) * W, (y1 - le) * H
	}
	for i := 0; i+1 < len(bien); i++ {
		for j := 0; j+1 < len(bien); j++ {
			a, bb, c, d := o(bien[i], bien[j], bien[i+1], bien[j+1])
			if c-a < 6 || d-bb < 6 {
				continue
			}
			b.chuNhat(a, bb, c, d, mau("#e2ddd2"))
			if !nho && (i+j)%2 == 0 {
				b.chuNhat(a+4, bb+4, (a+c)/2-2, (bb+d)/2-2, mau("#d6d0c2"))
			}
		}
	}
	a, bb, c, d := o(0.3, 0.7, 0.5, 0.9) // công viên
	b.chuNhat(a, bb, c, d, mau("#bfe3a6"))
	for k := 0; k < 5; k++ {
		y := 0.65
		if k%2 == 1 {
			y = 0.35
		}
		r := 6.0
		if nho {
			r = 3
		}
		b.tron(a+(c-a)*(0.2+0.15*float64(k)), bb+(d-bb)*y, r, mau("#7cbf5a"))
	}
	a, bb, c, d = o(0.7, 0.1, 0.9, 0.3) // hồ
	b.elip(a, bb, c, d, pm(mau("#a9d3f2")), pm(mau("#8cc2ea")), 1, false)
	rong := 12.0
	if nho {
		rong = 7
	}
	for _, x := range tc.Duong { // đường dọc và ngang
		m, r := mau("#ffffff"), rong
		if x == 0.5 {
			m, r = mau("#fde293"), rong+2
		}
		b.chuNhat(x*W-r/2, 0, x*W+r/2, H, m)
		b.chuNhat(0, x*H-r/2, W, x*H+r/2, m)
	}
	return oAnhTinh(b.anh(), float32(w), float32(h))
}

func (bd *banDo) xoa() { bd.dong.Objects = nil }

func (bd *banDo) them(o fyne.CanvasObject, x, y, w, h float32) {
	o.Move(fyne.NewPos(x, y))
	o.Resize(fyne.NewSize(w, h))
	bd.dong.Add(o)
}

func (bd *banDo) doanThang(x0, y0, x1, y1 float32, m color.Color, day float32) {
	l := canvas.NewLine(m)
	l.StrokeWidth = day
	l.Position1, l.Position2 = fyne.NewPos(x0, y0), fyne.NewPos(x1, y1)
	bd.dong.Add(l)
	r := canvas.NewCircle(m) // khớp tròn ở đầu đoạn
	bd.them(r, x1-day/2, y1-day/2, day, day)
}

// tuyen – tuyến đường; net = nét đứt (xe đang tới điểm đón).
func (bd *banDo) tuyen(ds [][2]float64, m color.Color, net bool) {
	if len(ds) < 2 {
		return
	}
	rong := float32(5)
	if bd.nho {
		rong = 3
	}
	for i := 0; i+1 < len(ds); i++ {
		x0, y0 := bd.p(ds[i][0], ds[i][1])
		x1, y1 := bd.p(ds[i+1][0], ds[i+1][1])
		if !net {
			bd.doanThang(x0, y0, x1, y1, mau("#0b4fb3"), rong+2)
			continue
		}
		dai := float32(math.Hypot(float64(x1-x0), float64(y1-y0)))
		for s := float32(0); s < dai; s += 12 {
			e := min(s+7, dai)
			bd.doanThang(x0+(x1-x0)*s/dai, y0+(y1-y0)*s/dai, x0+(x1-x0)*e/dai, y0+(y1-y0)*e/dai, m, rong)
		}
	}
	if !net {
		for i := 0; i+1 < len(ds); i++ {
			x0, y0 := bd.p(ds[i][0], ds[i][1])
			x1, y1 := bd.p(ds[i+1][0], ds[i+1][1])
			bd.doanThang(x0, y0, x1, y1, m, rong)
		}
	}
}

// ghim – ghim điểm đón/đến: chân nhọn + đầu tròn viền trắng + chấm trắng, kèm chữ.
func (bd *banDo) ghim(x, y float64, m color.Color, nhan string) {
	px, py := bd.p(x, y)
	r := float32(9)
	if bd.nho {
		r = 5
	}
	chan_ := canvas.NewLine(m)
	chan_.StrokeWidth = r * 0.8
	chan_.Position1, chan_.Position2 = fyne.NewPos(px, py-r*1.4), fyne.NewPos(px, py)
	bd.dong.Add(chan_)
	dau := canvas.NewCircle(m)
	dau.StrokeColor, dau.StrokeWidth = Trang, 2
	bd.them(dau, px-r, py-r*2.6, 2*r, 2*r)
	bd.them(canvas.NewCircle(Trang), px-r*0.35, py-r*1.95, r*0.7, r*0.7)
	if nhan != "" && !bd.nho {
		t := chu(nhan, m, 11, true)
		bd.them(t, px+r+3, py-r*2.3, t.MinSize().Width, t.MinSize().Height)
	}
}

// xe – một chiếc xe: bóng, thân (màu theo xe; mờ nếu đã có khách), kính lái, kính sau, nhãn.
func (bd *banDo) xe(x, y float64, mauTen, nhan, huong string, mo, noiBat bool) {
	px, py := bd.p(x, y)
	dai, ngan := float32(24), float32(13)
	if bd.nho {
		dai, ngan = 14, 8
	}
	rx, ry := dai/2, ngan/2
	if huong != "ngang" {
		rx, ry = ngan/2, dai/2
	}
	if noiBat {
		r := dai * 0.85
		v := canvas.NewCircle(color.Transparent)
		v.StrokeColor, v.StrokeWidth = xanhGrab, 3
		bd.them(v, px-r, py-r, 2*r, 2*r)
	}
	than := mau(mauXe[mauTen])
	vien := color.NRGBA{0x20, 0x20, 0x20, 0xFF}
	if mauXe[mauTen] == "" {
		than = mau("#f4f4f4")
	}
	if mo {
		than, vien = mau("#d0d0d0"), mau("#555555")
	}
	bd.them(canvas.NewRectangle(mau("#9a9488")), px-rx+1, py-ry+2, 2*rx, 2*ry)
	tt := canvas.NewRectangle(than)
	tt.StrokeColor, tt.StrokeWidth = vien, 1
	bd.them(tt, px-rx, py-ry, 2*rx, 2*ry)
	kinh := mau("#5c6f80")
	if huong == "ngang" {
		bd.them(canvas.NewRectangle(kinh), px+rx*0.25, py-ry+2, rx*0.3, 2*ry-4)
		bd.them(canvas.NewRectangle(kinh), px-rx*0.7, py-ry+2, rx*0.25, 2*ry-4)
	} else {
		bd.them(canvas.NewRectangle(kinh), px-rx+2, py+ry*0.25, 2*rx-4, ry*0.3)
		bd.them(canvas.NewRectangle(kinh), px-rx+2, py-ry*0.7, 2*rx-4, ry*0.25)
	}
	if nhan != "" {
		m := color.Color(Den)
		if mo {
			m = mau("#707070")
		}
		co := float32(10)
		if bd.nho {
			co = 9
		}
		t := chu(nhan, m, co, true)
		kt := t.MinSize()
		bd.them(t, px-kt.Width/2, py-max(rx, ry)-kt.Height, kt.Width, kt.Height)
	}
}

func (bd *banDo) lamMoi() { bd.dong.Refresh() }

// ---------------------------------------------------------------- cửa sổ gọi xe

type hoatHinh struct {
	ds       [][2]float64
	t0       time.Time
	dai, moc float64 // độ dài cả tuyến, mốc tới điểm đón
}

type cuaSoGoiXe struct {
	*cuaSoNguoiDungCoSo
	don, den      [2]float64
	trang         string // san_sang | dang_tim | da_dat | dang_huy | het_cho | mat_xe | xong
	xeToi         int64  // mã xe được báo đặt thành công (0 = không)
	chuyen        tc.M   // thông tin chuyến từ kq
	phienToi      int64
	hh            *hoatHinh
	pha           string
	tTim          time.Time
	soDat, soDuoc int
	bd            *banDo
	nhanDon       *canvas.Text
	nhanDen       *canvas.Text
	nhanCuoc      *canvas.Text
	nutDat        *widget.Button
	nhanTT        *oMau
	avatar        *canvas.Circle
	chuAvatar     *canvas.Text
	nhanTX        *canvas.Text
	nhanTX2       *widget.Label
	nutHuy        *widget.Button
	nhanXeTrong   *canvas.Text
	nhanCuaToi    *canvas.Text
}

func moCuaSoGoiXe(a fyne.App, ten, host string, cong int) *cuaSoGoiXe {
	x := &cuaSoGoiXe{trang: "san_sang"}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Gọi xe", ten, host, cong, x)
	x.don, x.den = diemMacDinh(ten)
	x.hoanTat(780)
	go func() { // hoạt hình 40 ms
		t := time.NewTicker(40 * time.Millisecond)
		defer t.Stop()
		for {
			select {
			case <-x.dong:
				return
			case <-t.C:
			}
			fyne.Do(x.nhipHoatHinh)
		}
	}()
	return x
}

// diemMacDinh – điểm đón và điểm đến cố định theo tên (như bản dòng lệnh).
func diemMacDinh(ten string) ([2]float64, [2]float64) {
	s := 0
	for _, c := range ten {
		s += int(c)
	}
	don := tc.NgaTu[(s*7)%len(tc.NgaTu)]
	den := tc.NgaTu[(s*11+9)%len(tc.NgaTu)]
	if tc.Km(don, den) < 5 {
		den = [2]float64{0.1, 0.1}
		if don[0] < 0.5 {
			den[0] = 0.9
		}
		if don[1] < 0.5 {
			den[1] = 0.9
		}
	}
	return don, den
}

func (x *cuaSoGoiXe) ve() fyne.CanvasObject {
	const rong, cao = 440, 340
	x.bd = moiBanDo(rong, cao, false, func(p fyne.Position) { x.chon(p, "don") }, func(p fyne.Position) { x.chon(p, "den") })
	trai := container.NewVBox(x.bd.O, chu("Chuột trái: chọn điểm đón · chuột phải: chọn điểm đến", mau("#404040"), 10, false))

	x.nhanDon = chu("", xanhGrab, 12, false)
	x.nhanDen = chu("", doPin, 12, false)
	x.nhanCuoc = chu("", Den, 13, true)
	x.nutDat = nut("ĐẶT XE", widget.SuccessImportance, x.datXe)
	x.nhanTT = moiOMau("Chưa kết nối", mau("#e8e8e8"), Den, 12, 300, 34, true)
	x.avatar = canvas.NewCircle(color.Transparent)
	x.chuAvatar = chu("", Trang, 20, true)
	x.nhanTX = chu("Tài xế: –", Den, 13, true)
	x.nhanTX2 = widget.NewLabel("")
	the := container.NewStack(canvas.NewRectangle(mau("#f5f7f6")), container.NewHBox(
		coDinh(container.NewStack(x.avatar, container.NewCenter(x.chuAvatar)), 44, 44),
		container.NewVBox(x.nhanTX, x.nhanTX2)))
	x.nutHuy = nut("Hủy chuyến", widget.MediumImportance, x.huy)
	x.nhanXeTrong = chu("", Den, 12, false)
	x.nhanCuaToi = chu("", mau("#505050"), 10, false)
	phai := container.NewStack(canvas.NewRectangle(Trang), container.NewPadded(container.NewVBox(
		chu("Bạn muốn đi đâu?", Den, 15, true), x.nhanDon, x.nhanDen, x.nhanCuoc, coDinh(x.nutDat, 300, 40),
		x.nhanTT.O, coDinh(the, 300, 70), container.NewHBox(x.nutHuy, layout.NewSpacer()), x.nhanXeTrong,
		x.nhanCuaToi)))
	x.veDiem()
	x.batNut()
	x.veDong()
	return container.NewHBox(trai, coDinh(phai, 316, cao+20))
}

// ------------------------------------------------------------ chọn điểm

func (x *cuaSoGoiXe) chon(p fyne.Position, loai string) {
	if x.trang == "dang_tim" || x.trang == "da_dat" {
		return
	}
	px, py := float64(p.X/x.bd.w), float64(p.Y/x.bd.h)
	gan, kc := tc.NgaTu[0], math.Inf(1)
	for _, q := range tc.NgaTu {
		if d := (q[0]-px)*(q[0]-px) + (q[1]-py)*(q[1]-py); d < kc {
			gan, kc = q, d
		}
	}
	if loai == "don" && gan != x.den {
		x.don = gan
	} else if loai == "den" && gan != x.don {
		x.den = gan
	}
	x.veDiem()
	x.veDong()
}

func (x *cuaSoGoiXe) veDiem() {
	datChu(x.nhanDon, "● Đón:  "+tc.TenDiem(x.don), xanhGrab)
	datChu(x.nhanDen, "● Đến:  "+tc.TenDiem(x.den), doPin)
	datChu(x.nhanCuoc, fmt.Sprintf("%.1f km · cước dự kiến %s", tc.Km(x.don, x.den), tc.Tien(tc.Cuoc(x.don, x.den))), Den)
}

func (x *cuaSoGoiXe) batNut() {
	coTheDat := x.kn != nil && (x.trang == "san_sang" || x.trang == "het_cho" || x.trang == "mat_xe" || x.trang == "xong")
	batTat(coTheDat, x.nutDat)
	batTat(x.trang == "da_dat", x.nutHuy)
	chuHuy := "Hủy chuyến"
	if x.hh != nil && x.daToi() {
		chuHuy = "Hoàn tất chuyến"
	}
	if x.nutHuy.Text != chuHuy {
		x.nutHuy.SetText(chuHuy)
	}
}

func (x *cuaSoGoiXe) bao(text string, nen, mauChu color.Color) { x.nhanTT.dat(text, nen, mauChu) }

// ------------------------------------------------------------ đặt / hủy

func (x *cuaSoGoiXe) datXe() {
	if x.kn == nil || x.trang == "dang_tim" || x.trang == "da_dat" {
		return
	}
	x.trang = "dang_tim"
	x.soDat++
	x.tTim = time.Now()
	x.bao("Đang tìm tài xế…", VangNhat, Den)
	x.log.them(fmt.Sprintf("Đặt xe: %s -> %s", tc.TenDiem(x.don), tc.TenDiem(x.den)))
	x.batNut()
	x.guiYC("dat_xe", tc.T("don", []any{x.don[0], x.don[1]}, "den", []any{x.den[0], x.den[1]}))
}

func (x *cuaSoGoiXe) huy() {
	if x.trang == "da_dat" {
		x.trang = "dang_huy"
		x.batNut()
		x.guiYC("huy", nil)
	}
}

func (x *cuaSoGoiXe) ketQua(loai string, _ tc.TS, tin tc.M, rtt float64) {
	ok, _ := tin["ok"].(bool)
	if loai == "dat_xe" {
		switch {
		case ok:
			x.soDuoc++
			x.trang, x.xeToi, x.chuyen, x.phienToi = "da_dat", tc.SoNguyen(tin["xe"]), tin, tc.SoNguyen(tin["phien"])
			x.bao("Đặt thành công – tài xế "+tc.Chuoi(tin["tai_xe"]), mau("#c8f0d2"), XanhDam)
			x.log.them(fmt.Sprintf("  ĐẶT THÀNH CÔNG: %s %s, đến sau %d phút (%.1f ms)", tc.Chuoi(tin["tai_xe"]),
				tc.Chuoi(tin["bien_so"]), tc.SoNguyen(tin["phut"]), rtt))
			x.theTaiXe(tin)
			goc := x.don
			if xe := x.xe(x.xeToi); xe != nil {
				goc = viTri(xe)
			}
			ds := duongDi(goc, x.don)
			moc := doDai(ds)
			ds = append(ds, duongDi(x.don, x.den)[1:]...)
			x.hh = &hoatHinh{ds, time.Now(), doDai(ds), moc}
			x.kiemXe(x.tt)
		case tin["ma_loi"] == "het_cho":
			x.trang = "het_cho"
			x.bao("Hết chỗ – không còn xe trống", mau("#ffd0d0"), DoDam)
			x.log.them(fmt.Sprintf("  HẾT CHỖ (%.1f ms)", rtt))
		default:
			x.trang = "het_cho"
			x.bao("Không đặt được: "+tc.StrPy(tin["ly_do"]), mau("#ffd0d0"), DoDam)
			x.log.them("  từ chối: " + tc.StrPy(tin["ly_do"]))
		}
	} else {
		if ok {
			xong := x.hh != nil && x.daToi()
			if xong {
				x.log.them("Hoàn tất chuyến, cảm ơn bạn!")
				x.bao("Hoàn tất chuyến – cảm ơn bạn!", mau("#e8e8e8"), Den)
			} else {
				x.log.them(fmt.Sprintf("Đã hủy chuyến (%s)", tc.Chuoi(tin["tai_xe"])))
				x.bao("Đã hủy chuyến", mau("#e8e8e8"), Den)
			}
			x.trang = "xong"
		} else {
			x.log.them("  hủy bị từ chối: " + tc.StrPy(tin["ly_do"]))
			x.trang = "san_sang"
		}
		x.xeToi, x.chuyen, x.hh = 0, nil, nil
		x.theTaiXe(nil)
	}
	datChu(x.nhanCuaToi, fmt.Sprintf("Bạn: bấm đặt %d lần, được báo thành công %d", x.soDat, x.soDuoc), x.nhanCuaToi.Color)
	x.batNut()
	x.veDong()
}

func (x *cuaSoGoiXe) theTaiXe(tin tc.M) {
	if tin == nil {
		x.avatar.FillColor = color.Transparent
		x.avatar.Refresh()
		datChu(x.chuAvatar, "", Trang)
		datChu(x.nhanTX, "Tài xế: –", Den)
		x.nhanTX2.SetText("")
		return
	}
	ten := tc.Chuoi(tin["tai_xe"])
	x.avatar.FillColor = xanhGrab
	x.avatar.Refresh()
	datChu(x.chuAvatar, string([]rune(ten)[:1]), Trang)
	datChu(x.nhanTX, ten+"  ★ 4.9", Den)
	x.nhanTX2.SetText(fmt.Sprintf("%s · xe %s\nđến sau %d phút · %s", tc.Chuoi(tin["bien_so"]), tc.Chuoi(tin["mau"]),
		tc.SoNguyen(tin["phut"]), tc.Tien(tc.SoNguyen(tin["cuoc"]))))
}

func viTri(xe tc.M) [2]float64 {
	v, _ := xe["vi_tri"].([]any)
	if len(v) != 2 {
		return [2]float64{0.5, 0.5}
	}
	return [2]float64{tc.SoThuc(v[0]), tc.SoThuc(v[1])}
}

func (x *cuaSoGoiXe) xe(ma int64) tc.M {
	if x.tt == nil {
		return nil
	}
	for _, xe := range tc.DSM(x.tt["xe"]) {
		if tc.SoNguyen(xe["ma"]) == ma {
			return xe
		}
	}
	return nil
}

// kiemXe – chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq). Xe mình được báo đặt
// thành công mà lại ghi khách khác: bị "cướp" chỗ (tranh chấp); xe trống: máy chủ đặt lại.
func (x *cuaSoGoiXe) kiemXe(tt tc.M) {
	if x.trang != "da_dat" || x.xeToi == 0 || tt == nil || tc.SoNguyen(tt["phien"]) < x.phienToi {
		return
	}
	xe := x.xe(x.xeToi)
	khach, _ := xe["khach"].(string)
	if xe != nil && khach == x.ten {
		return
	}
	if xe != nil && khach != "" {
		x.trang, x.hh = "mat_xe", nil
		x.bao(fmt.Sprintf("Tài xế %s đã nhận khách khác (%s)!", tc.Chuoi(xe["tai_xe"]), khach), DoSP, Trang)
		x.log.them(fmt.Sprintf("Tài xế %s đã nhận khách khác: %s", tc.Chuoi(xe["tai_xe"]), khach))
	} else {
		x.trang = "san_sang"
		x.xeToi, x.chuyen, x.hh = 0, nil, nil
		x.theTaiXe(nil)
		x.bao("Chuyến đã kết thúc (máy chủ đặt lại)", mau("#e8e8e8"), Den)
	}
	x.batNut()
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoGoiXe) capNhat(tt tc.M) {
	datChu(x.nhanXeTrong, fmt.Sprintf("Xe trống quanh bạn: %d/%d", tc.SoNguyen(tt["trong"]), tc.SoNguyen(tt["so_xe"])), Den)
	if x.trang != "dang_tim" && x.nhanTT.text() == "Chưa kết nối" {
		x.bao("Chọn điểm đón và bấm ĐẶT XE", mau("#e8e8e8"), Den)
	}
	x.kiemXe(tt)
	x.batNut()
	x.veDong()
}

func (x *cuaSoGoiXe) daToi() bool {
	return time.Since(x.hh.t0).Seconds()*tocDo >= x.hh.dai
}

func (x *cuaSoGoiXe) veDong() {
	bd := x.bd
	bd.xoa()
	if x.hh != nil {
		s := time.Since(x.hh.t0).Seconds() * tocDo
		if s < x.hh.moc { // xe đang tới điểm đón: tuyến nét đứt
			bd.tuyen(doan(x.hh.ds, s, x.hh.moc), mau("#5f8fd8"), true)
			bd.tuyen(duongDi(x.don, x.den), xanhTuyen, false)
		} else {
			bd.tuyen(doan(x.hh.ds, s, x.hh.dai), xanhTuyen, false)
		}
	} else {
		bd.tuyen(duongDi(x.don, x.den), mau("#8fb4ec"), false)
	}
	bd.ghim(x.don[0], x.don[1], xanhGrab, "Đón")
	bd.ghim(x.den[0], x.den[1], doPin, "Đến")
	if x.tt != nil {
		for _, xe := range tc.DSM(x.tt["xe"]) {
			if tc.SoNguyen(xe["ma"]) == x.xeToi && x.hh != nil {
				continue
			}
			p := viTri(xe)
			bd.xe(p[0], p[1], tc.Chuoi(xe["mau"]), tc.Chuoi(xe["tai_xe"]), "ngang", xe["khach"] != nil, false)
		}
	}
	if x.hh != nil && x.chuyen != nil {
		p, huong := diemTren(x.hh.ds, math.Min(x.hh.dai, time.Since(x.hh.t0).Seconds()*tocDo))
		bd.xe(p[0], p[1], tc.Chuoi(x.chuyen["mau"]), tc.Chuoi(x.chuyen["tai_xe"]), huong, false, true)
	}
	bd.lamMoi()
}

func (x *cuaSoGoiXe) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoGoiXe) moiNhip() {
	if x.trang == "dang_tim" {
		cham := strings.Repeat(".", 1+int(time.Since(x.tTim).Seconds()*5)%3)
		x.bao("Đang tìm tài xế"+cham, VangNhat, Den)
	}
}

func (x *cuaSoGoiXe) nhipHoatHinh() {
	if x.hh == nil {
		x.pha = ""
		return
	}
	s := time.Since(x.hh.t0).Seconds() * tocDo
	pha := "den"
	if s < x.hh.moc {
		pha = "toi_don"
	} else if s < x.hh.dai {
		pha = "di"
	}
	if pha != x.pha {
		x.pha = pha
		if pha == "di" {
			x.bao(fmt.Sprintf("Tài xế %s đã đón bạn – đang đi", tc.Chuoi(x.chuyen["tai_xe"])), mau("#c8f0d2"), XanhDam)
			x.log.them("Tài xế đã tới điểm đón")
		} else if pha == "den" {
			x.bao("Đã tới nơi – bấm Hoàn tất chuyến", mau("#c8f0d2"), XanhDam)
			x.log.them("Đã tới " + tc.TenDiem(x.den))
			x.batNut()
		}
	}
	x.veDong()
}

func (x *cuaSoGoiXe) daKetNoi() { x.batNut() }

func (x *cuaSoGoiXe) matKetNoi() {
	x.trang = "san_sang"
	x.xeToi, x.chuyen, x.hh = 0, nil, nil
	x.batNut()
}

func (x *cuaSoGoiXe) dongDinhKy() string {
	tt := x.tt
	s := fmt.Sprintf("%d/%d xe trống; trạng thái %s", tc.SoNguyen(tt["trong"]), tc.SoNguyen(tt["so_xe"]), x.trang)
	if xe := x.xe(x.xeToi); x.xeToi != 0 && xe != nil {
		s += fmt.Sprintf(", xe %d %s", tc.SoNguyen(xe["ma"]), tc.Chuoi(xe["tai_xe"]))
	}
	return s + fmt.Sprintf("; bạn đặt %d, được %d", x.soDat, x.soDuoc)
}
