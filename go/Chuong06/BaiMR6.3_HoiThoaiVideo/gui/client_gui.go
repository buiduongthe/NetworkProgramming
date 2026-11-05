package main

// Cửa sổ "Camera Client": kết nối server; camera (chọn camera thật hoặc "Camera giả lập", START =
// tự bật, CANCEL = tự tắt); xem trước camera của mình; lưới nhận từ server (chọn fps nhận, độ phân
// giải nhận 1/1, 1/2); thanh trạng thái (đang truyền, fps gửi, kbit/s).
//
// Luồng: lõi video.Khach (goroutine TCP, UDP nhận + ghép mảnh, vòng gửi theo fps) -> goroutine hẹn
// giờ 100 ms đọc TrangThai() -> fyne.Do -> luồng giao diện vẽ ảnh và chữ.

import (
	"fmt"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	vd "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.3_HoiThoaiVideo/video"
)

const tenCameraGia = "Camera giả lập"

type cuaSoClient struct {
	w                     fyne.Window
	khach                 *vd.Khach
	ip, cong, ten         *widget.Entry
	ketNoi, ngat          *widget.Button
	chonCam               *widget.Select
	batCam, tatCam        *widget.Button
	nguon                 *widget.Label
	xemTruoc, luoi        *canvas.Image
	nhanXem, nhanLuoi     *widget.Label
	fpsNhan, phanGiaiNhan *widget.Select
	trangThai             *widget.Label
	thietBi               []vd.ThietBi
	cam                   *vd.NguonCamera
	log                   *nhatKy
	dungHenGio            chan struct{}
	TrangThaiCuoi         vd.TrangThaiKhach
}

func moCuaSoClient(a fyne.App, ten, host string, cong int) *cuaSoClient {
	c := &cuaSoClient{w: a.NewWindow("Camera Client – " + ten), log: moiNhatKy()}
	c.ip, c.cong, c.ten = oNhap(host), oNhap(fmt.Sprint(cong)), oNhap(ten)
	c.ketNoi = nut("Kết nối", widget.SuccessImportance, c.lamKetNoi)
	c.ngat = nut("Ngắt kết nối", widget.DangerImportance, c.lamNgat)
	c.chonCam = widget.NewSelect([]string{tenCameraGia}, nil)
	c.chonCam.SetSelected(tenCameraGia)
	c.batCam = nut("START", widget.SuccessImportance, c.lamBatCam)
	c.tatCam = nut("CANCEL", widget.DangerImportance, c.lamTatCam)
	c.nguon = nhanCat("Camera: tắt")
	c.xemTruoc, c.luoi = khungAnh(240, 180), khungAnh(240, 180)
	c.nhanXem, c.nhanLuoi = nhanCat("Camera của tôi (xem trước)"), nhanCat("Lưới nhận từ server")
	c.fpsNhan = widget.NewSelect(dsFps, func(string) { c.guiNhan() })
	c.phanGiaiNhan = widget.NewSelect([]string{"1/1", "1/2"}, func(string) { c.guiNhan() })
	c.fpsNhan.SetSelected("10")
	c.phanGiaiNhan.SetSelected("1/1")
	c.trangThai = nhanCat("Trạng thái: chưa kết nối")
	c.datDieuKhien(false)

	ketNoi := nhom("Kết nối server", container.NewGridWithColumns(2,
		widget.NewLabel("Server IP:"), c.ip, widget.NewLabel("Cổng:"), c.cong, widget.NewLabel("Tên:"), c.ten,
		c.ketNoi, c.ngat))
	camera := nhom("Camera", widget.NewLabel("Chọn camera:"), c.chonCam,
		container.NewGridWithColumns(2, c.batCam, c.tatCam), c.nguon)
	anh := container.NewGridWithColumns(2,
		container.NewBorder(c.nhanXem, nil, nil, nil, c.xemTruoc),
		container.NewBorder(c.nhanLuoi, container.NewGridWithColumns(2,
			widget.NewLabel("fps nhận:"), c.fpsNhan, widget.NewLabel("Độ phân giải:"), c.phanGiaiNhan), nil, nil, c.luoi))
	c.w.SetContent(container.NewBorder(container.NewGridWithColumns(2, ketNoi, camera),
		container.NewVBox(widget.NewSeparator(), c.trangThai), nil, nil, anh))
	c.w.Resize(fyne.NewSize(600, 560))
	c.w.SetOnClosed(c.lamNgat)

	go func() { // liệt kê camera thật ở goroutine nền
		ds := vd.LietKeCamera()
		fyne.Do(func() {
			c.thietBi = ds
			tuyChon := []string{tenCameraGia}
			for _, tb := range ds {
				tuyChon = append(tuyChon, tb.Ten)
			}
			c.chonCam.SetOptions(tuyChon)
		})
	}()
	return c
}

func (c *cuaSoClient) datDieuKhien(daKetNoi bool) {
	if daKetNoi {
		c.ketNoi.Disable()
		c.ngat.Enable()
		c.batCam.Enable()
		c.tatCam.Disable()
		c.fpsNhan.Enable()
		c.phanGiaiNhan.Enable()
	} else {
		c.ketNoi.Enable()
		c.ngat.Disable()
		c.batCam.Disable()
		c.tatCam.Disable()
		c.fpsNhan.Disable()
		c.phanGiaiNhan.Disable()
	}
}

func (c *cuaSoClient) guiNhan() {
	if c.khach == nil {
		return
	}
	n, _ := strconv.Atoi(c.fpsNhan.Selected)
	chia := 1
	if c.phanGiaiNhan.Selected == "1/2" {
		chia = 2
	}
	c.khach.DatNhan(max(1, n), chia)
}

func (c *cuaSoClient) lamKetNoi() {
	ten, host, cong := c.ten.Text, c.ip.Text, soNguyen(c.cong.Text, 41635)
	c.ketNoi.Disable()
	c.trangThai.SetText("Trạng thái: đang kết nối…")
	go func() {
		k, err := vd.KetNoi(ten, host, cong, c.log.them)
		fyne.Do(func() {
			if err != nil {
				c.trangThai.SetText("Trạng thái: không kết nối được – " + err.Error())
				c.ketNoi.Enable()
				return
			}
			c.khach = k
			c.w.SetTitle(fmt.Sprintf("Camera Client – %s (#%d)", ten, k.Ma))
			c.trangThai.SetText(fmt.Sprintf("Trạng thái: đã kết nối %s:%d, mã %d", host, cong, k.Ma))
			c.datDieuKhien(true)
			if c.fpsNhan.Selected != "10" || c.phanGiaiNhan.Selected != "1/1" {
				c.guiNhan()
			}
			c.dungHenGio = make(chan struct{})
			go c.henGio(k, c.dungHenGio)
		})
	}()
}

func (c *cuaSoClient) lamNgat() {
	if c.khach == nil {
		return
	}
	c.lamTatCam()
	close(c.dungHenGio)
	k := c.khach
	c.khach = nil
	k.Ngat()
	c.trangThai.SetText("Trạng thái: đã ngắt kết nối")
	c.datDieuKhien(false)
}

// lamBatCam – START: chỉ lúc này mới mở camera thật (nếu chọn thiết bị thật).
func (c *cuaSoClient) lamBatCam() {
	var tb *vd.ThietBi
	for i := range c.thietBi {
		if c.thietBi[i].Ten == c.chonCam.Selected {
			tb = &c.thietBi[i]
		}
	}
	k := c.khach
	c.batCam.Disable()
	go func() {
		n := vd.MoNguonCamera(tb, k.Ten, int(k.Ma))
		fyne.Do(func() {
			if c.khach != k {
				n.Dong()
				return
			}
			c.cam = n
			k.BatCamera(n)
			c.tatCam.Enable()
		})
	}()
}

func (c *cuaSoClient) lamTatCam() {
	if c.cam == nil {
		return
	}
	if c.khach != nil {
		c.khach.TatCamera()
		c.batCam.Enable()
	}
	c.cam.Dong()
	c.cam = nil
	c.tatCam.Disable()
	c.nguon.SetText("Camera: tắt")
	c.xemTruoc.Image = anhRGB(nil, 4, 3)
	c.xemTruoc.Refresh()
}

func (c *cuaSoClient) henGio(k *vd.Khach, dung chan struct{}) {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	daXem, daLuoi := 0, 0
	for {
		select {
		case <-dung:
			return
		case <-t.C:
		}
		tt := k.TrangThai(daXem, daLuoi)
		daXem, daLuoi = tt.SoXemTruoc, tt.SoLuoi
		fyne.Do(func() { c.ve(k, tt) })
		if tt.DaNgat {
			return
		}
	}
}

func (c *cuaSoClient) ve(k *vd.Khach, t vd.TrangThaiKhach) {
	if c.khach != k {
		return
	}
	c.TrangThaiCuoi = t
	if t.DaNgat {
		c.lamTatCam()
		c.khach = nil
		c.trangThai.SetText("Trạng thái: mất kết nối – " + t.LyDoNgat)
		c.datDieuKhien(false)
		return
	}
	if t.XemTruoc != nil {
		c.xemTruoc.Image = anhRGB(t.XemTruoc, vd.Rong, vd.Cao)
		c.xemTruoc.Refresh()
	}
	if t.Luoi != nil {
		c.luoi.Image = anhRGB(t.Luoi, t.LuoiW, t.LuoiH)
		c.luoi.Refresh()
	}
	c.nhanLuoi.SetText(fmt.Sprintf("Lưới nhận từ server: %dx%d, %.0f fps, %.0f kbit/s", t.LuoiW, t.LuoiH, t.FpsNhan, t.KbpsNhan))
	if c.cam != nil {
		chu := "Nguồn: " + c.cam.MoTa()
		if c.cam.GiaLap() {
			chu += "  [GIẢ LẬP]"
		} else {
			chu += "  [THIẾT BỊ THẬT]"
		}
		c.nguon.SetText(chu)
	}
	gioiHan := fmt.Sprintf("server cho ≤ %d fps, 1/%d", t.FpsServer, t.Chia)
	switch {
	case !t.CoNguon:
		c.nhanXem.SetText("Camera của tôi: tắt (bấm START)")
		c.trangThai.SetText(fmt.Sprintf("Trạng thái: đã kết nối, camera tắt · nhận %.0f fps %.0f kbit/s", t.FpsNhan, t.KbpsNhan))
	case t.TuChoi != "":
		c.nhanXem.SetText("Camera của tôi: bị từ chối")
		c.trangThai.SetText("Trạng thái: bị từ chối – " + t.TuChoi)
	case t.CamMo:
		c.nhanXem.SetText("Camera của tôi (gửi " + gioiHan + ")")
		c.trangThai.SetText(fmt.Sprintf("Trạng thái: Đang truyền dữ liệu camera… gửi %.0f fps, %.0f kbit/s · nhận %.0f fps, %.0f kbit/s",
			t.FpsGui, t.KbpsGui, t.FpsNhan, t.KbpsNhan))
	default:
		c.nhanXem.SetText("Camera của tôi: " + t.LyDo)
		c.trangThai.SetText("Trạng thái: camera " + t.LyDo + " – không gửi hình")
	}
}
