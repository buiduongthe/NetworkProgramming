package main

// Các mảnh giao diện dùng chung: chủ đề sáng, chuyển ảnh RGB, ô nhật ký, chụp cửa sổ.
//
// QUY TẮC LUỒNG (Fyne ≥ 2.6): chỉ luồng giao diện được chạm vào widget. Goroutine nền (mạng, thu
// camera, hẹn giờ) muốn cập nhật giao diện phải gói việc đó trong fyne.Do(func(){…}) – Fyne
// xếp hàng hàm đó và chạy trên luồng giao diện. Các hàm xử lý sự kiện (OnTapped, OnChanged…) vốn đã
// chạy trên luồng giao diện, nên không được làm việc chặn lâu trong đó (kết nối mạng, mở ffmpeg…):
// việc chặn được đẩy sang goroutine, xong thì fyne.Do để báo kết quả.

import (
	"fmt"
	"image"
	"image/color"
	"image/draw"
	"image/png"
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
	if n == theme.SizeNameText {
		return 13
	}
	return theme.DefaultTheme().Size(n)
}

// ---------------------------------------------------------------- Ảnh RGB -> image.Image

// anhRGB – chuyển ảnh RGB thô (3 byte/điểm, như trên dây) sang image.NRGBA để canvas.Image vẽ.
func anhRGB(rgb []byte, w, h int) *image.NRGBA {
	img := image.NewNRGBA(image.Rect(0, 0, w, h))
	for i := 0; i < w*h && 3*i+2 < len(rgb); i++ {
		copy(img.Pix[4*i:4*i+3], rgb[3*i:3*i+3])
		img.Pix[4*i+3] = 255
	}
	return img
}

// khungAnh – canvas.Image có kích thước tối thiểu, giữ tỉ lệ, phóng kiểu điểm ảnh (ảnh nhỏ 160×120).
func khungAnh(w, h float32) *canvas.Image {
	img := canvas.NewImageFromImage(image.NewNRGBA(image.Rect(0, 0, 4, 3)))
	img.FillMode = canvas.ImageFillContain
	img.ScaleMode = canvas.ImageScalePixels
	img.SetMinSize(fyne.NewSize(w, h))
	return img
}

// ---------------------------------------------------------------- Nhật ký

// nhatKy – danh sách cuộn các dòng sự kiện (giữ tối đa 300 dòng).
type nhatKy struct {
	dong []string
	ds   *widget.List
}

func moiNhatKy() *nhatKy {
	n := &nhatKy{}
	n.ds = widget.NewList(func() int { return len(n.dong) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, o fyne.CanvasObject) { o.(*widget.Label).SetText(n.dong[i]) })
	return n
}

// them – an toàn khi gọi từ goroutine bất kỳ: việc sửa danh sách được chuyển về luồng giao diện.
func (n *nhatKy) them(s string) {
	dong := time.Now().Format("15:04:05") + "  " + s
	fyne.Do(func() {
		n.dong = append(n.dong, dong)
		if len(n.dong) > 300 {
			n.dong = n.dong[len(n.dong)-300:]
		}
		n.ds.Refresh()
		n.ds.ScrollToBottom()
	})
}

// ---------------------------------------------------------------- Tiện ích nhỏ

// nhanCat – nhãn một dòng, chữ dài bị cắt "…" (không làm cửa sổ phình to theo nội dung).
func nhanCat(chu string) *widget.Label {
	l := widget.NewLabel(chu)
	l.Truncation = fyne.TextTruncateEllipsis
	return l
}

func nhom(tieuDe string, noiDung ...fyne.CanvasObject) fyne.CanvasObject {
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
			// Capture phải chạy trên luồng giao diện: gọi từ goroutine khác, OpenGL (WGL trên Windows) báo
			// "Failed to make context current" và ảnh ra màu nền trống.
			var anh image.Image
			fyne.DoAndWait(func() { anh = w.Canvas().Capture() })
			kq = append(kq, anh)
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
	if err := exec.Command("import", "-window", "root", tam).Run(); err != nil {
		return w.Canvas().Capture()
	}
	f, err := os.Open(tam)
	if err != nil {
		return w.Canvas().Capture()
	}
	defer f.Close()
	anh, err := png.Decode(f)
	if err != nil {
		return w.Canvas().Capture()
	}
	var kt fyne.Size
	var tl float32
	fyne.DoAndWait(func() { kt, tl = w.Canvas().Size(), w.Canvas().Scale() })
	r := image.Rect(0, 0, int(kt.Width*tl), int(kt.Height*tl)).Intersect(anh.Bounds())
	ra := image.NewNRGBA(image.Rect(0, 0, r.Dx(), r.Dy()))
	draw.Draw(ra, ra.Bounds(), anh, r.Min, draw.Src)
	return ra
}

func coLenh(ten string) bool { _, err := exec.LookPath(ten); return err == nil }

// ghepAnhChup – ghép ảnh các cửa sổ (kèm tiêu đề) thành một tệp PNG: dùng chính Fyne để vẽ tiêu đề
// tiếng Việt – tạo một cửa sổ chứa các ảnh, chụp cửa sổ đó.
func ghepAnhChup(a fyne.App, tren []image.Image, tieuDeTren []string, duoi []image.Image,
	tieuDeDuoi []string, duongDan string) (int, int, error) {
	o := func(anh image.Image, tieuDe string) fyne.CanvasObject {
		img := canvas.NewImageFromImage(anh)
		img.FillMode = canvas.ImageFillOriginal
		img.ScaleMode = canvas.ImageScalePixels
		nhan := widget.NewLabelWithStyle(tieuDe, fyne.TextAlignLeading, fyne.TextStyle{Bold: true})
		vien := canvas.NewRectangle(color.NRGBA{0x60, 0x70, 0x80, 0xFF})
		return container.NewVBox(nhan, container.NewStack(vien, container.NewPadded(img)))
	}
	hangTren := container.NewHBox()
	for i, anh := range tren {
		hangTren.Add(o(anh, tieuDeTren[i]))
	}
	hangDuoi := container.NewHBox()
	for i, anh := range duoi {
		hangDuoi.Add(o(anh, tieuDeDuoi[i]))
	}
	var w fyne.Window
	fyne.DoAndWait(func() {
		w = a.NewWindow("Ảnh ghép")
		w.SetContent(container.NewPadded(container.NewVBox(hangTren, hangDuoi)))
		w.Resize(w.Content().MinSize())
		w.Show()
	})
	time.Sleep(time.Second)
	anh := chupCuaSo([]fyne.Window{w})[0]
	fyne.DoAndWait(w.Close)
	f, err := os.Create(duongDan)
	if err != nil {
		return 0, 0, err
	}
	defer f.Close()
	b := anh.Bounds()
	return b.Dx(), b.Dy(), png.Encode(f, anh)
}
