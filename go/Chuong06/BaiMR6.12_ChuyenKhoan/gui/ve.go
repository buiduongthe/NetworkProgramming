package main

// Bút vẽ ảnh tĩnh (thay Canvas của Tkinter cho các hình khó dựng bằng đối tượng Fyne: đa giác cong,
// cung, nền chuyển màu). Vẽ trên ảnh lớn gấp 3 lần rồi thu nhỏ trung bình (khử răng cưa) -> ảnh RGBA
// đưa vào canvas.Image. Dùng chung cho các bài tranh chấp (giống nhau ở các bài).

import (
	"image"
	"image/color"
	"math"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
)

const sieuMau = 3 // hệ số vẽ dư (supersampling)

type diem struct{ x, y float64 }

type but struct {
	w, h int
	a    *image.NRGBA
}

func moiBut(w, h int) *but {
	return &but{w, h, image.NewNRGBA(image.Rect(0, 0, w*sieuMau, h*sieuMau))}
}

func (b *but) cham(x, y int, m color.NRGBA) {
	if x >= 0 && y >= 0 && x < b.w*sieuMau && y < b.h*sieuMau {
		if m.A == 0xFF {
			b.a.SetNRGBA(x, y, m)
			return
		}
		c := b.a.NRGBAAt(x, y)
		t := float64(m.A) / 255
		pha := func(p, q uint8) uint8 { return uint8(float64(p)*(1-t) + float64(q)*t + 0.5) }
		b.a.SetNRGBA(x, y, color.NRGBA{pha(c.R, m.R), pha(c.G, m.G), pha(c.B, m.B), max(c.A, m.A)})
	}
}

// chuNhat – hình chữ nhật đặc (tọa độ ảnh thật).
func (b *but) chuNhat(x0, y0, x1, y1 float64, m color.NRGBA) {
	for y := int(y0 * sieuMau); y < int(y1*sieuMau); y++ {
		for x := int(x0 * sieuMau); x < int(x1*sieuMau); x++ {
			b.cham(x, y, m)
		}
	}
}

// daGiac – đa giác đặc (quét dòng, chẵn-lẻ).
func (b *but) daGiac(p []diem, m color.NRGBA) {
	if len(p) < 3 {
		return
	}
	ymin, ymax := math.Inf(1), math.Inf(-1)
	for _, q := range p {
		ymin, ymax = math.Min(ymin, q.y), math.Max(ymax, q.y)
	}
	for y := int(ymin * sieuMau); y <= int(ymax*sieuMau); y++ {
		yy := (float64(y) + 0.5) / sieuMau
		var xs []float64
		for i := range p {
			a, c := p[i], p[(i+1)%len(p)]
			if (a.y <= yy && c.y > yy) || (c.y <= yy && a.y > yy) {
				xs = append(xs, a.x+(yy-a.y)*(c.x-a.x)/(c.y-a.y))
			}
		}
		for i := 1; i < len(xs); i++ { // sắp xếp chèn (ít điểm)
			for j := i; j > 0 && xs[j] < xs[j-1]; j-- {
				xs[j], xs[j-1] = xs[j-1], xs[j]
			}
		}
		for i := 0; i+1 < len(xs); i += 2 {
			for x := int(xs[i]*sieuMau + 0.5); x < int(xs[i+1]*sieuMau+0.5); x++ {
				b.cham(x, y, m)
			}
		}
	}
}

// elip – elip đặc (nen) và/hoặc viền dày day (điểm ảnh thật); tuNua: chỉ nửa trên (cung 0–180°).
func (b *but) elip(x0, y0, x1, y1 float64, nen *color.NRGBA, vien *color.NRGBA, day float64, tuNua bool) {
	cx, cy, rx, ry := (x0+x1)/2, (y0+y1)/2, (x1-x0)/2, (y1-y0)/2
	if rx <= 0 || ry <= 0 {
		return
	}
	for y := int((y0 - day) * sieuMau); y <= int((y1+day)*sieuMau); y++ {
		for x := int((x0 - day) * sieuMau); x <= int((x1+day)*sieuMau); x++ {
			px, py := (float64(x)+0.5)/sieuMau, (float64(y)+0.5)/sieuMau
			if tuNua && py > cy {
				continue
			}
			dx, dy := (px-cx)/rx, (py-cy)/ry
			r := math.Sqrt(dx*dx + dy*dy)
			if nen != nil && r <= 1 {
				b.cham(x, y, *nen)
			}
			if vien != nil && math.Abs(r-1)*math.Min(rx, ry) <= day/2 {
				b.cham(x, y, *vien)
			}
		}
	}
}

func (b *but) tron(cx, cy, r float64, m color.NRGBA) {
	b.elip(cx-r, cy-r, cx+r, cy+r, &m, nil, 0, false)
}

// duong – đoạn thẳng dày day.
func (b *but) duong(x0, y0, x1, y1, day float64, m color.NRGBA) {
	r := math.Max(day/2, 0.5)
	minx, maxx := math.Min(x0, x1)-r, math.Max(x0, x1)+r
	miny, maxy := math.Min(y0, y1)-r, math.Max(y0, y1)+r
	dx, dy := x1-x0, y1-y0
	l2 := dx*dx + dy*dy
	for y := int(miny * sieuMau); y <= int(maxy*sieuMau); y++ {
		for x := int(minx * sieuMau); x <= int(maxx*sieuMau); x++ {
			px, py := (float64(x)+0.5)/sieuMau, (float64(y)+0.5)/sieuMau
			t := 0.0
			if l2 > 0 {
				t = math.Max(0, math.Min(1, ((px-x0)*dx+(py-y0)*dy)/l2))
			}
			ex, ey := px-(x0+t*dx), py-(y0+t*dy)
			if ex*ex+ey*ey <= r*r {
				b.cham(x, y, m)
			}
		}
	}
}

// gapKhuc – đường gấp khúc (mịn: đi qua các điểm bằng spline Catmull-Rom).
func (b *but) gapKhuc(p []diem, day float64, m color.NRGBA, muot bool) {
	if muot {
		p = splineMin(p, 8)
	}
	for i := 0; i+1 < len(p); i++ {
		b.duong(p[i].x, p[i].y, p[i+1].x, p[i+1].y, day, m)
	}
}

func splineMin(p []diem, buoc int) []diem {
	if len(p) < 3 {
		return p
	}
	var kq []diem
	for i := 0; i+1 < len(p); i++ {
		p0, p1, p2, p3 := p[max(i-1, 0)], p[i], p[i+1], p[min(i+2, len(p)-1)]
		for s := 0; s < buoc; s++ {
			t := float64(s) / float64(buoc)
			t2, t3 := t*t, t*t*t
			f := func(a, b, c, d float64) float64 {
				return 0.5 * (2*b + (-a+c)*t + (2*a-5*b+4*c-d)*t2 + (-a+3*b-3*c+d)*t3)
			}
			kq = append(kq, diem{f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)})
		}
	}
	return append(kq, p[len(p)-1])
}

// anh – thu nhỏ trung bình về kích thước thật.
func (b *but) anh() *image.NRGBA {
	ra := image.NewNRGBA(image.Rect(0, 0, b.w, b.h))
	n := float64(sieuMau * sieuMau)
	for y := 0; y < b.h; y++ {
		for x := 0; x < b.w; x++ {
			var r, g, bl, a float64
			for j := 0; j < sieuMau; j++ {
				for i := 0; i < sieuMau; i++ {
					c := b.a.NRGBAAt(x*sieuMau+i, y*sieuMau+j)
					t := float64(c.A)
					r, g, bl, a = r+float64(c.R)*t, g+float64(c.G)*t, bl+float64(c.B)*t, a+t
				}
			}
			if a > 0 {
				ra.SetNRGBA(x, y, color.NRGBA{uint8(r / a), uint8(g / a), uint8(bl / a), uint8(a / n)})
			}
		}
	}
	return ra
}

func pm(m color.NRGBA) *color.NRGBA { return &m }

// oAnhTinh – ảnh tĩnh kích thước cố định.
func oAnhTinh(a image.Image, w, h float32) *canvas.Image {
	img := canvas.NewImageFromImage(a)
	img.FillMode = canvas.ImageFillStretch
	img.SetMinSize(fyne.NewSize(w, h))
	return img
}
