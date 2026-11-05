package main

// Kịch bản minh họa (--demo): tự bấm nút như người dùng, chụp nội dung cửa sổ hai lần rồi ghép dọc
// thành ket_qua_gui.png (thư mục hiện hành) bằng image/png của thư viện chuẩn.
// Kịch bản chạy trên một goroutine riêng; mọi thao tác lên giao diện đều đi qua fyne.DoAndWait.

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
	"time"

	"fyne.io/fyne/v2"

	L "github.com/buiduongthe/NetworkProgramming/go/Chuong05/BaiMR5.6_MoPhongThucThiTacVu/laplich"
)

func in(s string) { fmt.Println("[gui] " + s) }

func demo(u *ungDung) {
	cho := func(ms int) { time.Sleep(time.Duration(ms) * time.Millisecond) }
	lam := fyne.DoAndWait
	chup := func() image.Image { return chupCuaSo(u.w) }

	cho(1500)
	in("Bấm 'Đa luồng – đồng bộ', dừng ở 3.8 s: U đã chen ngang Luồng T1, hai nửa T1 chạy trên Luồng T1 và T1b")
	lam(func() {
		u.chon([]string{"da_dong_bo"})
		u.dangChay = false
		u.datNutChay()
		u.T = 38
		u.ve()
	})
	cho(500)
	anh1 := chup()
	in("Bấm 'Đa luồng – bất đồng bộ', chạy hết: T2 chạy tiếp trên thợ khác sau I/O")
	var n0 int
	lam(func() {
		u.selToc.SetSelected("× 4")
		u.chon([]string{"da_bat_dong_bo"})
		n0 = u.soLanVe
	})
	t0 := time.Now()
	for dang := true; dang && time.Since(t0) < 10*time.Second; {
		cho(50)
		lam(func() { dang = u.dangChay })
	}
	var n1 int
	lam(func() { n1 = u.soLanVe })
	in(fmt.Sprintf("Hoạt hình chạy xong sau %.1f s thật, vẽ lại %d lần (giao diện không đứng)",
		time.Since(t0).Seconds(), n1-n0))
	in("Bấm 'So sánh 4 chế độ', tua tới cuối")
	lam(func() {
		u.chon(L.DanhSachCheDo)
		u.dangChay = false
		u.datNutChay()
		u.T = float64(u.doDai())
		u.ve()
	})
	cho(500)
	anh2 := chup()
	lam(func() {
		for _, x := range u.ketQua {
			s := L.TinhChiSo(x.kq)
			uc := "–"
			if s.UCho >= 0 {
				uc = L.Giay(s.UCho) + " s"
			}
			in(fmt.Sprintf("  %s hoàn tất %s s, luồng %d, U chờ %s, chen ngang %d",
				L.Trai(L.TenNgan[x.cheDo], 14), L.Giay(s.Tong), s.Luong, uc, s.Chen))
		}
	})
	in("Bỏ việc khẩn, bấm 'Đa luồng – đồng bộ', dừng ở 3.3 s rồi bấm 'Chen ngang U ngay'")
	lam(func() {
		u.chkKhan.SetChecked(false)
		u.chon([]string{"da_dong_bo"})
		u.dangChay = false
		u.datNutChay()
		u.T = 33
		u.chenNgang()
		s := L.TinhChiSo(u.ketQua[0].kq)
		in(fmt.Sprintf("  U đến 3.3 s, chờ %s s, chen ngang %d lần, hoàn tất %s s", L.Giay(s.UCho), s.Chen, L.Giay(s.Tong)))
	})
	if err := ghep("ket_qua_gui.png", anh1, anh2); err != nil {
		in("Lỗi ghi ảnh: " + err.Error())
	} else {
		in("Đã chụp cửa sổ: ket_qua_gui.png")
	}
	lam(u.app.Quit)
}

// ghep – mỗi ảnh có viền 4 px màu #d0d4dc, xếp chồng theo chiều dọc (như convert -border 4 -append).
func ghep(tep string, ds ...image.Image) error {
	const b = 4
	W, H := 0, 0
	for _, a := range ds {
		W = max(W, a.Bounds().Dx()+2*b)
		H += a.Bounds().Dy() + 2*b
	}
	out := image.NewRGBA(image.Rect(0, 0, W, H))
	draw.Draw(out, out.Bounds(), &image.Uniform{color.NRGBA{0xd0, 0xd4, 0xdc, 0xff}}, image.Point{}, draw.Src)
	y := 0
	for _, a := range ds {
		r := a.Bounds()
		draw.Draw(out, image.Rect(b, y+b, b+r.Dx(), y+b+r.Dy()), a, r.Min, draw.Src)
		y += r.Dy() + 2*b
	}
	f, err := os.Create(tep)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, out)
}

// chupCuaSo – ảnh nội dung cửa sổ. Trên Windows/macOS (có GPU) dùng Canvas().Capture() của Fyne. Dưới Xvfb
// (Linux, OpenGL phần mềm) Capture() trả về ảnh trắng, nên chụp màn hình bằng ImageMagick
// "import -window root" rồi cắt đúng kích thước cửa sổ (cửa sổ nằm ở góc (0,0) vì không có trình quản lý
// cửa sổ).
func chupCuaSo(w fyne.Window) image.Image {
	_, loi := exec.LookPath("import")
	if runtime.GOOS != "linux" || os.Getenv("DISPLAY") == "" || loi != nil {
		return w.Canvas().Capture()
	}
	tam := filepath.Join(os.TempDir(), fmt.Sprintf("mr56_%d.png", time.Now().UnixNano()))
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
