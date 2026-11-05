package main

// Biểu đồ Gantt: một widget tự vẽ bằng các đối tượng canvas (hình chữ nhật, đường, chữ, ảnh điểm).
// Mỗi lần Refresh (hoặc đổi kích thước) dựng lại toàn bộ danh sách đối tượng từ kết quả mô phỏng và thời
// điểm đang xem – cách làm giống Canvas.delete("all") rồi vẽ lại của bản Tkinter.

import (
	"image/color"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/widget"

	L "github.com/buiduongthe/NetworkProgramming/go/Chuong05/BaiMR5.6_MoPhongThucThiTacVu/laplich"
)

type banDo struct {
	widget.BaseWidget
	u *ungDung
}

func moiBanDo(u *ungDung) *banDo {
	b := &banDo{u: u}
	b.ExtendBaseWidget(b)
	return b
}

func (b *banDo) CreateRenderer() fyne.WidgetRenderer { return &veBanDo{b: b} }

type veBanDo struct {
	b    *banDo
	size fyne.Size
	nen  []fyne.CanvasObject // lưới (vẽ dưới cùng)
	objs []fyne.CanvasObject
	all  []fyne.CanvasObject
}

func (r *veBanDo) Layout(s fyne.Size)           { r.size = s; r.dung() }
func (r *veBanDo) MinSize() fyne.Size           { return fyne.NewSize(600, 320) }
func (r *veBanDo) Refresh()                     { r.dung(); canvas.Refresh(r.b) }
func (r *veBanDo) Objects() []fyne.CanvasObject { return r.all }
func (r *veBanDo) Destroy()                     {}

// ---- các hàm vẽ cơ bản

func (r *veBanDo) chuNhat(x0, y0, x1, y1 float32, fill color.Color, vien color.Color) {
	rc := canvas.NewRectangle(fill)
	if vien != nil {
		rc.StrokeColor = vien
		rc.StrokeWidth = 1
	}
	rc.Move(fyne.NewPos(x0, y0))
	rc.Resize(fyne.NewSize(max(0, x1-x0), max(0, y1-y0)))
	r.objs = append(r.objs, rc)
}

func duong(x0, y0, x1, y1 float32, c color.Color, w float32) *canvas.Line {
	l := canvas.NewLine(c)
	l.StrokeWidth = w
	l.Position1 = fyne.NewPos(x0, y0)
	l.Position2 = fyne.NewPos(x1, y1)
	return l
}

// vienDut – viền nét đứt (Fyne không có kiểu dash cho hình chữ nhật): ghép các đoạn ngắn.
func (r *veBanDo) vienDut(x0, y0, x1, y1 float32, c color.Color, net, ho float32) {
	ngang := func(y float32) {
		for x := x0; x < x1; x += net + ho {
			r.objs = append(r.objs, duong(x, y, min(x+net, x1), y, c, 1))
		}
	}
	doc := func(x float32) {
		for y := y0; y < y1; y += net + ho {
			r.objs = append(r.objs, duong(x, y, x, min(y+net, y1), c, 1))
		}
	}
	ngang(y0)
	ngang(y1)
	doc(x0)
	doc(x1)
}

// gachCheo – ô nhạt có gạch chéo (như stipple của Tk), vẽ bằng ảnh điểm.
func (r *veBanDo) gachCheo(x0, y0, x1, y1 float32, nhat, dam color.NRGBA) {
	rs := canvas.NewRasterWithPixels(func(x, y, w, h int) color.Color {
		if (x+y)%6 == 0 {
			return dam
		}
		return nhat
	})
	rs.Move(fyne.NewPos(x0, y0))
	rs.Resize(fyne.NewSize(max(0, x1-x0), max(0, y1-y0)))
	r.objs = append(r.objs, rs)
	vien := canvas.NewRectangle(color.Transparent)
	vien.StrokeColor = dam
	vien.StrokeWidth = 1
	vien.Move(fyne.NewPos(x0, y0))
	vien.Resize(fyne.NewSize(max(0, x1-x0), max(0, y1-y0)))
	r.objs = append(r.objs, vien)
}

// chu – neo: 'w' (trái, giữa dọc), 'e' (phải), 'c' (giữa).
func (r *veBanDo) chu(x, y float32, s string, c color.Color, co float32, dam bool, neo byte) {
	t := canvas.NewText(s, c)
	t.TextSize = co
	t.TextStyle = fyne.TextStyle{Bold: dam}
	kt := fyne.MeasureText(s, co, t.TextStyle)
	switch neo {
	case 'e':
		x -= kt.Width
	case 'c':
		x -= kt.Width / 2
	}
	t.Move(fyne.NewPos(x, y-kt.Height/2))
	t.Resize(kt)
	r.objs = append(r.objs, t)
}

// ---- vẽ toàn bộ

func (r *veBanDo) dung() {
	u := r.b.u
	r.nen, r.objs = nil, nil
	W, H := max(200, r.size.Width), max(200, r.size.Height)
	D := u.doDai()
	t := min(int(u.T), D)
	defer func() { r.all = append(r.nen, r.objs...) }()
	if len(u.ketQua) == 0 {
		r.chu(W/2, H/2, "Chọn một chế độ để bắt đầu mô phỏng", hex("#667085"), 18, false, 'c')
		return
	}
	trai, phai := float32(110), W-20
	X := func(n float64) float32 { return trai + (phai-trai)*float32(n)/float32(max(1, D)) }
	soHang := float32(0)
	for _, x := range u.ketQua {
		soHang += float32(len(x.kq.Lan)) + 1.2
		if len(x.kq.IO) > 0 {
			soHang++
		}
	}
	h := min(48, (H-80)/soHang) // chừa chỗ cho trục thời gian và hai hàng chú giải
	y := float32(8)
	for _, x := range u.ketQua {
		r.chu(8, y+h*0.4, L.CheDo[x.cheDo], chu, 13, true, 'w')
		y += h * 0.9
		for _, lan := range x.kq.Lan {
			r.veLan(lan, y, h, X, t)
			y += h
		}
		if len(x.kq.IO) > 0 {
			r.veIO(x.kq, y, h, X, t)
			y += h
		}
		y += h * 0.3
	}
	// trục thời gian (lưới ở dưới cùng) và đường "bây giờ"
	for g := 0; g <= D; g += 5 {
		xg := X(float64(g))
		c := hex("#c9ced8")
		if g%10 != 0 {
			c = luoi
		}
		r.nen = append(r.nen, duong(xg, 4, xg, y, c, 1))
		if g%10 == 0 {
			r.chu(xg, y+10, strconv.Itoa(g/10)+" s", hex("#667085"), 12, false, 'c')
		}
	}
	xt := X(u.T)
	r.objs = append(r.objs, duong(xt, 2, xt, y, hex("#f2b705"), 2))
	r.veChuGiai(y+30, W)
}

func (r *veBanDo) veLan(lan L.Lan, y, h float32, X func(float64) float32, t int) {
	u := r.b.u
	o := lan.O
	r.chu(104, y+h/2, lan.Ten, chu, 13, false, 'e')
	r.chuNhat(X(0), y+2, X(float64(len(o))), y+h-2, hex("#f6f7f9"), nil)
	for j := 0; j < min(len(o), t+1); { // gộp các nhịp liên tiếp cùng trạng thái thành một khối
		k := j
		for k+1 < len(o) && k+1 <= t && o[k+1] == o[j] {
			k++
		}
		tt, nhan := o[j].TrangThai, o[j].Nhan
		x0, x1 := X(float64(j)), X(float64(k+1))
		if k >= t {
			x1 = X(min(float64(k+1), u.T))
		}
		switch tt {
		case L.CPU:
			r.chuNhat(x0, y+3, x1, y+h-3, mau[nhan], nil)
			if x1-x0 > 18 {
				r.chu((x0+x1)/2, y+h/2, nhan, color.White, 12, true, 'c')
			}
		case L.IO:
			r.gachCheo(x0, y+6, x1, y+h-6, nhat[nhan], mau[nhan])
			if x1-x0 > 50 {
				r.chu((x0+x1)/2, y+h/2, nhan+" chờ I/O", chu, 11, false, 'c')
			}
		case L.SanSang:
			r.vienDut(x0, y+5, x1, y+h-5, mau[nhan], 3, 2)
		case L.Cho:
			r.chuNhat(x0, y+8, x1, y+h-8, hex("#e9ecef"), nil)
			r.vienDut(x0, y+8, x1, y+h-8, hex("#adb5bd"), 1, 2)
			if x1-x0 > 50 {
				r.chu((x0+x1)/2, y+h/2, "chờ join", hex("#667085"), 11, false, 'c')
			}
		}
		j = k + 1
	}
}

func (r *veBanDo) veIO(kq *L.KetQua, y, h float32, X func(float64) float32, t int) {
	r.chu(104, y+h/2, "I/O chờ", chu, 13, false, 'e')
	r.chuNhat(X(0), y+2, X(float64(kq.Tong)), y+h-2, hex("#f6fbf3"), nil)
	nMax := 1
	for _, n := range kq.IO {
		nMax = max(nMax, n)
	}
	for j := 0; j < min(len(kq.IO), t+1); j++ {
		if n := kq.IO[j]; n > 0 {
			hh := (h - 6) * float32(n) / float32(nMax)
			r.chuNhat(X(float64(j)), y+h-3-hh, X(float64(j+1)), y+h-3, hex("#7fb069"), nil)
		}
	}
}

// veChuGiai – chú giải: mỗi mục một ô mẫu + chữ; vị trí tính theo độ rộng chữ thật để không chồng nhau.
func (r *veBanDo) veChuGiai(y, W float32) {
	type muc struct {
		kieu int // 0 đặc, 1 gạch chéo, 2 viền đứt, 3 chờ join
		c    color.NRGBA
		s    string
	}
	var ds []muc
	for _, t := range []string{"T1", "T2", "T3", "U"} {
		ds = append(ds, muc{0, mau[t], t + " dùng CPU"})
	}
	ds = append(ds, muc{1, mau["T1"], "bị chặn chờ I/O"}, muc{2, mau["T1"], "sẵn sàng, chờ lõi"},
		muc{3, hex("#adb5bd"), "chờ join"}, muc{0, hex("#7fb069"), "I/O bất đồng bộ (không giữ luồng)"})
	x := float32(20)
	for _, m := range ds {
		w := fyne.MeasureText(m.s, 13, fyne.TextStyle{}).Width
		if x+23+w > W-10 { // hết chỗ: xuống hàng
			x, y = 20, y+22
		}
		switch m.kieu {
		case 0:
			r.chuNhat(x, y-6, x+18, y+6, m.c, nil)
		case 1:
			r.gachCheo(x, y-6, x+18, y+6, nhat["T1"], m.c)
		case 2:
			r.vienDut(x, y-6, x+18, y+6, m.c, 3, 2)
		case 3:
			r.chuNhat(x, y-6, x+18, y+6, hex("#e9ecef"), nil)
			r.vienDut(x, y-6, x+18, y+6, m.c, 1, 2)
		}
		r.chu(x+23, y, m.s, chu, 13, false, 'w')
		x += 23 + w + 16
	}
}
