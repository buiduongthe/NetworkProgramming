package main

// Cửa sổ "Máy phát trực tiếp": kết nối server; chọn nguồn (camera thật qua ffmpeg / tệp video / mẫu thử
// testsrc / giả lập clapper; micro thật qua ffmpeg / tiếng của tệp video / giả lập), FPS, GOP, ô Tệp
// video + "Chọn tệp…" + "Tạo tệp mẫu"; PHÁT / DỪNG PHÁT / Khung khóa ngay; xem trước khung vừa gửi,
// thanh mức micro, thống kê, danh sách người xem; nhật ký. Thiết bị thật chỉ được mở khi bấm PHÁT.
//
// Luồng: lõi tructiep.MayPhat (goroutine TCP, âm thanh 20 ms, video theo fps) -> goroutine hẹn giờ
// 100 ms đọc TrangThai() (và mỗi 1 s ThongKeMoiGiay()) -> fyne.Do -> luồng giao diện vẽ.

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

type nguon struct{ ma, ten string }

const (
	tenTep    = "Tệp video (chọn tệp bên dưới)"
	tenMauThu = "Mẫu thử ffmpeg (testsrc)"
	tenCamGia = "Camera giả lập (bảng clapper)"
	tenMicTep = "Tiếng của tệp video"
	tenMicGia = "Micro giả lập (440 Hz + bíp mỗi giây)"
)

type cuaSoPhat struct {
	w                      fyne.Window
	lop                    *tt.MayPhat
	ip, cong, ten          *widget.Entry
	ketNoi                 *widget.Button
	chonCam, chonMic       *widget.Select
	dsCam, dsMic           []nguon
	micThat                []tt.ThietBi
	fps, gop, tep          *widget.Entry
	chonTep, taoMau        *widget.Button
	phat, dungPhat, khungI *widget.Button
	chiBao                 *widget.Label
	anh                    *canvas.Image
	thanhMic               *thanhDo
	nhanTK                 map[string]*widget.Label
	nhanDs                 *widget.Label
	dsXem                  []string
	listXem                *widget.List
	trangThai              *widget.Label
	nhanTt                 string
	log                    *nhatKy
	dinhKy                 *thongKeDinhKy
	tsDaVe                 int64
	dong                   chan struct{}
	henThamSo              *time.Timer // hẹn giờ áp dụng FPS/GOP (chờ người dùng ngừng bấm)
}

func moCuaSoPhat(a fyne.App, ten, host string, cong int) *cuaSoPhat {
	p := &cuaSoPhat{w: a.NewWindow("Máy phát trực tiếp – MR6.5 (" + ten + ")"), log: moiNhatKy("phat " + ten),
		dinhKy: moiThongKeDinhKy("phat " + ten), nhanTK: map[string]*widget.Label{}, nhanTt: "chưa kết nối",
		tsDaVe: -1, dong: make(chan struct{})}
	p.ip, p.cong, p.ten = oNhap(host), oNhap(fmt.Sprint(cong)), oNhap(ten)
	p.ketNoi = nut("Kết nối", widget.SuccessImportance, p.bamKetNoi)
	coFF := tt.CoFfmpeg()
	if coFF {
		p.dsCam = []nguon{{tt.Tep, tenTep}, {tt.MauThu, tenMauThu}}
		p.dsMic = []nguon{{tt.Tep, tenMicTep}}
	}
	p.dsCam = append(p.dsCam, nguon{tt.GiaLap, tenCamGia})
	p.dsMic = append(p.dsMic, nguon{tt.GiaLap, tenMicGia})
	p.chonCam = widget.NewSelect(tenNguon(p.dsCam), func(string) { p.doiCamera() })
	p.chonCam.SetSelected(tenCamGia)
	p.chonMic = widget.NewSelect(tenNguon(p.dsMic), nil)
	p.chonMic.SetSelected(tenMicGia)
	p.fps, p.gop = oNhap(fmt.Sprint(tt.FpsMacDinh)), oNhap(fmt.Sprint(tt.GopMacDinh))
	p.fps.OnSubmitted = func(string) { p.apThamSo() }
	p.gop.OnSubmitted = func(string) { p.apThamSo() }
	p.tep = oNhap("")
	p.tep.SetPlaceHolder("đường dẫn tệp mp4, mkv, avi…")
	p.chonTep = nut("Chọn tệp…", widget.MediumImportance, p.moHopChonTep)
	p.taoMau = nut("Tạo tệp mẫu", widget.MediumImportance, p.lamTaoMau)
	if !coFF {
		p.tep.Disable()
		p.chonTep.Disable()
		p.taoMau.Disable()
	}
	p.phat = nut("PHÁT", widget.SuccessImportance, p.batPhat)
	p.dungPhat = nut("DỪNG PHÁT", widget.DangerImportance, p.lamDungPhat)
	p.khungI = nut("Khung khóa ngay", widget.HighImportance, func() {
		if p.lop != nil {
			p.lop.KhungKhoaNgay("nút Khung khóa ngay")
		}
	})
	for _, b := range []*widget.Button{p.phat, p.dungPhat, p.khungI} {
		b.Disable()
	}
	chi := "Camera: tắt | Micro: tắt"
	if !coFF {
		chi += " (không có ffmpeg)"
	}
	p.chiBao = nhanCat(chi)
	p.anh = khungAnh(2*tt.Rong, 2*tt.Cao)
	p.thanhMic = moiThanhDo()
	p.listXem = widget.NewList(func() int { return len(p.dsXem) },
		func() fyne.CanvasObject { return widget.NewLabel("") },
		func(i widget.ListItemID, o fyne.CanvasObject) { o.(*widget.Label).SetText(p.dsXem[i]) })
	p.nhanDs = widget.NewLabel("Người xem (0):")
	p.trangThai = nhanCat("Trạng thái: chưa kết nối")

	ketNoi := nhom("Kết nối server", container.New(layout.NewFormLayout(),
		widget.NewLabel("Server IP:"), p.ip, widget.NewLabel("Port:"), p.cong, widget.NewLabel("Tên:"), p.ten),
		p.ketNoi)
	thamSo := container.New(layout.NewFormLayout(),
		widget.NewLabel("Camera:"), p.chonCam, widget.NewLabel("Micro:"), p.chonMic,
		widget.NewLabel("FPS:"), container.NewGridWithColumns(3, p.fps, widget.NewLabel("GOP (khung):"), p.gop),
		widget.NewLabel("Tệp video:"), container.NewBorder(nil, nil, nil, container.NewHBox(p.chonTep, p.taoMau), p.tep))
	nguonTS := nhom("Nguồn và tham số", thamSo, container.NewHBox(p.phat, p.dungPhat, p.khungI), p.chiBao)
	tren := container.NewHBox(coDinh(ketNoi, 230, 250), coDinh(nguonTS, 560, 250))
	xemTruoc := nhom("Xem trước (khung vừa gửi)", oAnh(p.anh, chuCho("")),
		container.NewBorder(nil, nil, widget.NewLabel("Mức micro:"), nil, p.thanhMic.bar))
	var tk []fyne.CanvasObject
	for _, k := range [][2]string{{"ip", "Khung I / P đã gửi"}, {"tb", "Kích thước TB I / P"},
		{"pmin", "P nhỏ nhất / lớn nhất"}, {"kb", "kbit/s tiếng / hình"}, {"pli", "PLI (xin khung khóa) nhận"}} {
		p.nhanTK[k[0]] = nhanDam("–")
		tk = append(tk, container.New(cotCoDinh{200, 230}, widget.NewLabel(k[1]+":"), p.nhanTK[k[0]]))
	}
	tk = append(tk, p.nhanDs, coDinh(p.listXem, 420, 120))
	thongKe := nhom("Thống kê", tk...)
	giua := container.NewHBox(xemTruoc, coDinh(thongKe, 450, 420))
	nk := nhom("Nhật ký", coDinh(p.log.ds, 790, 110))
	p.w.SetContent(container.NewVBox(tren, giua, nk, widget.NewSeparator(), p.trangThai))
	p.w.SetFixedSize(true)
	giuVe(p.w, "phat "+ten, p.dong)
	p.w.SetOnClosed(func() {
		close(p.dong)
		if p.henThamSo != nil {
			p.henThamSo.Stop()
		}
		if p.lop != nil {
			go p.lop.Dong()
		}
	})
	go func() { // liệt kê camera/micro thật ở goroutine nền (Windows: ffmpeg mất ~1 s)
		cam, mic := tt.LietKeCamera(), tt.LietKeMicro()
		fyne.Do(func() {
			var c, m []nguon
			for _, tb := range cam {
				c = append(c, nguon{tb.Ma, tb.Ten})
			}
			for _, tb := range mic {
				m = append(m, nguon{tb.Ma, tb.Ten})
			}
			p.dsCam, p.dsMic, p.micThat = append(c, p.dsCam...), append(m, p.dsMic...), mic
			p.chonCam.SetOptions(tenNguon(p.dsCam))
			p.chonMic.SetOptions(tenNguon(p.dsMic))
			// Mặc định: micro thật đầu tiên tìm thấy (nếu ô Micro còn để giả lập và chưa kết nối);
			// máy không có micro -> giữ micro giả lập (mục cuối)
			if len(m) > 0 && p.lop == nil && p.chonMic.Selected == tenMicGia {
				p.chonMic.SetSelected(m[0].ten)
			}
		})
	}()
	go p.henGio()
	return p
}

func tenNguon(ds []nguon) []string {
	var t []string
	for _, n := range ds {
		t = append(t, n.ten)
	}
	return t
}

func maNguon(ds []nguon, ten string) string {
	for _, n := range ds {
		if n.ten == ten {
			return n.ma
		}
	}
	return tt.GiaLap
}

// ------------------------------------------------------------ kết nối

func (p *cuaSoPhat) bamKetNoi() {
	if p.lop != nil && p.lop.DaKetNoi() {
		lop := p.lop
		go lop.Dong()
		p.daNgat("đã ngắt kết nối")
		return
	}
	ten, host := strings.TrimSpace(p.ten.Text), strings.TrimSpace(p.ip.Text)
	cong := soNguyen(p.cong.Text, 0)
	if cong == 0 {
		p.nhanTt = "cổng không hợp lệ"
		return
	}
	if p.lop != nil {
		go p.lop.Dong()
	}
	lop := tt.MoiMayPhat(ten, p.log.them, 0)
	p.lop = lop
	p.ketNoi.Disable()
	p.ketNoi.SetText("Đang kết nối…")
	go func() { // connect có thể chờ tới 5 s
		ma, err := lop.KetNoi(host, cong)
		fyne.Do(func() {
			if err != nil {
				p.daNgat("không kết nối được – " + err.Error())
				return
			}
			p.ketNoi.Enable()
			p.ketNoi.SetText("Ngắt kết nối")
			p.ketNoi.Importance = widget.DangerImportance
			p.ketNoi.Refresh()
			p.nhanTt = fmt.Sprintf("đã kết nối, mã %d", ma)
			p.phat.Enable()
			p.ip.Disable()
			p.cong.Disable()
			p.ten.Disable()
		})
	}()
}

func (p *cuaSoPhat) daNgat(text string) {
	p.ketNoi.Enable()
	p.ketNoi.SetText("Kết nối")
	p.ketNoi.Importance = widget.SuccessImportance
	p.ketNoi.Refresh()
	p.nhanTt = text
	for _, b := range []*widget.Button{p.phat, p.dungPhat, p.khungI} {
		b.Disable()
	}
	p.chonCam.Enable()
	p.chonMic.Enable()
	p.ip.Enable()
	p.cong.Enable()
	p.ten.Enable()
}

// ------------------------------------------------------------ phát

func (p *cuaSoPhat) so(e *widget.Entry, macDinh, toiDa int) int {
	return min(toiDa, soNguyen(e.Text, macDinh))
}

func (p *cuaSoPhat) batPhat() {
	lop := p.lop
	if lop == nil || !lop.DaKetNoi() {
		return
	}
	maCam, tenCam := maNguon(p.dsCam, p.chonCam.Selected), p.chonCam.Selected
	maMic := maNguon(p.dsMic, p.chonMic.Selected)
	fps, gop := p.so(p.fps, tt.FpsMacDinh, 30), p.so(p.gop, tt.GopMacDinh, 300)
	duong := strings.TrimSpace(p.tep.Text)
	var tbMic *tt.ThietBi
	for i := range p.micThat {
		if p.micThat[i].Ma == maMic {
			tbMic = &p.micThat[i]
		}
	}
	p.phat.Disable()
	p.chonCam.Disable()
	p.chonMic.Disable()
	go func() { // mở ffmpeg/thiết bị ở goroutine nền
		var cam tt.Camera
		var mic tt.Micro
		if maCam == tt.Tep { // tệp video: hình (và tiếng nếu chọn)
			c, m, err := tt.MoTep(duong, fps, maMic == tt.Tep)
			if err != nil {
				p.log.them("Không mở được tệp video (" + err.Error() + "); dùng camera giả lập")
			} else {
				cam = c
				if m != nil {
					mic = m
				}
				chu := "Phát tệp " + c.Tep.Ten
				if m != nil {
					chu += " (cả tiếng)"
				}
				p.log.them(chu)
			}
		}
		if cam == nil {
			if maCam == tt.Tep {
				maCam = tt.GiaLap
			}
			cam = tt.MoCamera(maCam, tenCam, lop.Ten, max(1, int(lop.Ma)), fps)
		}
		if mic == nil {
			if maMic == tt.Tep {
				p.log.them("Tiếng của tệp chỉ dùng được khi camera là tệp video; dùng micro giả lập")
				tbMic = nil
			}
			m, loi := tt.MoMic(tbMic)
			if loi != "" {
				p.log.them(loi)
			}
			mic = m
		}
		lop.BatDauPhat(cam, mic, fps, gop)
		p.log.them("Camera: " + cam.MoTa())
		p.log.them("Micro: " + mic.MoTa())
		fyne.Do(func() {
			if p.lop != lop {
				return
			}
			p.chiBao.SetText("Camera: " + cam.MoTa() + " | Micro: " + mic.MoTa())
			p.chiBao.Importance = widget.WarningImportance
			if cam.That() || mic.That() {
				p.chiBao.Importance = widget.SuccessImportance
			}
			p.chiBao.Refresh()
			p.dungPhat.Enable()
			p.khungI.Enable()
		})
	}()
}

func (p *cuaSoPhat) lamDungPhat() {
	if p.lop != nil {
		lop := p.lop
		go lop.DungPhat()
	}
	p.chiBao.SetText("Camera: tắt | Micro: tắt")
	p.chiBao.Importance = widget.MediumImportance
	p.chiBao.Refresh()
	if p.lop != nil && p.lop.DaKetNoi() {
		p.phat.Enable()
	}
	p.dungPhat.Disable()
	p.khungI.Disable()
	p.chonCam.Enable()
	p.chonMic.Enable()
}

// apThamSo – đổi FPS/GOP liên tục (giữ Enter, gõ sửa nhiều lần): chỉ áp dụng (và gửi thong_tin) khi giá
// trị đứng yên 400 ms – tránh gửi hàng chục thông điệp cho mỗi lần đổi. Hàm hẹn giờ chạy ở goroutine
// riêng nên chuyển về luồng giao diện bằng fyne.Do trước khi đọc ô nhập.
func (p *cuaSoPhat) apThamSo() {
	if p.henThamSo == nil {
		p.henThamSo = time.AfterFunc(400*time.Millisecond, func() { fyne.Do(p.apThamSoThat) })
		return
	}
	p.henThamSo.Reset(400 * time.Millisecond) // đếm lại 400 ms từ lần đổi mới nhất
}

func (p *cuaSoPhat) apThamSoThat() {
	if p.lop != nil && p.lop.DangPhat() {
		p.lop.DatThamSo(p.so(p.fps, tt.FpsMacDinh, 30), p.so(p.gop, tt.GopMacDinh, 300))
	}
}

// doiCamera – chọn camera "Tệp video" -> micro tự chuyển sang tiếng của tệp (đổi lại được); bỏ "Tệp
// video" mà micro còn là tiếng của tệp -> micro về thiết bị đầu tiên trong danh sách (micro thật nếu có).
func (p *cuaSoPhat) doiCamera() {
	if p.chonMic == nil {
		return
	}
	if maNguon(p.dsCam, p.chonCam.Selected) == tt.Tep {
		p.chonMic.SetSelected(tenMicTep)
	} else if p.chonMic.Selected == tenMicTep && len(p.dsMic) > 0 {
		p.chonMic.SetSelected(p.dsMic[0].ten)
	}
}

func (p *cuaSoPhat) moHopChonTep() {
	d := dialog.NewFileOpen(func(r fyne.URIReadCloser, err error) {
		if err != nil || r == nil {
			return
		}
		r.Close()
		p.datTep(r.URI().Path())
	}, p.w)
	d.SetFilter(storage.NewExtensionFileFilter([]string{".mp4", ".mkv", ".avi", ".mov", ".webm", ".flv", ".ts", ".m4v"}))
	d.Resize(fyne.NewSize(700, 480))
	d.Show()
}

// datTep – đặt đường dẫn tệp, chọn nguồn "Tệp video" (micro tự chuyển sang tiếng của tệp).
func (p *cuaSoPhat) datTep(duong string) {
	p.tep.SetText(duong)
	p.chonCam.SetSelected(tenTep)
	p.doiCamera()
}

// lamTaoMau – tạo tệp mẫu 10 s (chớp trắng + bíp mỗi giây) ở out/tep_mau.mp4, rồi chọn nó.
func (p *cuaSoPhat) lamTaoMau() {
	duong, _ := filepath.Abs(filepath.Join("out", "tep_mau.mp4"))
	os.MkdirAll(filepath.Dir(duong), 0o755)
	p.taoMau.Disable()
	p.taoMau.SetText("Đang tạo…")
	go func() {
		err := tt.TaoTepMau(duong, 10)
		fyne.Do(func() {
			p.taoMau.Enable()
			p.taoMau.SetText("Tạo tệp mẫu")
			if err != nil {
				p.log.them("Không tạo được tệp mẫu: " + err.Error())
				return
			}
			p.log.them("Đã tạo tệp mẫu " + duong)
			p.datTep(duong)
		})
	}()
}

// ------------------------------------------------------------ vẽ lại

func (p *cuaSoPhat) henGio() {
	t := time.NewTicker(100 * time.Millisecond)
	defer t.Stop()
	dem := 0
	for {
		select {
		case <-p.dong:
			return
		case <-t.C:
		}
		dem++
		var lop *tt.MayPhat
		fyne.DoAndWait(func() { lop = p.lop })
		if lop == nil {
			continue
		}
		if dem%10 == 0 {
			lop.ThongKeMoiGiay() // cập nhật kbit/s của giây vừa qua
		}
		tr := lop.TrangThai()
		fyne.Do(func() { p.ve(lop, tr) })
	}
}

func (p *cuaSoPhat) ve(lop *tt.MayPhat, t tt.TrangThaiPhat) {
	if p.lop != lop {
		return
	}
	p.thanhMic.datMuc(t.MucMic)
	if t.DangPhat {
		p.dinhKy.inNeuDenHan(func() string {
			return fmt.Sprintf("I %d P %d | hình %.0f tiếng %.0f kbit/s | mic %.0f dBFS | PLI %d | %d người xem",
				t.SoI, t.SoP, t.KbpsHinh, t.KbpsAm, t.MucMic, t.PliNhan, len(t.NguoiXem))
		})
	}
	if t.XemTruoc != nil && t.TsXemTruoc != p.tsDaVe && t.DangPhat {
		p.anh.Image = tt.AnhNRGBA(t.XemTruoc, tt.Rong, tt.Cao)
		p.anh.Refresh()
		p.tsDaVe = t.TsXemTruoc
	} else if !t.DangPhat && p.tsDaVe != -1 {
		p.anh.Image = tt.AnhNRGBA(nil, 4, 3)
		p.anh.Refresh()
		p.tsDaVe = -1
	}
	p.nhanTK["ip"].SetText(fmt.Sprintf("%d / %d", t.SoI, t.SoP))
	p.nhanTK["tb"].SetText(fmt.Sprintf("%.0f B / %.0f B", t.TbI, t.TbP))
	p.nhanTK["pmin"].SetText(fmt.Sprintf("%d B / %d B", t.PMin, t.PMax))
	p.nhanTK["kb"].SetText(fmt.Sprintf("%.0f / %.0f", t.KbpsAm, t.KbpsHinh))
	p.nhanTK["pli"].SetText(fmt.Sprint(t.PliNhan))
	if strings.Join(p.dsXem, "\n") != strings.Join(t.NguoiXem, "\n") {
		p.dsXem = t.NguoiXem
		p.listXem.Refresh()
	}
	p.nhanDs.SetText(fmt.Sprintf("Người xem (%d):", len(t.NguoiXem)))
	if strings.HasPrefix(p.nhanTt, "đã kết nối") && !t.DaKetNoi {
		p.daNgat("mất kết nối server")
	}
	trang := p.nhanTt
	if t.DangPhat {
		trang = fmt.Sprintf("ĐANG PHÁT %d fps, GOP %d – %d người xem", t.Fps, t.Gop, len(t.NguoiXem))
	}
	p.trangThai.SetText("Trạng thái: " + trang)
}
