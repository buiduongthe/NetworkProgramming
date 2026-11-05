package main

// Vòng quay may mắn (ứng dụng người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu)
// ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ vòng quay: bánh xe chia ô (mỗi món quà một ô màu, xen
// giữa là ô "May mắn lần sau"; chữ xoay theo ô), kim chỉ ở đỉnh, đèn viền nhấp nháy khi quay, nút QUAY.
// Bấm QUAY: bánh xe quay tự do ngay, yêu cầu {"loai":"quay"} đi tới máy chủ; khi có kết quả, bánh xe giảm
// tốc (ease-out) và dừng đúng ô kết quả (món trúng, hoặc một ô "may mắn lần sau"). Bên phải: số quà còn
// lại, ảnh từng món (vẽ bằng bút ảnh) với trạng thái còn / đã trao cho ai, danh sách người trúng trực
// tiếp (từ cap_nhat ≤ 10 lần/giây).
//
// Fyne không có cung tròn và chữ xoay, nên bánh xe là ẢNH: đĩa các ô (kèm chữ xoay, vẽ bằng phông chữ của
// chủ đề qua golang.org/x/image/font/opentype) được dựng lại khi trạng thái quà đổi; mỗi khung hình
// (30 ms) chỉ xoay ảnh đĩa theo góc hiện tại (nội suy song tuyến). Vành, kim, trục là ảnh tĩnh; đèn viền
// là 24 canvas.Circle.

import (
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"math"
	"math/rand/v2"
	"strings"
	"sync"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"golang.org/x/image/font"
	"golang.org/x/image/font/opentype"
	"golang.org/x/image/math/fixed"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.9_QuaySo/tranhchap"
)

var mauQua = []string{"#e6194b", "#f58231", "#ffe119", "#3cb44b", "#42d4f4", "#4363d8", "#911eb4", "#f032e6",
	"#9a6324", "#469990"}

var tenNgan = []string{"Xe đạp", "Điện thoại", "Laptop", "Tai nghe", "Đồng hồ", "Máy ảnh", "Ba lô", "Nồi cơm",
	"Loa", "500k"}

// ===================================================================== chữ trên ảnh (phông của chủ đề)

var (
	muPhong sync.Mutex
	phong   = map[bool]*opentype.Font{}
)

func matChu(px float64, dam bool) font.Face {
	muPhong.Lock()
	defer muPhong.Unlock()
	f := phong[dam]
	if f == nil {
		var err error
		f, err = opentype.Parse(theme.DefaultTheme().Font(fyne.TextStyle{Bold: dam}).Content())
		if err != nil {
			return nil
		}
		phong[dam] = f
	}
	mat, err := opentype.NewFace(f, &opentype.FaceOptions{Size: px, DPI: 72, Hinting: font.HintingNone})
	if err != nil {
		return nil
	}
	return mat
}

// anhChu – ảnh trong suốt chứa chữ (nhiều dòng, căn giữa) cỡ px điểm ảnh.
func anhChu(s string, px float64, m color.NRGBA, dam bool) *image.NRGBA {
	mat := matChu(px, dam)
	if mat == nil {
		return image.NewNRGBA(image.Rect(0, 0, 1, 1))
	}
	defer mat.Close()
	dong := strings.Split(s, "\n")
	met := mat.Metrics()
	cao := (met.Ascent + met.Descent).Ceil()
	rong := 0
	for _, d := range dong {
		rong = max(rong, font.MeasureString(mat, d).Ceil())
	}
	a := image.NewNRGBA(image.Rect(0, 0, rong+2, cao*len(dong)+2))
	ve := &font.Drawer{Dst: a, Src: image.NewUniform(m), Face: mat}
	for i, d := range dong {
		w := font.MeasureString(mat, d).Ceil()
		ve.Dot = fixed.P((rong-w)/2+1, i*cao+met.Ascent.Ceil()+1)
		ve.DrawString(d)
	}
	return a
}

// layMau – lấy mẫu song tuyến (alpha nhân trước) tại tọa độ thực (x, y) của ảnh src.
func layMau(src *image.NRGBA, x, y float64) (r, g, b, a float64) {
	x, y = x-0.5, y-0.5
	x0, y0 := int(math.Floor(x)), int(math.Floor(y))
	fx, fy := x-float64(x0), y-float64(y0)
	bd := src.Bounds()
	for j := 0; j < 2; j++ {
		for i := 0; i < 2; i++ {
			px, py := x0+i, y0+j
			if px < bd.Min.X || py < bd.Min.Y || px >= bd.Max.X || py >= bd.Max.Y {
				continue
			}
			w := (1 - math.Abs(float64(i)-fx)) * (1 - math.Abs(float64(j)-fy))
			c := src.NRGBAAt(px, py)
			al := float64(c.A) / 255 * w
			r, g, b, a = r+float64(c.R)*al, g+float64(c.G)*al, b+float64(c.B)*al, a+al
		}
	}
	return
}

// danXoay – dán ảnh src (tâm ảnh) vào dst tại (cx, cy), xoay góc goc độ NGƯỢC chiều kim đồng hồ (như
// angle của Tk), phủ lên trên (alpha).
func danXoay(dst, src *image.NRGBA, cx, cy, goc float64) {
	sw, sh := float64(src.Bounds().Dx()), float64(src.Bounds().Dy())
	ban := math.Hypot(sw, sh)/2 + 1
	c, s := math.Cos(goc*math.Pi/180), math.Sin(goc*math.Pi/180)
	bd := dst.Bounds()
	for y := int(cy - ban); y <= int(cy+ban); y++ {
		for x := int(cx - ban); x <= int(cx+ban); x++ {
			if x < bd.Min.X || y < bd.Min.Y || x >= bd.Max.X || y >= bd.Max.Y {
				continue
			}
			dx, dy := float64(x)+0.5-cx, float64(y)+0.5-cy
			u, v := dx*c-dy*s, dx*s+dy*c
			r, g, b, a := layMau(src, u+sw/2, v+sh/2)
			if a <= 0.002 {
				continue
			}
			d := dst.NRGBAAt(x, y)
			da := float64(d.A) / 255
			oa := a + da*(1-a)
			tron := func(sc, dc float64) uint8 {
				return uint8(math.Min(255, (sc+dc*da*(1-a))/oa+0.5))
			}
			dst.SetNRGBA(x, y, color.NRGBA{tron(r, float64(d.R)), tron(g, float64(d.G)), tron(b, float64(d.B)),
				uint8(math.Min(255, oa*255+0.5))})
		}
	}
}

// ===================================================================== ảnh quà (vẽ đơn giản)

// hcn – hình chữ nhật đặc có viền 1 điểm ảnh.
func (b *but) hcn(x0, y0, x1, y1 float64, nen *color.NRGBA, vien *color.NRGBA) {
	if nen != nil {
		b.chuNhat(x0, y0, x1, y1, *nen)
	}
	if vien != nil {
		b.gapKhuc([]diem{{x0, y0}, {x1, y0}, {x1, y1}, {x0, y1}, {x0, y0}}, 1, *vien, false)
	}
}

// veQua – món quà i quanh tâm (cx, cy), nửa cạnh s; xam = đã trao (vẽ màu xám). Trả về chữ cần dán thêm
// (phiếu 500K) – nil nếu không có.
func veQua(b *but, i int, cx, cy, s float64, xam bool) {
	m, den := mau(mauQua[i]), mau("#303030")
	if xam {
		m, den = mau("#9a9a9a"), mau("#7a7a7a")
	}
	switch i {
	case 0: // xe đạp
		for _, dx := range []float64{-0.55, 0.55} {
			b.elip(cx+(dx-0.4)*s, cy-0.1*s, cx+(dx+0.4)*s, cy+0.7*s, nil, &den, 2, false)
		}
		b.gapKhuc([]diem{{cx - 0.55*s, cy + 0.3*s}, {cx - 0.1*s, cy - 0.35*s}, {cx + 0.4*s, cy - 0.35*s},
			{cx + 0.55*s, cy + 0.3*s}}, 3, m, false)
		b.gapKhuc([]diem{{cx - 0.1*s, cy - 0.35*s}, {cx, cy + 0.3*s}, {cx + 0.4*s, cy - 0.35*s}}, 3, m, false)
		b.duong(cx-0.25*s, cy-0.5*s, cx+0.05*s, cy-0.5*s, 3, den)
		b.duong(cx+0.35*s, cy-0.6*s, cx+0.55*s, cy-0.6*s, 3, den)
	case 1: // điện thoại
		b.hcn(cx-0.42*s, cy-0.85*s, cx+0.42*s, cy+0.85*s, &den, &den)
		b.chuNhat(cx-0.34*s, cy-0.68*s, cx+0.34*s, cy+0.6*s, m)
		b.elip(cx-0.07*s, cy+0.66*s, cx+0.07*s, cy+0.8*s, pm(mau("#d0d0d0")), nil, 0, false)
	case 2: // laptop
		b.hcn(cx-0.7*s, cy-0.65*s, cx+0.7*s, cy+0.3*s, &den, &den)
		b.chuNhat(cx-0.6*s, cy-0.55*s, cx+0.6*s, cy+0.2*s, m)
		de := []diem{{cx - 0.7*s, cy + 0.35*s}, {cx + 0.7*s, cy + 0.35*s}, {cx + 0.9*s, cy + 0.6*s}, {cx - 0.9*s, cy + 0.6*s}}
		b.daGiac(de, mau("#b0b0b0"))
		b.gapKhuc(append(de, de[0]), 1, den, false)
	case 3: // tai nghe
		b.elip(cx-0.65*s, cy-0.75*s, cx+0.65*s, cy+0.55*s, nil, &den, 3, true)
		for _, dx := range []float64{-0.65, 0.65} {
			b.hcn(cx+(dx-0.18)*s, cy-0.1*s, cx+(dx+0.18)*s, cy+0.6*s, &m, &den)
		}
	case 4: // đồng hồ
		b.chuNhat(cx-0.25*s, cy-0.9*s, cx+0.25*s, cy+0.9*s, mau("#6b4226"))
		b.elip(cx-0.55*s, cy-0.55*s, cx+0.55*s, cy+0.55*s, pm(Trang), &m, 3, false)
		b.duong(cx, cy, cx, cy-0.38*s, 2, den)
		b.duong(cx, cy, cx+0.28*s, cy+0.1*s, 2, den)
	case 5: // máy ảnh
		b.chuNhat(cx-0.3*s, cy-0.65*s, cx+0.15*s, cy-0.4*s, den)
		b.hcn(cx-0.85*s, cy-0.45*s, cx+0.85*s, cy+0.6*s, &m, &den)
		b.elip(cx-0.42*s, cy-0.33*s, cx+0.42*s, cy+0.5*s, &den, pm(Trang), 1, false)
		b.elip(cx-0.2*s, cy-0.12*s, cx+0.2*s, cy+0.28*s, pm(mau("#5080c0")), nil, 0, false)
	case 6: // ba lô
		b.elip(cx-0.4*s, cy-0.95*s, cx+0.4*s, cy-0.45*s, nil, &den, 2, true)
		b.hcn(cx-0.6*s, cy-0.7*s, cx+0.6*s, cy+0.85*s, &m, &den)
		b.chuNhat(cx-0.4*s, cy+0.15*s, cx+0.4*s, cy+0.65*s, den)
	case 7: // nồi cơm điện
		b.elip(cx-0.8*s, cy-0.55*s, cx+0.8*s, cy-0.05*s, pm(mau("#e0e0e0")), &den, 1, false)
		b.hcn(cx-0.8*s, cy-0.3*s, cx+0.8*s, cy+0.7*s, &m, &den)
		b.chuNhat(cx-0.15*s, cy-0.75*s, cx+0.15*s, cy-0.5*s, den)
		b.chuNhat(cx-0.25*s, cy+0.1*s, cx+0.25*s, cy+0.4*s, Trang)
	case 8: // loa
		b.hcn(cx-0.55*s, cy-0.9*s, cx+0.55*s, cy+0.9*s, &m, &den)
		b.elip(cx-0.18*s, cy-0.72*s, cx+0.18*s, cy-0.36*s, &den, nil, 0, false)
		b.elip(cx-0.42*s, cy-0.15*s, cx+0.42*s, cy+0.69*s, &den, pm(Trang), 1, false)
	default: // phiếu mua hàng 500k
		b.hcn(cx-0.9*s, cy-0.5*s, cx+0.9*s, cy+0.5*s, &m, &den)
		for y := cy - 0.5*s; y < cy+0.5*s; y += 4 { // đường răng cưa (nét đứt)
			b.duong(cx+0.4*s, y, cx+0.4*s, math.Min(y+2, cy+0.5*s), 1, Trang)
		}
	}
}

// anhQua – ảnh w×h nền nen, món quà i ở giữa (nửa cạnh s).
func anhQua(i, w, h int, s float64, xam bool, nen color.NRGBA) *image.NRGBA {
	b := moiBut(w, h)
	b.chuNhat(0, 0, float64(w), float64(h), nen)
	cx, cy := float64(w)/2, float64(h)/2
	veQua(b, i, cx, cy, s, xam)
	a := b.anh()
	if i == 9 {
		c := anhChu("500K", math.Max(7, s*0.5), Trang, true)
		draw.Draw(a, c.Bounds().Add(image.Pt(int(cx-0.25*s)-c.Bounds().Dx()/2, int(cy)-c.Bounds().Dy()/2)), c,
			image.Point{}, draw.Over)
	}
	return a
}

// boTriO – các ô của bánh xe: mã quà hoặc -1 ("may mắn lần sau"); quà rải đều, tối thiểu 8 ô.
func boTriO(soQua int) []int {
	n := max(2*soQua, 8)
	o := make([]int, n)
	for i := range o {
		o[i] = -1
	}
	for i := 0; i < soQua; i++ {
		o[int(math.RoundToEven(float64(i)*float64(n)/float64(soQua)))%n] = i
	}
	return o
}

// ===================================================================== cửa sổ

const banKinh = 168 // bán kính bánh xe

type cuaSoVongQuay struct {
	*cuaSoNguoiDungCoSo
	nhanHien      string  // tên hiện trong nhật ký demo ("Bình (máy 2)")
	goc           float64 // góc xoay bánh xe (độ, ngược chiều kim đồng hồ)
	pha           string  // "" | "tu_do" | "dung"
	ttVe          tc.M    // trạng thái dùng để vẽ bánh xe
	choKq         tc.M    // kết quả chờ đến lúc bánh xe dừng
	ketQuaCuoi    []string
	nhip          int
	tBam, tDung   time.Time
	goc0, gocDich float64
	khoaDia       string
	dia           *image.NRGBA // ảnh đĩa các ô dựng ở góc gocDia
	gocDia        float64
	anhDia        *canvas.Image
	den           []*canvas.Circle
	nutQuay       *widget.Button
	thongBao      *oMau
	nhanLuat      *widget.Label
	nhanCon       *canvas.Text
	nhanDaQuay    *widget.Label
	oQua          []*canvas.Image
	vienQua       []*canvas.Rectangle
	nhanQua       []*canvas.Text
	nhanQua2      []*canvas.Text
	daVeQua       string
	ls            *dsChu
	dongLs        []string
}

func moCuaSoVongQuay(a fyne.App, ten, host string, cong int) *cuaSoVongQuay {
	x := &cuaSoVongQuay{goc: 90, nhanHien: ten}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Vòng quay may mắn", ten, host, cong, x)
	x.hoanTat(720)
	go func() { // khung hình 30 ms (chỉ vẽ khi đang quay)
		t := time.NewTicker(30 * time.Millisecond)
		defer t.Stop()
		for {
			select {
			case <-x.dong:
				return
			case <-t.C:
			}
			fyne.Do(x.khungHinh)
		}
	}()
	return x
}

func (x *cuaSoVongQuay) ve() fyne.CanvasObject {
	R := float64(banKinh)
	W, H := 2*banKinh+40, 2*banKinh+44
	cx, cy := R+20, R+26
	// nền + vành vàng (tĩnh)
	b := moiBut(W, H)
	b.chuNhat(0, 0, float64(W), float64(H), mau("#2b1055"))
	b.elip(cx-R-14, cy-R-14, cx+R+14, cy+R+14, pm(mau("#f4b400")), pm(mau("#a86b00")), 3, false)
	nen := oAnhTinh(b.anh(), float32(W), float32(H))
	x.anhDia = oAnhTinh(image.NewNRGBA(image.Rect(0, 0, 1, 1)), float32(2*banKinh+4), float32(2*banKinh+4))
	// trục + kim chỉ ở đỉnh (tĩnh, nền trong suốt)
	b = moiBut(W, H)
	b.elip(cx-30, cy-30, cx+30, cy+30, pm(mau("#d62828")), pm(Trang), 3, false)
	kim := []diem{{cx - 15, cy - R - 24}, {cx + 15, cy - R - 24}, {cx, cy - R + 12}}
	b.daGiac(kim, mau("#d62828"))
	b.gapKhuc(append(kim, kim[0]), 2, Trang, false)
	tren := b.anh()
	c := anhChu("MAY\nMẮN", 10, Trang, true)
	draw.Draw(tren, c.Bounds().Add(image.Pt(int(cx)-c.Bounds().Dx()/2, int(cy)-c.Bounds().Dy()/2)), c, image.Point{},
		draw.Over)
	banhXe := container.NewWithoutLayout(nen, x.anhDia)
	nen.Move(fyne.NewPos(0, 0))
	nen.Resize(fyne.NewSize(float32(W), float32(H)))
	x.anhDia.Move(fyne.NewPos(float32(cx-R-2), float32(cy-R-2)))
	x.anhDia.Resize(fyne.NewSize(float32(2*banKinh+4), float32(2*banKinh+4)))
	for k := 0; k < 24; k++ { // đèn viền
		g := float64(k) * 15 * math.Pi / 180
		d := canvas.NewCircle(mau("#c08000"))
		d.Move(fyne.NewPos(float32(cx+(R+7)*math.Cos(g)-4), float32(cy-(R+7)*math.Sin(g)-4)))
		d.Resize(fyne.NewSize(8, 8))
		x.den = append(x.den, d)
		banhXe.Add(d)
	}
	anhTren := oAnhTinh(tren, float32(W), float32(H))
	anhTren.Move(fyne.NewPos(0, 0))
	anhTren.Resize(fyne.NewSize(float32(W), float32(H)))
	banhXe.Add(anhTren)
	x.nutQuay = nut("QUAY", widget.DangerImportance, x.quay)
	x.thongBao = moiOMau("Chưa kết nối", XamNhat, Den, 13, 290, 44, true)
	trai := container.NewVBox(coDinh(banhXe, float32(W), float32(H)),
		container.NewHBox(coDinh(x.nutQuay, 120, 44), x.thongBao.O))

	x.nhanLuat = widget.NewLabel("Mỗi người 1 lượt")
	x.nhanCon = chu("–", DoDam, 15, true)
	x.nhanDaQuay = widget.NewLabel("–")
	luoi := container.New(layout.NewGridLayout(2))
	for i := range tc.TenQua {
		img := oAnhTinh(anhQua(i, 36, 36, 15, false, Trang), 36, 36)
		vien := canvas.NewRectangle(color.Transparent)
		vien.StrokeColor, vien.StrokeWidth = mau("#808080"), 1
		t1, t2 := chu(tc.TenQua[i], Den, 11, false), chu("", Den, 11, false)
		x.oQua, x.nhanQua, x.nhanQua2 = append(x.oQua, img), append(x.nhanQua, t1), append(x.nhanQua2, t2)
		x.vienQua = append(x.vienQua, vien)
		luoi.Add(container.NewHBox(coDinh(container.NewStack(img, vien), 36, 36),
			container.NewVBox(layout.NewSpacer(), t1, t2, layout.NewSpacer())))
	}
	x.ls = moiDsChu(7, 11, 300, 128)
	phai := container.NewVBox(chu("VÒNG QUAY MAY MẮN", mau("#7a1fa2"), 19, true), x.nhanLuat, x.nhanCon,
		x.nhanDaQuay, nhom("Quà tặng", coDinh(luoi, 300, 210)), nhom("Người trúng thưởng (trực tiếp)", x.ls.O))
	x.veBanhXe()
	x.nutQuay.Disable()
	return container.NewHBox(trai, phai)
}

// ------------------------------------------------------------ bánh xe

// dungDia – ảnh đĩa các ô ở góc goc (ô j từ goc + j·ext tới goc + (j+1)·ext), chữ xoay theo ô; dựng lại khi
// trạng thái quà đổi, hoặc khi bánh xe đứng yên ở góc mới: chữ ở nửa trái được lật cho dễ đọc như bản
// Python. Khi đang quay, ảnh đĩa (cả chữ) chỉ được xoay thêm goc − gocDia.
func (x *cuaSoVongQuay) dungDia(goc float64) {
	soQua := 10
	con := map[int64]bool{}
	if x.ttVe != nil {
		soQua = int(tc.SoNguyen(x.ttVe["so_qua"]))
		for _, q := range tc.DSM(x.ttVe["qua"]) {
			con[tc.SoNguyen(q["ma"])], _ = q["con"].(bool)
		}
	}
	o := boTriO(soQua)
	khoa := fmt.Sprint(soQua, con, math.Round(goc*10))
	if x.ttVe == nil {
		khoa = fmt.Sprint("?", math.Round(goc*10))
	}
	if khoa == x.khoaDia && x.dia != nil {
		return
	}
	x.khoaDia, x.gocDia = khoa, goc
	R := float64(banKinh)
	D := 2*banKinh + 4
	cx := float64(D) / 2
	ext := 360 / float64(len(o))
	b := moiBut(D, D)
	type nhanO struct {
		s       string
		m       color.NRGBA
		px      float64
		x, y, g float64
	}
	var nhan []nhanO
	for j, ma := range o {
		batDau := goc + float64(j)*ext
		var nen, mChu color.NRGBA
		var s string
		conMon, coMon := con[int64(ma)]
		switch {
		case ma < 0:
			nen, s, mChu = mau("#ffe8f0"), "May mắn\nlần sau", mau("#7a1fa2")
			if j%2 == 1 {
				nen = mau("#fff7e6")
			}
		case conMon || !coMon:
			nen, s, mChu = mau(mauQua[ma]), tenNgan[ma], Trang
			if ma == 2 || ma == 4 {
				mChu = mau("#202020")
			}
		default:
			nen, s, mChu = mau("#bdbdbd"), tenNgan[ma]+"\n(đã trao)", mau("#606060")
		}
		p := []diem{{cx, cx}}
		for t := 0; t <= 16; t++ {
			g := (batDau + ext*float64(t)/16) * math.Pi / 180
			p = append(p, diem{cx + R*math.Cos(g), cx - R*math.Sin(g)})
		}
		b.daGiac(p, nen)
		giua := batDau + ext/2
		gChu := math.Mod(math.Mod(giua, 360)+360, 360)
		if gChu > 90 && gChu < 270 { // chữ ở nửa trái: lật cho dễ đọc
			gChu -= 180
		}
		px := 12.0
		if ma < 0 || len(o) > 12 {
			px = 11
		}
		r := R * 0.62
		nhan = append(nhan, nhanO{s, mChu, px, cx + r*math.Cos(giua*math.Pi/180), cx - r*math.Sin(giua*math.Pi/180), gChu})
	}
	for j := range o { // đường ngăn trắng
		g := (goc + float64(j)*ext) * math.Pi / 180
		b.duong(cx, cx, cx+R*math.Cos(g), cx-R*math.Sin(g), 2, Trang)
	}
	b.elip(cx-R, cx-R, cx+R, cx+R, nil, pm(Trang), 2, false)
	a := b.anh()
	for _, n := range nhan {
		danXoay(a, anhChu(n.s, n.px, n.m, true), n.x, n.y, n.g)
	}
	x.dia = a
}

func (x *cuaSoVongQuay) veBanhXe() {
	if x.pha == "" || x.dia == nil {
		x.dungDia(x.goc)
	}
	D := x.dia.Bounds().Dx()
	ra := image.NewNRGBA(image.Rect(0, 0, D, D))
	danXoay(ra, x.dia, float64(D)/2, float64(D)/2, x.goc-x.gocDia)
	x.anhDia.Image = ra
	x.anhDia.Refresh()
	for k, d := range x.den { // đèn viền: nhấp nháy khi quay
		sang := k%2 == 0
		if x.pha != "" {
			sang = (k+x.nhip/3)%2 == 0
		}
		m := mau("#c08000")
		if sang {
			m = mau("#fffde0")
		}
		if d.FillColor != m {
			d.FillColor = m
			d.Refresh()
		}
	}
}

func (x *cuaSoVongQuay) quay() {
	if x.kn == nil || x.pha != "" {
		return
	}
	x.ttVe = x.tt
	x.pha, x.tBam, x.choKq = "tu_do", time.Now(), nil
	x.nutQuay.SetText("ĐANG QUAY")
	x.nutQuay.Disable()
	x.bao("Đang quay…", mau("#fff3c4"), Den)
	x.log.them("Bấm QUAY")
	x.guiYC("quay", nil)
}

func (x *cuaSoVongQuay) ketQua(_ string, _ tc.TS, tin tc.M, rtt float64) {
	var s string
	if ok, _ := tin["ok"].(bool); ok {
		s = "không trúng"
		if trung, _ := tin["trung"].(bool); trung {
			s = "trúng " + tc.Chuoi(tin["ten_qua"])
		}
	} else {
		s = "từ chối – " + tc.StrPy(tin["ly_do"])
	}
	x.log.them(fmt.Sprintf("  máy chủ trả lời sau %.1f ms: %s", rtt, s))
	x.choKq = tin
	if x.pha == "" { // (không xảy ra khi bấm nút; phòng khi gọi trực tiếp)
		x.ketThucQuay()
	}
}

func (x *cuaSoVongQuay) batDauDung() {
	tin := x.choKq
	o := boTriO(int(tc.SoNguyen(x.ttVe["so_qua"])))
	j := -1
	if ok, _ := tin["ok"].(bool); ok {
		if trung, _ := tin["trung"].(bool); trung {
			for k, ma := range o {
				if int64(ma) == tc.SoNguyen(tin["qua"]) {
					j = k
				}
			}
		}
	}
	if j < 0 {
		var trong []int
		for k, ma := range o {
			if ma < 0 {
				trong = append(trong, k)
			}
		}
		j = trong[rand.IntN(len(trong))]
	}
	ext := 360 / float64(len(o))
	dich := 90 - (float64(j)+0.5)*ext + (rand.Float64()*0.6-0.3)*ext
	m := math.Ceil((dich - (x.goc - 540)) / 360)
	x.goc0, x.gocDich = x.goc, dich-360*m
	x.tDung = time.Now()
	x.pha = "dung"
}

func (x *cuaSoVongQuay) khungHinh() {
	if x.pha == "" {
		return
	}
	x.nhip++
	if x.pha == "tu_do" {
		x.goc -= 20 // quay theo chiều kim đồng hồ
		if x.choKq != nil && time.Since(x.tBam) > 600*time.Millisecond {
			x.batDauDung()
		}
	} else {
		t := math.Min(1, time.Since(x.tDung).Seconds()/2.6)
		x.goc = x.goc0 + (x.gocDich-x.goc0)*(1-math.Pow(1-t, 3)) // giảm tốc (ease-out)
		if t >= 1 {
			x.pha = ""
			x.veBanhXe()
			x.ketThucQuay()
			return
		}
	}
	x.veBanhXe()
}

func (x *cuaSoVongQuay) ketThucQuay() {
	tin := x.choKq
	ok, _ := tin["ok"].(bool)
	trung, _ := tin["trung"].(bool)
	daQuay := tin["ma_loi"] == "da_quay"
	switch {
	case ok && trung:
		x.ketQuaCuoi = []string{"CHÚC MỪNG! Bạn trúng " + tc.Chuoi(tin["ten_qua"]), "#ffd700", "#000000"}
	case ok:
		x.ketQuaCuoi = []string{"Chúc bạn may mắn lần sau", "#d0d0ff", "#000000"}
	case daQuay:
		x.ketQuaCuoi = []string{"Bạn đã quay rồi – mỗi người 1 lượt", "#ffb347", "#000000"}
	default:
		x.ketQuaCuoi = []string{"Lỗi: " + tc.Chuoi(tin["ly_do"]), "#ff6060", "#ffffff"}
	}
	x.bao(x.ketQuaCuoi[0], mau(x.ketQuaCuoi[1]), mau(x.ketQuaCuoi[2]))
	x.log.them("Kết quả: " + x.ketQuaCuoi[0])
	if ok || daQuay {
		x.nutQuay.SetText("ĐÃ QUAY")
		x.nutQuay.Disable()
	} else {
		x.nutQuay.SetText("QUAY")
		x.nutQuay.Enable()
	}
	if x.tt != nil {
		x.ttVe = x.tt
		x.veBanhXe()
	}
}

func (x *cuaSoVongQuay) bao(text string, nen, mauChu color.Color) { x.thongBao.dat(text, nen, mauChu) }

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoVongQuay) capNhat(tt tc.M) {
	if x.pha == "" {
		moiDatLai := x.ttVe != nil && tc.SoNguyen(tt["da_quay"]) < tc.SoNguyen(x.ttVe["da_quay"])
		x.ttVe = tt
		x.veBanhXe()
		if moiDatLai || x.ketQuaCuoi == nil { // đặt lại chương trình: được quay lại
			x.ketQuaCuoi = nil
			x.nutQuay.SetText("QUAY")
			x.nutQuay.Enable()
			x.bao("Mời bạn bấm QUAY", XamNhat, Den)
		}
	}
	soQua := int(tc.SoNguyen(tt["so_qua"]))
	conLai := tc.SoNguyen(tt["con_lai"])
	x.nhanLuat.SetText("Mỗi người 1 lượt · xác suất trúng " + tc.ChuP(tc.SoThuc(tt["p"])) + " mỗi lượt")
	m := color.Color(DoDam)
	if conLai == 0 {
		m = XamChu
	}
	datChu(x.nhanCon, fmt.Sprintf("Còn %d/%d quà", conLai, soQua), m)
	x.nhanDaQuay.SetText(fmt.Sprintf("Đã quay: %s lượt · %d lượt trúng", soNghin(tc.SoNguyen(tt["da_quay"])),
		tc.SoNguyen(tt["so_trung"])))
	qua := tc.DSM(tt["qua"])
	khoa := fmt.Sprint(soQua)
	for _, q := range qua {
		khoa += fmt.Sprint(q["con"])
	}
	if khoa != x.daVeQua {
		x.daVeQua = khoa
		for i, img := range x.oQua {
			switch {
			case i >= soQua: // chương trình ít quà hơn: ẩn ô thừa
				img.Hide()
				x.vienQua[i].Hide()
			default:
				con, _ := qua[i]["con"].(bool)
				nen := Trang
				if !con {
					nen = mau("#e8e8e8")
				}
				img.Image = anhQua(i, 36, 36, 15, !con, nen)
				img.Show()
				x.vienQua[i].Show()
				img.Refresh()
			}
		}
	}
	for i := range x.oQua {
		if i >= soQua || i >= len(qua) {
			datChu(x.nhanQua[i], "", Den)
			datChu(x.nhanQua2[i], "", Den)
			continue
		}
		q := qua[i]
		if con, _ := q["con"].(bool); con {
			datChu(x.nhanQua[i], tc.Chuoi(q["ten"]), XanhDam)
			datChu(x.nhanQua2[i], "còn", XanhDam)
		} else {
			ai, _ := q["nguoi"].(string)
			if ai == "" {
				ai = "?"
			}
			datChu(x.nhanQua[i], tc.Chuoi(q["ten"]), mau("#a00000"))
			datChu(x.nhanQua2[i], "→ "+ai, mau("#a00000"))
		}
	}
	var dong []string
	var cuaToi []bool
	for _, n := range tc.DSM(tt["nguoi_trung"]) {
		dong = append(dong, fmt.Sprintf("%s %-6s %s", tc.Chuoi(n["gio"]), tc.Chuoi(n["nguoi"]),
			tc.TenQua[tc.SoNguyen(n["qua"])]))
		cuaToi = append(cuaToi, tc.Chuoi(n["nguoi"]) == x.ten)
	}
	x.dongLs = dong
	x.ls.dat(dong, func(i int) color.Color {
		if cuaToi[i] {
			return mau("#0040c0")
		}
		return Den
	})
}

// soNghin – 12345 -> "12.345".
func soNghin(n int64) string { return strings.TrimSuffix(tc.Tien(n), " đ") }

func (x *cuaSoVongQuay) suKien(text string) { x.log.them("* " + text) }

func (x *cuaSoVongQuay) moiNhip() {}

func (x *cuaSoVongQuay) dongDinhKy() string {
	kq := "chưa quay"
	if x.ketQuaCuoi != nil {
		kq = x.ketQuaCuoi[0]
	}
	return fmt.Sprintf("còn %d/%d quà, đã quay %d lượt; bạn: %s", tc.SoNguyen(x.tt["con_lai"]), tc.SoNguyen(x.tt["so_qua"]),
		tc.SoNguyen(x.tt["da_quay"]), kq)
}

func (x *cuaSoVongQuay) daKetNoi()  {}
func (x *cuaSoVongQuay) matKetNoi() {}
