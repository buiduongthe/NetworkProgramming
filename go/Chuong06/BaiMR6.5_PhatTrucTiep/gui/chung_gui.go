package main

// Các mảnh giao diện dùng chung: chủ đề sáng, ảnh RGB, thanh đo mức dBFS, ô nhật ký, bảng cột cố
// định, chụp cửa sổ.
//
// QUY TẮC LUỒNG (Fyne ≥ 2.6, như MR6.2/MR6.3): chỉ luồng giao diện được chạm vào widget. Goroutine
// nền của lõi (mạng, âm thanh 20 ms, hiển thị 5 ms) ghi số liệu vào đối tượng lõi dưới khóa; một
// goroutine hẹn giờ của cửa sổ đọc TrangThai() rồi gói việc vẽ trong fyne.Do(func(){…}). Hàm xử lý
// sự kiện (OnTapped…) chạy trên luồng giao diện nên không làm việc chặn lâu (kết nối mạng, mở
// ffmpeg, tạo tệp mẫu…): việc đó đẩy sang goroutine, xong thì fyne.Do để báo kết quả.

import (
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// temSang – chủ đề sáng cố định (dễ đọc khi in ảnh chụp), nền hơi xanh như mẫu.
type temSang struct{}

func (temSang) Color(n fyne.ThemeColorName, _ fyne.ThemeVariant) color.Color {
	switch n {
	case theme.ColorNameBackground:
		return color.NRGBA{0xE8, 0xEE, 0xF6, 0xFF}
	case theme.ColorNameInputBackground, theme.ColorNameMenuBackground, theme.ColorNameOverlayBackground:
		return color.White
	}
	return theme.DefaultTheme().Color(n, theme.VariantLight)
}
func (temSang) Font(s fyne.TextStyle) fyne.Resource     { return theme.DefaultTheme().Font(s) }
func (temSang) Icon(n fyne.ThemeIconName) fyne.Resource { return theme.DefaultTheme().Icon(n) }
func (temSang) Size(n fyne.ThemeSizeName) float32 {
	switch n {
	case theme.SizeNameText:
		return 12
	case theme.SizeNamePadding:
		return 3
	case theme.SizeNameInnerPadding:
		return 5
	}
	return theme.DefaultTheme().Size(n)
}

// ---------------------------------------------------------------- Ảnh

// khungAnh – canvas.Image kích thước cố định, phóng kiểu điểm ảnh (ảnh nhỏ 160×120 phóng ×2).
func khungAnh(w, h float32) *canvas.Image {
	img := canvas.NewImageFromImage(image.NewNRGBA(image.Rect(0, 0, 4, 3)))
	img.FillMode = canvas.ImageFillStretch
	img.ScaleMode = canvas.ImageScalePixels
	img.SetMinSize(fyne.NewSize(w, h))
	return img
}

// oAnh – ảnh trên nền đen kèm chữ chờ ở giữa ("Chưa có luồng"…).
func oAnh(img *canvas.Image, chu *canvas.Text) fyne.CanvasObject {
	nen := canvas.NewRectangle(color.Black)
	nen.SetMinSize(img.MinSize())
	return container.NewStack(nen, img, container.NewCenter(chu))
}

func chuCho(s string) *canvas.Text {
	t := canvas.NewText(s, color.NRGBA{0xC0, 0xC0, 0xC0, 0xFF})
	t.TextSize = 12
	return t
}

// ---------------------------------------------------------------- Thanh đo mức

// thanhDo – ProgressBar −60 … 0 dBFS, chữ hiện số dBFS; mức hiện rơi dần 2 dB mỗi lần vẽ (như kim
// đồng hồ đo). Đặt t.bar vào bố cục.
type thanhDo struct {
	bar      *widget.ProgressBar
	db, hien float64
}

func moiThanhDo() *thanhDo {
	t := &thanhDo{bar: widget.NewProgressBar(), db: math.Inf(-1), hien: math.Inf(-1)}
	t.bar.Min, t.bar.Max = -60, 0
	t.bar.TextFormatter = func() string {
		if t.db < -90 {
			return "--- dBFS"
		}
		return fmt.Sprintf("%.1f dBFS", t.db)
	}
	t.bar.SetValue(-60)
	return t
}

// datMuc – gọi trên luồng giao diện.
func (t *thanhDo) datMuc(db float64) {
	t.db = db
	if db > t.hien {
		t.hien = db
	} else {
		t.hien -= 2
	}
	t.bar.SetValue(math.Max(-60, math.Min(0, t.hien)))
	t.bar.Refresh()
}

// ---------------------------------------------------------------- Nhật ký

// inRa – true khi chạy riêng từng cửa sổ (không qua demo_gui): mỗi dòng nhật ký cũng in ra dòng lệnh
// kèm tiền tố [ten], để chuyển hướng được vào tệp nhật ký khi thử với thiết bị thật.
var inRa bool

// nhatKy – danh sách cuộn các dòng sự kiện (giữ tối đa 500 dòng).
type nhatKy struct {
	ten  string
	dong []string
	ds   *widget.List
}

func moiNhatKy(ten string) *nhatKy {
	n := &nhatKy{ten: ten}
	n.ds = widget.NewList(func() int { return len(n.dong) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, o fyne.CanvasObject) { o.(*widget.Label).SetText(n.dong[i]) })
	return n
}

// them – an toàn khi gọi từ goroutine bất kỳ: việc sửa danh sách được chuyển về luồng giao diện.
func (n *nhatKy) them(s string) {
	gio := time.Now().Format("15:04:05")
	if inRa {
		fmt.Printf("%s [%s] %s\n", gio, n.ten, s)
	}
	dong := gio + " " + s
	fyne.Do(func() {
		n.dong = append(n.dong, dong)
		if len(n.dong) > 500 {
			n.dong = n.dong[len(n.dong)-500:]
		}
		n.ds.Refresh()
		n.ds.ScrollToBottom()
	})
}

// thongKeDinhKy – khi chạy riêng (inRa): cứ chuKy giây in một dòng số liệu ra dòng lệnh (không ghi vào
// ô nhật ký) – để tệp nhật ký khi thử thiết bị thật cho thấy tiếng/hình có thật sự đi qua.
type thongKeDinhKy struct {
	ten   string
	chuKy time.Duration
	han   time.Time
}

func moiThongKeDinhKy(ten string) *thongKeDinhKy {
	return &thongKeDinhKy{ten, 5 * time.Second, time.Now().Add(5 * time.Second)}
}

func (t *thongKeDinhKy) inNeuDenHan(taoDong func() string) {
	bayGio := time.Now()
	if !inRa || bayGio.Before(t.han) {
		return
	}
	t.han = bayGio.Add(t.chuKy) // không "đuổi bù" các chu kỳ đã lỡ (lúc chưa phát)
	fmt.Printf("%s [%s] %s\n", time.Now().Format("15:04:05"), t.ten, taoDong())
}

// ---------------------------------------------------------------- Bảng cột cố định

// cotCoDinh – bố cục một hàng: các ô đặt cạnh nhau theo độ rộng cho trước (bảng người xem).
type cotCoDinh []float32

func (c cotCoDinh) Layout(o []fyne.CanvasObject, kt fyne.Size) {
	x := float32(0)
	for i, ob := range o {
		w := c[min(i, len(c)-1)]
		ob.Move(fyne.NewPos(x, 0))
		ob.Resize(fyne.NewSize(w, kt.Height))
		x += w
	}
}

func (c cotCoDinh) MinSize(o []fyne.CanvasObject) fyne.Size {
	w, h := float32(0), float32(0)
	for i, ob := range o {
		w += c[min(i, len(c)-1)]
		h = max(h, ob.MinSize().Height)
	}
	return fyne.NewSize(w, h)
}

// hangBang – một hàng gồm các nhãn theo cột (cột căn phải khi phai[i]).
func hangBang(c cotCoDinh, phai []bool, dam bool) *fyne.Container {
	var o []fyne.CanvasObject
	for i := range c {
		l := widget.NewLabel("")
		l.Truncation = fyne.TextTruncateEllipsis
		if i < len(phai) && phai[i] {
			l.Alignment = fyne.TextAlignTrailing
		}
		l.TextStyle.Bold = dam
		o = append(o, l)
	}
	return container.New(c, o...)
}

func datHang(h *fyne.Container, gt []string) {
	for i, o := range h.Objects {
		if i < len(gt) {
			o.(*widget.Label).SetText(gt[i])
		}
	}
}

// ---------------------------------------------------------------- Tiện ích nhỏ

// coDinh – giữ đối tượng ở kích thước tối thiểu cho trước (danh sách, nhật ký trong hộp dọc).
func coDinh(o fyne.CanvasObject, w, h float32) fyne.CanvasObject {
	return container.New(layout.NewGridWrapLayout(fyne.NewSize(w, h)), o)
}

func nhanCat(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.Truncation = fyne.TextTruncateEllipsis
	return l
}

func nhanDam(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.TextStyle.Bold = true
	return l
}

func nhom(tieuDe string, noiDung ...fyne.CanvasObject) *widget.Card {
	return widget.NewCard("", tieuDe, container.NewVBox(noiDung...))
}

func nut(chu string, muc widget.Importance, f func()) *widget.Button {
	b := widget.NewButton(chu, f)
	b.Importance = muc
	return b
}

func oNhap(giaTri string) *widget.Entry {
	e := widget.NewEntry()
	e.SetText(giaTri)
	return e
}

func soNguyen(s string, macDinh int) int {
	if n, err := strconv.Atoi(strings.TrimSpace(s)); err == nil && n > 0 && n < 65536 {
		return n
	}
	return macDinh
}

func ms(x *float64, dau bool) string {
	if x == nil {
		return "–"
	}
	if dau {
		return fmt.Sprintf("%+.0f", *x)
	}
	return fmt.Sprintf("%.0f", *x)
}

func chuMs(x *float64, dau bool) string {
	if x == nil {
		return "–"
	}
	return ms(x, dau) + " ms"
}

// bam – mô phỏng người dùng bấm nút (demo tự thao tác); gọi trên luồng giao diện.
func bam(b *widget.Button) {
	if !b.Disabled() && b.OnTapped != nil {
		b.OnTapped()
	}
}

// ---------------------------------------------------------------- Chụp cửa sổ

// chupCuaSo – ảnh của từng cửa sổ. Dưới Xvfb (Linux, không có trình quản lý cửa sổ) mọi cửa sổ nằm
// chồng ở góc (0,0) và Canvas.Capture() của OpenGL phần mềm trả về ảnh trắng, nên: chỉ hiện một
// cửa sổ, chụp màn hình bằng ImageMagick "import -window root" rồi cắt đúng kích thước cửa sổ.
// Trên Windows/macOS (có GPU thật) dùng Canvas().Capture() của Fyne.
func chupCuaSo(ds []fyne.Window) []image.Image {
	dungImport := runtime.GOOS == "linux" && os.Getenv("DISPLAY") != "" && coLenh("import")
	var kq []image.Image
	for _, w := range ds {
		if !dungImport {
			var a image.Image
			fyne.DoAndWait(func() { a = w.Canvas().Capture() })
			kq = append(kq, a)
			continue
		}
		fyne.DoAndWait(func() {
			for _, w2 := range fyne.CurrentApp().Driver().AllWindows() {
				if w2 != w {
					w2.Hide() // ẩn mọi cửa sổ khác để không bị che
				}
			}
			w.Show()
		})
		time.Sleep(700 * time.Millisecond) // chờ cửa sổ được vẽ lại
		kq = append(kq, chupManHinh(w))
	}
	if dungImport {
		fyne.DoAndWait(func() {
			for _, w := range ds {
				w.Show()
			}
		})
	}
	return kq
}

func chupManHinh(w fyne.Window) image.Image {
	tam := filepath.Join(os.TempDir(), fmt.Sprintf("chup_%d.png", time.Now().UnixNano()))
	defer os.Remove(tam)
	var kt fyne.Size
	var tl float32
	fyne.DoAndWait(func() { kt, tl = w.Canvas().Size(), w.Canvas().Scale() })
	if err := exec.Command("import", "-window", "root", tam).Run(); err == nil {
		if f, err := os.Open(tam); err == nil {
			defer f.Close()
			if anh, err := png.Decode(f); err == nil {
				r := image.Rect(0, 0, int(kt.Width*tl), int(kt.Height*tl)).Intersect(anh.Bounds())
				ra := image.NewNRGBA(image.Rect(0, 0, r.Dx(), r.Dy()))
				draw.Draw(ra, ra.Bounds(), anh, r.Min, draw.Src)
				return ra
			}
		}
	}
	var a image.Image
	fyne.DoAndWait(func() { a = w.Canvas().Capture() })
	return a
}

func coLenh(ten string) bool { _, err := exec.LookPath(ten); return err == nil }

// ghepLuoi – ghép ảnh theo hàng (mỗi hàng các ảnh cạnh nhau, căn trên), nền xám #d0d0d0, viền 4
// điểm (như "convert +append/-append" ở bản Python), ghi PNG.
func ghepLuoi(duongDan string, hang [][]image.Image) (int, int, error) {
	W, H := 0, 0
	for _, h := range hang {
		w, c := 0, 0
		for _, a := range h {
			w += a.Bounds().Dx()
			c = max(c, a.Bounds().Dy())
		}
		W, H = max(W, w), H+c
	}
	W, H = W+8, H+8
	ra := image.NewNRGBA(image.Rect(0, 0, W, H))
	draw.Draw(ra, ra.Bounds(), image.NewUniform(color.NRGBA{0xD0, 0xD0, 0xD0, 0xFF}), image.Point{}, draw.Src)
	y := 4
	for _, h := range hang {
		x, c := 4, 0
		for _, a := range h {
			b := a.Bounds()
			draw.Draw(ra, image.Rect(x, y, x+b.Dx(), y+b.Dy()), a, b.Min, draw.Src)
			x += b.Dx()
			c = max(c, b.Dy())
		}
		y += c
	}
	f, err := os.Create(duongDan)
	if err != nil {
		return 0, 0, err
	}
	defer f.Close()
	return W, H, png.Encode(f, ra)
}
