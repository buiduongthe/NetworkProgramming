package main

// Ứng dụng đặt vé xe khách theo chặng (người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi
// yêu cầu) ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ ứng dụng đặt vé: băng rôn nhà xe, chọn "Đi từ" –
// "Đến" (thanh tuyến Sài Gòn – Bảo Lộc – Đà Lạt – Nha Trang tô đoạn đã chọn), sơ đồ xe (giường nằm 2 tầng hoặc
// ghế ngồi 50 chỗ, có vô lăng ở đầu xe). Mỗi chỗ tô màu theo ĐOẠN đang chọn: trống / đang chọn / người khác
// giữ / đã bán / bạn giữ / vé của bạn, và có dải nhỏ ở chân chỗ, mỗi chặng một vạch, cho thấy chỗ đó bận ở
// chặng nào. Bấm chỗ để chọn, "GIỮ CHỖ" gửi giu_cho, đồng hồ giữ chỗ đếm ngược (đỏ khi < 10 s), "THANH TOÁN"
// và "Nhả chỗ". Sơ đồ cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây).

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"slices"
	"sort"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.14_DatVeXeKhach/tranhchap"
)

var (
	mTrong   = mau("#ffffff")
	mChon    = mau("#b9f6ca")
	mKhacGiu = mau("#ffa726")
	mDaBan   = mau("#9e9e9e")
	mToiGiu  = mau("#2e7d32")
	mVeToi   = mau("#1565c0")
	mauXe    = mau("#0b5d3b")
	nenXe    = mau("#e9efeb")
	mVien    = mau("#607d74")
	mChuToi  = mau("#263238")
)

var mauVach = map[byte]color.NRGBA{'.': mau("#dfe6e2"), 'g': mKhacGiu, 'b': mDaBan, 'G': mToiGiu, 'V': mVeToi}

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

func chuCua(tt tc.M, o string) string {
	chu, _ := tt["chu"].(tc.M)
	s, _ := chu[o].(string)
	return s
}

func dongCua(tt tc.M, g string) string {
	i := slices.Index(tc.DSChuoi(tt["ghe"]), g)
	soDo := tc.DSChuoi(tt["so_do"])
	if i < 0 || i >= len(soDo) {
		return strings.Repeat(".", tc.SoChang)
	}
	return soDo[i]
}

// trangCho – trạng thái chỗ g cho đoạn [tu, den): "trong" | "giu" | "ban" | "toi_giu" | "ve".
func trangCho(tt tc.M, g, ten string, tu, den int) string {
	dong := dongCua(tt, g)
	kq := "trong"
	for k := tu; k < den; k++ {
		x, ai := dong[k], chuCua(tt, fmt.Sprintf("%s#%d", g, k))
		switch {
		case x == 'b' && ai != ten:
			return "ban"
		case x == 'g' && ai != ten:
			kq = "giu"
		case x == 'g' && kq == "trong":
			kq = "toi_giu"
		case x == 'b' && (kq == "trong" || kq == "toi_giu"):
			kq = "ve"
		}
	}
	return kq
}

// vachCua – một ký tự mỗi chặng: . g b (người khác) G V (của mình).
func vachCua(tt tc.M, g, ten string) string {
	dong := []byte(dongCua(tt, g))
	for k, x := range dong {
		if chuCua(tt, fmt.Sprintf("%s#%d", g, k)) == ten {
			if x == 'g' {
				dong[k] = 'G'
			} else if x == 'b' {
				dong[k] = 'V'
			}
		}
	}
	return string(dong)
}

// ---------------------------------------------------------------- sơ đồ xe

// soDoXe – sơ đồ xe bằng đối tượng Fyne; dùng ở ứng dụng đặt vé (k = 1) và bảng máy chủ (thu nhỏ, k < 1).
type soDoXe struct {
	loaiXe    string
	k         float32
	nhan      bool
	rong, cao float32
	viTri     map[string][4]float32
	o         map[string]*canvas.Rectangle
	vach      map[string][]*canvas.Rectangle
	so        map[string]*canvas.Text
	C         *fyne.Container
}

func moiSoDoXe(loaiXe string, k float32, nhan bool) *soDoXe {
	s := &soDoXe{loaiXe: loaiXe, k: k, nhan: nhan, viTri: map[string][4]float32{}, o: map[string]*canvas.Rectangle{},
		vach: map[string][]*canvas.Rectangle{}, so: map[string]*canvas.Text{}, C: container.NewWithoutLayout()}
	them := func(ob fyne.CanvasObject, x, y, w, h float32) {
		ob.Move(fyne.NewPos(x, y))
		ob.Resize(fyne.NewSize(w, h))
		s.C.Add(ob)
	}
	type tang struct {
		x0, x1 float32
		ten    string
	}
	var ds []tang
	var w, h, khe float32
	if loaiXe == "giuong" {
		w, h, khe = 46*k, 30*k, 6*k
		rongTang := 3*w + 2*khe
		ds = []tang{{12 * k, 12*k + rongTang, "TẦNG DƯỚI"}, {12*k + rongTang + 26*k, 12*k + 2*rongTang + 26*k, "TẦNG TRÊN"}}
		s.rong = 24*k + 2*rongTang + 26*k
		s.cao = 40*k + 7*(h+khe) + 4*k
	} else {
		w, h, khe = 34*k, 26*k, 5*k
		ds = []tang{{12 * k, 12*k + 6*(w+khe) - khe, ""}}
		s.rong = 24*k + 6*(w+khe) - khe
		s.cao = 40*k + 12*(h+khe) + 4*k
	}
	for _, t := range ds { // thân xe mỗi tầng
		r := canvas.NewRectangle(mau("#f7faf8"))
		r.StrokeColor, r.StrokeWidth = mauXe, max(1, 2*k)
		them(r, t.x0-6*k, 8*k, t.x1-t.x0+12*k, s.cao-8*k)
		if nhan && t.ten != "" {
			c := chu(t.ten, mauXe, 10, true)
			them(c, (t.x0+t.x1)/2-c.MinSize().Width/2, 12*k, c.MinSize().Width, c.MinSize().Height)
		}
	}
	if nhan || k >= 0.5 { // vô lăng ở đầu xe (bên trái, tầng dưới)
		r := 9 * k
		cx, cy := ds[0].x0+r+2*k, 22*k
		vl := canvas.NewCircle(color.Transparent)
		vl.StrokeColor, vl.StrokeWidth = mChuToi, max(1, 2*k)
		them(vl, cx-r, cy-r, 2*r, 2*r)
		l := canvas.NewLine(mChuToi)
		l.StrokeWidth = max(1, 2*k)
		l.Position1, l.Position2 = fyne.NewPos(cx-r, cy), fyne.NewPos(cx+r, cy)
		s.C.Add(l)
	}
	for _, x := range tc.BoTri(loaiXe) {
		var px float32
		if loaiXe == "giuong" {
			px = ds[x.Tang].x0 + float32(x.Cot)*(w+khe)
		} else {
			px = ds[0].x0 + float32(x.Cot)*(w+khe)
		}
		py := 40*k + float32(x.Hang)*(h+khe)
		s.viTri[x.Ma] = [4]float32{px, py, px + w, py + h}
		o := canvas.NewRectangle(mTrong)
		o.StrokeColor, o.StrokeWidth = mVien, 1
		s.o[x.Ma] = o
		them(o, px, py, w, h)
		cao := max(2, 5*k)
		for i := 0; i < tc.SoChang; i++ { // dải vạch: mỗi chặng một ô ở chân chỗ
			a := px + 2 + (w-4)*float32(i)/float32(tc.SoChang)
			b := px + 2 + (w-4)*float32(i+1)/float32(tc.SoChang) - 1
			v := canvas.NewRectangle(mauVach['.'])
			s.vach[x.Ma] = append(s.vach[x.Ma], v)
			them(v, a, py+h-cao-1, b-a, cao)
		}
		if nhan {
			t := chu(x.Ma, mChuToi, 9, false)
			t.Alignment = fyne.TextAlignCenter
			s.so[x.Ma] = t
			them(t, px, py+1, w, h-8)
		}
	}
	return s
}

// ve – tô mọi chỗ theo đoạn [tu, den) và vẽ dải vạch theo chặng; vien: {chỗ: màu viền} (ghế nóng ở máy chủ).
func (s *soDoXe) ve(tt tc.M, ten string, tu, den int, chon map[string]bool, vien map[string]color.NRGBA) {
	for g, o := range s.o {
		trang, vach := "trong", strings.Repeat(".", tc.SoChang)
		if tt != nil && slices.Contains(tc.DSChuoi(tt["ghe"]), g) {
			trang, vach = trangCho(tt, g, ten, tu, den), vachCua(tt, g, ten)
		}
		m := map[string]color.NRGBA{"trong": mTrong, "giu": mKhacGiu, "ban": mDaBan, "toi_giu": mToiGiu, "ve": mVeToi}[trang]
		if trang == "trong" && chon[g] {
			m = mChon
		}
		o.FillColor, o.StrokeColor, o.StrokeWidth = m, mVien, 1
		if v, co := vien[g]; co {
			o.StrokeColor, o.StrokeWidth = v, 2
		}
		o.Refresh()
		for i, v := range s.vach[g] {
			v.FillColor = mauVach[vach[i]]
			v.StrokeColor, v.StrokeWidth = color.Transparent, 0
			if tu <= i && i < den {
				v.StrokeColor, v.StrokeWidth = mChuToi, 1
			}
			v.Refresh()
		}
		if t := s.so[g]; t != nil {
			mc := mChuToi
			if m == mToiGiu || m == mVeToi || m == mDaBan {
				mc = Trang
			}
			datChu(t, t.Text, mc)
		}
	}
}

func (s *soDoXe) choTai(p fyne.Position) string {
	for g, v := range s.viTri {
		if v[0] <= p.X && p.X <= v[2] && v[1] <= p.Y && p.Y <= v[3] {
			return g
		}
	}
	return ""
}

// ---------------------------------------------------------------- cửa sổ đặt vé

type veXe struct {
	ghe, ma string
	tu, den int
}

type cuaSoDatVeXe struct {
	*cuaSoNguoiDungCoSo
	chon, giu         map[string]bool // chỗ đang chọn / mình đang giữ (theo kq) cho đoạn doanGiu
	doanGiu           [2]int
	veCua             []veXe // vé của mình
	phienGiu, phienVe int64
	han               *time.Time // thời điểm hết hạn giữ chỗ
	soGiu, soVe       int
	tu, den           int
	bangRonCu         string
	soDo              *soDoXe
	khungXe           *fyne.Container
	tenNhaXe, dongXe  *canvas.Text
	dongGiu           *canvas.Text
	chonTu, chonDen   *widget.Select
	anhTuyen          *canvas.Image
	giaTuyen          []*canvas.Text
	nhanGhe           *canvas.Text
	nhanTien          *canvas.Text
	nutGiu, nutTT     *widget.Button
	nutNha            *widget.Button
	nhanDongHo        *canvas.Text
	thongBao          *oMau
	nhanVe            *widget.Label
}

func moCuaSoDatVeXe(a fyne.App, ten, host string, cong int) *cuaSoDatVeXe {
	x := &cuaSoDatVeXe{chon: map[string]bool{}, giu: map[string]bool{}, den: tc.SoChang}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Đặt vé xe khách", ten, host, cong, x)
	x.hoanTat(700)
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

func (x *cuaSoDatVeXe) ve() fyne.CanvasObject {
	x.khungXe = container.NewStack()
	trai := coDinh(container.NewStack(canvas.NewRectangle(nenXe), x.khungXe, moiVungBam(func(p fyne.Position) {
		if x.soDo != nil {
			if g := x.soDo.choTai(p); g != "" {
				x.bam(g)
			}
		}
	}, nil)), 376, 480)

	nen := canvas.NewRectangle(mauXe)
	day := canvas.NewRectangle(mau("#0f7a4f"))
	x.tenNhaXe = chu("", Trang, 14, true)
	x.dongXe = chu("", mau("#d7f0e2"), 10, false)
	x.dongGiu = chu("", mau("#ffe082"), 10, true)
	bangRon := container.NewWithoutLayout(nen, day, x.tenNhaXe, x.dongXe, x.dongGiu)
	nen.Resize(fyne.NewSize(256, 62))
	day.Move(fyne.NewPos(0, 44))
	day.Resize(fyne.NewSize(256, 18))
	x.tenNhaXe.Move(fyne.NewPos(10, 4))
	x.dongXe.Move(fyne.NewPos(10, 26))
	x.dongGiu.Move(fyne.NewPos(10, 46))

	x.chonTu = widget.NewSelect(tc.Tram[:len(tc.Tram)-1], func(string) { x.doiDoan() })
	x.chonDen = widget.NewSelect(tc.Tram[1:], func(string) { x.doiDoan() })
	x.chonTu.SetSelected(tc.Tram[0])
	x.chonDen.SetSelected(tc.Tram[len(tc.Tram)-1])
	x.anhTuyen = oAnhTinh(x.veAnhTuyen(), 256, 40)
	tuyen := container.NewWithoutLayout(x.anhTuyen)
	for i := 0; i < len(tc.Tram); i++ {
		t := chu(tc.MaTram[i], mChuToi, 10, true)
		t.Move(fyne.NewPos(16+float32(i)*224/float32(len(tc.Tram)-1)-8, 22))
		tuyen.Add(t)
		if i+1 < len(tc.Tram) {
			g := chu("", mVien, 9, false)
			g.Move(fyne.NewPos(16+(float32(i)+0.5)*224/float32(len(tc.Tram)-1)-10, 23))
			x.giaTuyen = append(x.giaTuyen, g)
			tuyen.Add(g)
		}
	}
	x.nhanGhe = chu("Chỗ đã chọn: –", Den, 13, true)
	x.nhanTien = chu("", Den, 11, false)
	x.nutGiu = nut("GIỮ CHỖ", widget.WarningImportance, x.giuCho)
	x.nhanDongHo = chu("--:--", mau("#909090"), 22, true)
	x.nhanDongHo.TextStyle.Monospace = true
	x.nutTT = nut("THANH TOÁN", widget.SuccessImportance, x.thanhToan)
	x.nutNha = nut("Nhả chỗ", widget.MediumImportance, x.nha)
	x.thongBao = moiOMau("Chưa kết nối", mau("#e8e8e8"), Den, 12, 256, 30, true)
	x.nhanVe = widget.NewLabel("Vé của bạn: –")
	x.nhanVe.Wrapping = fyne.TextWrapWord
	phai := container.NewStack(canvas.NewRectangle(Trang), container.NewPadded(container.NewVBox(
		coDinh(bangRon, 256, 62),
		container.NewHBox(coDinh(widget.NewLabel("Đi từ"), 44, 30), coDinh(x.chonTu, 140, 34)),
		container.NewHBox(coDinh(widget.NewLabel("Đến"), 44, 30), coDinh(x.chonDen, 140, 34)),
		coDinh(tuyen, 256, 40), x.nhanGhe, x.nhanTien, coDinh(x.nutGiu, 256, 36),
		container.NewHBox(container.NewCenter(widget.NewLabel("Giữ chỗ còn")), x.nhanDongHo),
		coDinh(x.nutTT, 256, 36), container.NewHBox(x.nutNha, layout.NewSpacer()), x.thongBao.O,
		coDinh(x.nhanVe, 256, 56))))
	x.capNhatGio()
	return container.NewHBox(trai, coDinh(phai, 272, 480))
}

// ------------------------------------------------------------ băng rôn, tuyến, sơ đồ

func (x *cuaSoDatVeXe) veBangRon(tt tc.M) {
	datChu(x.tenNhaXe, strings.ToUpper(tc.Chuoi(tt["nha_xe"])), Trang)
	datChu(x.dongXe, fmt.Sprintf("Chuyến %s · %s", tc.Chuoi(tt["chuyen"]), tc.Chuoi(tt["ten_xe"])), x.dongXe.Color)
	datChu(x.dongGiu, fmt.Sprintf("Giữ chỗ %s s · giá theo chặng", tc.FmtG(tc.SoThuc(tt["ttl_s"]))), x.dongGiu.Color)
	gia, _ := tt["gia_chang"].([]any)
	for i, t := range x.giaTuyen {
		if i < len(gia) {
			datChu(t, fmt.Sprintf("%dk", tc.SoNguyen(gia[i])/1000), mVien)
		}
	}
}

func (x *cuaSoDatVeXe) veAnhTuyen() image.Image {
	b := moiBut(256, 40)
	n := len(tc.Tram)
	px := func(i int) float64 { return 16 + float64(i)*224/float64(n-1) }
	b.duong(px(0), 12, px(n-1), 12, 6, mau("#cfd8dc"))
	b.duong(px(x.tu), 12, px(x.den), 12, 6, mauXe)
	for i := 0; i < n; i++ {
		nen := Trang
		if x.tu <= i && i <= x.den {
			nen = mauXe
		}
		b.elip(px(i)-6, 6, px(i)+6, 18, pm(nen), pm(mauXe), 2, false)
	}
	return b.anh()
}

func (x *cuaSoDatVeXe) veCho() {
	tt := x.tt
	if tt == nil {
		return
	}
	if loai := tc.Chuoi(tt["loai_xe"]); x.soDo == nil || x.soDo.loaiXe != loai {
		x.soDo = moiSoDoXe(loai, 1, true)
		cg := container.NewWithoutLayout()
		px, py := float32(8), x.soDo.cao+10
		for i, c := range chuGiai { // chú giải màu, 2 hàng
			if i == 3 {
				px, py = 8, py+18
			}
			o := canvas.NewRectangle(c.m)
			o.StrokeColor, o.StrokeWidth = mVien, 1
			o.Move(fyne.NewPos(px, py))
			o.Resize(fyne.NewSize(14, 11))
			t := chu(c.s, mChuToi, 10, false)
			t.Move(fyne.NewPos(px+18, py-2))
			cg.Add(o)
			cg.Add(t)
			px += 30 + t.MinSize().Width
		}
		var chang []string
		for k := 0; k < tc.SoChang; k++ {
			chang = append(chang, tc.MaTram[k]+"-"+tc.MaTram[k+1])
		}
		t := chu("Vạch ở chân chỗ: từng chặng "+strings.Join(chang, " | "), mau("#455a64"), 9, false)
		t.Move(fyne.NewPos(8, py+18))
		cg.Add(t)
		x.khungXe.Objects = []fyne.CanvasObject{x.soDo.C, cg}
		x.khungXe.Refresh()
	}
	x.soDo.ve(tt, x.ten, x.tu, x.den, x.chon, nil)
}

// ------------------------------------------------------------ chọn đoạn, chọn chỗ

// datDoan – chọn đoạn đường (người dùng hoặc kịch bản demo): chỉ số trạm.
func (x *cuaSoDatVeXe) datDoan(tu, den int) {
	x.chonTu.SetSelected(tc.Tram[tu])
	x.chonDen.SetSelected(tc.Tram[den])
	x.doiDoan()
}

func (x *cuaSoDatVeXe) doiDoan() {
	if x.chonTu == nil || x.chonDen == nil || x.nutGiu == nil {
		return // đang dựng cửa sổ
	}
	tu, den := slices.Index(tc.Tram, x.chonTu.Selected), slices.Index(tc.Tram, x.chonDen.Selected)
	if tu < 0 || den < 0 {
		return
	}
	if den <= tu { // "Đến" phải sau "Đi từ"
		den = tu + 1
		x.chonDen.SetSelected(tc.Tram[den])
	}
	x.tu, x.den = tu, den
	if x.tt != nil {
		var bo []string
		for _, g := range sapXep(x.chon) {
			if trangCho(x.tt, g, x.ten, tu, den) != "trong" {
				bo = append(bo, g)
				delete(x.chon, g)
			}
		}
		if len(bo) > 0 {
			x.bao(fmt.Sprintf("Chỗ %s không trống cả đoạn %s", strings.Join(bo, ", "), tc.TenDoan(tu, den)),
				mau("#ffe0b2"), Den)
		}
	}
	x.anhTuyen.Image = x.veAnhTuyen()
	x.anhTuyen.Refresh()
	x.capNhatGio()
}

// bam – bấm một chỗ (người dùng hoặc kịch bản demo).
func (x *cuaSoDatVeXe) bam(g string) {
	if x.kn == nil || x.tt == nil {
		return
	}
	trang := trangCho(x.tt, g, x.ten, x.tu, x.den)
	switch {
	case x.chon[g]:
		delete(x.chon, g)
	case trang == "trong" && !x.giu[g]:
		if len(x.chon)+len(x.giu) >= tc.ToiDaGhe {
			x.bao(fmt.Sprintf("Tối đa %d chỗ mỗi lần", tc.ToiDaGhe), mau("#ffe0b2"), Den)
			return
		}
		x.chon[g] = true
	case trang == "giu":
		x.bao("Chỗ "+g+" đang có người giữ trên đoạn này", mau("#ffe0b2"), Den)
	case trang == "ban":
		x.bao("Chỗ "+g+" đã bán trên đoạn này", mau("#e0e0e0"), Den)
	}
	x.capNhatGio()
}

func (x *cuaSoDatVeXe) gia(tu, den, soCho int) int64 {
	if x.tt == nil {
		return 0
	}
	gia, _ := x.tt["gia_chang"].([]any)
	var t int64
	for k := tu; k < den && k < len(gia); k++ {
		t += tc.SoNguyen(gia[k])
	}
	return t * int64(soCho)
}

func (x *cuaSoDatVeXe) capNhatGio() {
	if len(x.giu) > 0 {
		tu, den := x.doanGiu[0], x.doanGiu[1]
		g := x.gia(tu, den, len(x.giu))
		datChu(x.nhanGhe, "Đang giữ: "+strings.Join(sapXep(x.giu), ", "), Den)
		datChu(x.nhanTien, tc.TenDoan(tu, den)+" · tổng "+tc.Tien(g), Den)
		x.nutTT.SetText("THANH TOÁN " + tc.Tien(g))
	} else {
		ds := strings.Join(sapXep(x.chon), ", ")
		if ds == "" {
			ds = "–"
		}
		datChu(x.nhanGhe, "Chỗ đã chọn: "+ds, Den)
		datChu(x.nhanTien, tc.TenDoan(x.tu, x.den)+" · tạm tính "+tc.Tien(x.gia(x.tu, x.den, len(x.chon))), Den)
		x.nutTT.SetText("THANH TOÁN")
	}
	x.batNut()
	x.veCho()
}

func (x *cuaSoDatVeXe) batNut() {
	co := x.kn != nil
	conHan := x.han != nil && time.Now().Before(*x.han)
	batTat(co && len(x.chon) > 0 && len(x.giu) == 0, x.nutGiu)
	batTat(co && len(x.giu) > 0 && conHan, x.nutTT)
	batTat(co && len(x.giu) > 0, x.nutNha)
	batTat(len(x.giu) == 0, x.chonTu, x.chonDen) // đang giữ chỗ: không đổi đoạn
}

func (x *cuaSoDatVeXe) bao(text string, nen, mauChu color.Color) { x.thongBao.dat(text, nen, mauChu) }

// ------------------------------------------------------------ giữ / thanh toán / nhả

func (x *cuaSoDatVeXe) truong(ghe []string, tu, den int) tc.TS {
	return tc.T("ghe", ghe, "tu", int64(tu), "den", int64(den))
}

func (x *cuaSoDatVeXe) giuCho() {
	if len(x.chon) == 0 || len(x.giu) > 0 {
		return
	}
	ghe := sapXep(x.chon)
	x.log.them(fmt.Sprintf("Giữ chỗ %s %s…", strings.Join(ghe, ", "), tc.TenDoan(x.tu, x.den)))
	x.bao("Đang giữ chỗ…", VangNhat, Den)
	x.nutGiu.Disable()
	x.guiYC("giu_cho", x.truong(ghe, x.tu, x.den))
}

func (x *cuaSoDatVeXe) thanhToan() {
	if len(x.giu) > 0 {
		x.log.them(fmt.Sprintf("Thanh toán %s…", strings.Join(sapXep(x.giu), ", ")))
		x.guiYC("thanh_toan", x.truong(sapXep(x.giu), x.doanGiu[0], x.doanGiu[1]))
	}
}

func (x *cuaSoDatVeXe) nha() {
	if len(x.giu) > 0 {
		x.guiYC("nha", x.truong(sapXep(x.giu), x.doanGiu[0], x.doanGiu[1]))
	}
}

func (x *cuaSoDatVeXe) ketQua(loai string, truong tc.TS, tin tc.M, rtt float64) {
	gv, _ := truong.Lay("ghe")
	ghe := gv.([]string)
	tv, _ := truong.Lay("tu")
	dv, _ := truong.Lay("den")
	tu, den := int(tc.SoNguyen(tv)), int(tc.SoNguyen(dv))
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
			x.doanGiu = [2]int{tu, den}
			x.phienGiu = tc.SoNguyen(tin["phien"])
			h := time.Now().Add(time.Duration(tc.SoThuc(tin["het_han_ms"]) * float64(time.Millisecond)))
			x.han = &h
			x.bao(fmt.Sprintf("Đã giữ %s – thanh toán trong %.0f s", strings.Join(ghe, ", "), tc.SoThuc(tin["het_han_ms"])/1000),
				mau("#c8f0d2"), XanhDam)
			x.log.them(fmt.Sprintf("  ĐÃ GIỮ %s (%.1f ms)", strings.Join(ghe, ", "), rtt))
			x.kiemCho(x.tt)
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
			ma := tc.DSChuoi(tin["ma_ve"])
			for i, g := range ghe {
				v := veXe{ghe: g, tu: tu, den: den}
				if i < len(ma) {
					v.ma = ma[i]
				}
				x.veCua = append(x.veCua, v)
			}
			x.phienVe = tc.SoNguyen(tin["phien"])
			x.giu, x.han = map[string]bool{}, nil
			x.bao("Đặt vé thành công! "+tc.Tien(tc.SoNguyen(tin["tien"])), mau("#c8f0d2"), XanhDam)
			x.log.them(fmt.Sprintf("  ĐẶT VÉ THÀNH CÔNG: %s (%.1f ms)", strings.Join(ma, ", "), rtt))
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

func (x *cuaSoDatVeXe) veLaiVe() {
	var ds []string
	for _, v := range x.veCua {
		ds = append(ds, fmt.Sprintf("%s %s→%s", v.ghe, tc.MaTram[v.tu], tc.MaTram[v.den]))
	}
	s := strings.Join(ds, ", ")
	if s == "" {
		s = "–"
	}
	x.nhanVe.SetText(fmt.Sprintf("Vé của bạn: %s\nBạn: giữ được %d lần, mua %d vé", s, x.soGiu, x.soVe))
}

// kiemCho – chỗ mình giữ/mua bị người khác lấy (tranh chấp) hoặc hết hạn; chỗ đang chọn bị người khác giữ.
// Chỉ tin cap_nhat chụp SAU lần ghi của mình (tt["phien"] ≥ phien của kq giữ chỗ / thanh toán).
func (x *cuaSoDatVeXe) kiemCho(tt tc.M) {
	if tt == nil {
		return
	}
	phien := tc.SoNguyen(tt["phien"])
	if phien >= x.phienGiu && len(x.giu) > 0 {
		tu, den := x.doanGiu[0], x.doanGiu[1]
		for _, g := range sapXep(x.giu) {
			tatCa, ai := true, ""
			for k := tu; k < den; k++ {
				a := chuCua(tt, fmt.Sprintf("%s#%d", g, k))
				if a != x.ten {
					tatCa = false
					if a != "" && ai == "" {
						ai = a
					}
				}
			}
			if tatCa {
				continue
			}
			delete(x.giu, g)
			if ai != "" {
				x.bao(fmt.Sprintf("Chỗ %s đã bị %s lấy mất!", g, ai), DoSP, Trang)
				x.log.them(fmt.Sprintf("Chỗ %s đã bị %s lấy mất!", g, ai))
			} else {
				x.bao("Hết thời gian giữ chỗ – chỗ đã được nhả", mau("#ffd0d0"), DoDam)
				x.log.them("Hết thời gian giữ chỗ: " + g)
			}
		}
		if len(x.giu) == 0 {
			x.han = nil
		}
	}
	if phien >= x.phienVe {
		var con []veXe
		for _, v := range x.veCua {
			ai := x.ten
			for k := v.tu; k < v.den; k++ {
				if a := chuCua(tt, fmt.Sprintf("%s#%d", v.ghe, k)); a != x.ten {
					ai = a
					break
				}
			}
			if ai == x.ten {
				con = append(con, v)
				continue
			}
			if ai == "" {
				ai = "không ai"
			}
			x.bao(fmt.Sprintf("Vé %s bị ghi cho %s!", v.ghe, ai), DoSP, Trang)
			x.log.them(fmt.Sprintf("Vé %s bị ghi cho %s!", v.ghe, ai))
		}
		x.veCua = con
	}
	var biLay []string
	for _, g := range sapXep(x.chon) {
		if trangCho(tt, g, x.ten, x.tu, x.den) != "trong" {
			biLay = append(biLay, g)
			delete(x.chon, g)
		}
	}
	if len(biLay) > 0 {
		x.bao(fmt.Sprintf("Chỗ %s vừa có người giữ", strings.Join(biLay, ", ")), mau("#ffe0b2"), Den)
		x.log.them(fmt.Sprintf("Chỗ %s vừa có người giữ", strings.Join(biLay, ", ")))
	}
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoDatVeXe) capNhat(tt tc.M) {
	if k := fmt.Sprint(tt["nha_xe"], tt["chuyen"], tt["ten_xe"], tt["ttl_s"]); k != x.bangRonCu {
		x.veBangRon(tt)
		x.bangRonCu = k
	}
	if x.thongBao.text() == "Chưa kết nối" {
		x.bao("Chọn đoạn đường, chọn chỗ rồi bấm GIỮ CHỖ", mau("#e8e8e8"), Den)
	}
	dem, _ := tt["dem"].(tc.M)
	chu, _ := tt["chu"].(tc.M)
	if len(chu) == 0 && (len(x.veCua) > 0 || len(x.giu) > 0) && tc.SoNguyen(dem["ban"]) == 0 && tc.SoNguyen(dem["giu"]) == 0 {
		x.veCua = nil // máy chủ đặt lại: xe trống
	}
	ghe := tc.DSChuoi(tt["ghe"])
	for g := range x.chon { // đổi loại xe: bỏ chỗ không còn
		if !slices.Contains(ghe, g) {
			delete(x.chon, g)
		}
	}
	if len(x.giu) > 0 {
		conLai, _ := tt["con_lai_ms"].(tc.M)
		minMs := math.Inf(1)
		for g := range x.giu {
			for k := x.doanGiu[0]; k < x.doanGiu[1]; k++ {
				o := fmt.Sprintf("%s#%d", g, k)
				if v, co := conLai[o]; co && chuCua(tt, o) == x.ten {
					minMs = math.Min(minMs, tc.SoThuc(v))
				}
			}
		}
		if !math.IsInf(minMs, 1) {
			h := time.Now().Add(time.Duration(minMs * float64(time.Millisecond)))
			x.han = &h
		}
	}
	x.kiemCho(tt)
	x.veLaiVe()
	x.capNhatGio()
}

func (x *cuaSoDatVeXe) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoDatVeXe) moiNhip() {
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

func (x *cuaSoDatVeXe) dongDinhKy() string {
	d, _ := x.tt["dem"].(tc.M)
	con := "–"
	if x.han != nil {
		con = fmt.Sprintf("%.0f s", math.Max(0, time.Until(*x.han).Seconds()))
	}
	giu := strings.Join(sapXep(x.giu), ", ")
	if giu == "" {
		giu = "–"
	}
	return fmt.Sprintf("chỗ-chặng trống %d, giữ %d, bán %d; bạn giữ %s (còn %s), vé %d", tc.SoNguyen(d["trong"]),
		tc.SoNguyen(d["giu"]), tc.SoNguyen(d["ban"]), giu, con, len(x.veCua))
}

func (x *cuaSoDatVeXe) daKetNoi() { x.batNut() }

func (x *cuaSoDatVeXe) matKetNoi() { x.batNut() }
