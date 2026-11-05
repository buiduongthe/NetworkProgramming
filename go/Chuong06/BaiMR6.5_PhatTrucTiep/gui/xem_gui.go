package main

// Cửa sổ "Người xem trực tiếp": kết nối; ô hình phóng ×2 kèm nhãn hình và đồng hồ tiếng lúc hiện;
// thanh mức tiếng, ô Loa (ffplay nếu có thiết bị phát, không thì "chỉ đo mức"); công tắc Đồng bộ
// tiếng-hình; chọn nhận Âm thanh / Video / Cả hai; độ sâu bộ đệm jitter; Chụp (out/<tên>_gui_chup_N.png);
// thống kê; nhật ký.
//
// Luồng: lõi tructiep.NguoiXem (goroutine TCP, UDP, phát 20 ms = đồng hồ chủ, hiển thị 5 ms) ->
// goroutine hẹn giờ: 50 ms vẽ khung đang hiện, 200 ms cập nhật thanh mức và thống kê (fyne.Do).

import (
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"

	tt "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.5_PhatTrucTiep/tructiep"
)

var nhanNhan = map[string]string{"am_thanh": "Âm thanh", "video": "Video", "ca_hai": "Cả hai"}

type cuaSoXem struct {
	w             fyne.Window
	lop           *tt.NguoiXem
	ip, cong, ten *widget.Entry
	ketNoi        *widget.Button
	anh           *canvas.Image
	cho           *canvas.Text
	nhanKhung     *widget.Label
	thanh         *thanhDo
	oLoa          *widget.Check
	nhanLoa       *widget.Label
	loa           *tt.Loa
	dongBo        *widget.Check
	nhan          *widget.RadioGroup
	doSau         *widget.Select
	chupNut       *widget.Button
	nhanTK        map[string]*widget.Label
	trangThai     *widget.Label
	nhanTt        string
	log           *nhatKy
	dinhKy        *thongKeDinhKy
	soDaVe        int
	soChup        int
	dong          chan struct{}
}

func moCuaSoXem(a fyne.App, ten, host string, cong int) *cuaSoXem {
	x := &cuaSoXem{w: a.NewWindow("Người xem trực tiếp – MR6.5 (" + ten + ")"), log: moiNhatKy("xem " + ten),
		dinhKy: moiThongKeDinhKy("xem " + ten), nhanTK: map[string]*widget.Label{}, nhanTt: "chưa kết nối",
		soDaVe: -1, dong: make(chan struct{})}
	x.ip, x.cong, x.ten = oNhap(host), oNhap(fmt.Sprint(cong)), oNhap(ten)
	x.ketNoi = nut("Kết nối", widget.SuccessImportance, x.bamKetNoi)
	x.anh, x.cho = khungAnh(2*tt.Rong, 2*tt.Cao), chuCho("Chưa kết nối")
	x.nhanKhung = widget.NewLabelWithStyle("Khung: –", fyne.TextAlignLeading, fyne.TextStyle{Monospace: true})
	x.thanh = moiThanhDo()
	x.oLoa = widget.NewCheck("Loa", func(b bool) { x.apLoa() })
	x.oLoa.Disable()
	x.nhanLoa = nhanCat("Loa: đang thử mở…")
	x.dongBo = widget.NewCheck("Đồng bộ tiếng-hình (tiếng làm đồng hồ chủ)", func(b bool) {
		if x.lop != nil {
			x.lop.DatDongBo(b)
		}
	})
	x.dongBo.SetChecked(true)
	x.nhan = widget.NewRadioGroup([]string{"Âm thanh", "Video", "Cả hai"}, func(string) { x.apDangKy() })
	x.nhan.Horizontal = true
	x.nhan.SetSelected("Cả hai")
	var so []string
	for i := 1; i <= 25; i++ {
		so = append(so, strconv.Itoa(i))
	}
	x.doSau = widget.NewSelect(so, func(string) { x.apDoSau() })
	x.doSau.SetSelected(strconv.Itoa(tt.DoSauMacDinh))
	x.chupNut = nut("Chụp", widget.HighImportance, func() { x.chup() })
	x.trangThai = nhanCat("Trạng thái: chưa kết nối")

	ketNoi := nhom("Kết nối server", container.NewHBox(widget.NewLabel("IP:"), coDinh(x.ip, 130, 34),
		widget.NewLabel("Port:"), coDinh(x.cong, 70, 34), widget.NewLabel("Tên:"), coDinh(x.ten, 80, 34), x.ketNoi))
	hinh := nhom("Hình (phóng ×2)", oAnh(x.anh, x.cho), x.nhanKhung,
		container.NewBorder(nil, nil, widget.NewLabel("Tiếng:"), x.oLoa, x.thanh.bar), x.nhanLoa)
	dieuKhien := nhom("Điều khiển", x.dongBo, container.NewHBox(widget.NewLabel("Nhận:"), x.nhan),
		container.NewHBox(widget.NewLabel("Bộ đệm jitter (khung 20 ms):"), x.doSau, x.chupNut))
	var tk []fyne.CanvasObject
	for _, k := range [][2]string{{"tre", "Trễ đầu-cuối (tiếng)*"}, {"lech", "Lệch tiếng-hình hiện tại"},
		{"lech_tb", "Lệch TB giai đoạn này"}, {"mat", "Tiếng nhận / mất"}, {"khung", "Khung nhận I / P"},
		{"dung", "Đứng hình / PLI gửi"}, {"dau", "Khung đầu sau"}} {
		x.nhanTK[k[0]] = nhanDam("–")
		tk = append(tk, container.New(cotCoDinh{170, 200}, widget.NewLabel(k[1]+":"), x.nhanTK[k[0]]))
	}
	ghiChu := widget.NewLabel("* giả định cùng đồng hồ (cùng máy / NTP); lệch +: hình chậm hơn tiếng")
	ghiChu.TextStyle.Italic = true
	tk = append(tk, ghiChu)
	thongKe := nhom("Thống kê", tk...)
	phai := container.NewVBox(dieuKhien, thongKe)
	nk := nhom("Nhật ký", coDinh(x.log.ds, 740, 110))
	x.w.SetContent(container.NewVBox(ketNoi, container.NewHBox(hinh, coDinh(phai, 400, 440)), nk,
		widget.NewSeparator(), x.trangThai))
	x.w.SetFixedSize(true)
	x.w.SetOnClosed(func() {
		close(x.dong)
		if x.lop != nil {
			go x.lop.Dong()
		}
		if x.loa != nil {
			x.loa.Dong()
		}
	})
	go func() { // mở loa (ffplay) ở nền: chờ tới ~0,8 s để biết có chạy được không
		l, lyDo := tt.MoLoa()
		fyne.Do(func() {
			x.loa = l
			if l != nil {
				x.nhanLoa.SetText("Loa: THẬT – thiết bị mặc định (ffplay)")
				x.nhanLoa.Importance = widget.SuccessImportance
				x.oLoa.Enable()
				x.oLoa.SetChecked(true)
			} else {
				x.nhanLoa.SetText("Loa: " + lyDo)
				x.nhanLoa.Importance = widget.WarningImportance
			}
			x.nhanLoa.Refresh()
		})
	}()
	go x.henGio()
	return x
}

func (x *cuaSoXem) loaChon() tt.LoaRa {
	if x.loa != nil && x.oLoa.Checked {
		return x.loa
	}
	return nil
}

// ------------------------------------------------------------ kết nối

func (x *cuaSoXem) bamKetNoi() {
	if x.lop != nil && x.lop.DaKetNoi() {
		go x.lop.Ngat()
		x.daNgat("đã ngắt kết nối")
		return
	}
	ten, host := strings.TrimSpace(x.ten.Text), strings.TrimSpace(x.ip.Text)
	cong := soNguyen(x.cong.Text, 0)
	if cong == 0 {
		x.nhanTt = "cổng không hợp lệ"
		return
	}
	if x.lop != nil {
		cu := x.lop
		cu.DatLoa(nil) // loa dùng lại cho kết nối mới, không đóng
		go cu.Dong()
	}
	v := maNhan(x.nhan.Selected)
	lop := tt.MoiNguoiXem(ten, x.log.them, x.layDoSau(), x.dongBo.Checked, v != "video", v != "am_thanh", 0, x.loaChon())
	x.lop = lop
	x.soDaVe = -1
	x.ketNoi.Disable()
	x.ketNoi.SetText("Đang kết nối…")
	go func() {
		ma, err := lop.KetNoi(host, cong)
		fyne.Do(func() {
			if err != nil {
				x.daNgat("không kết nối được – " + err.Error())
				return
			}
			x.ketNoi.Enable()
			x.ketNoi.SetText("Ngắt kết nối")
			x.ketNoi.Importance = widget.DangerImportance
			x.ketNoi.Refresh()
			x.nhanTt = fmt.Sprintf("đã kết nối, mã %d", ma)
			x.cho.Text = "Chờ khung hình…"
			x.cho.Refresh()
			x.ip.Disable()
			x.cong.Disable()
			x.ten.Disable()
		})
	}()
}

func (x *cuaSoXem) daNgat(text string) {
	x.ketNoi.Enable()
	x.ketNoi.SetText("Kết nối")
	x.ketNoi.Importance = widget.SuccessImportance
	x.ketNoi.Refresh()
	x.nhanTt = text
	x.ip.Enable()
	x.cong.Enable()
	x.ten.Enable()
}

// ------------------------------------------------------------ điều khiển

func maNhan(chon string) string {
	for k, v := range nhanNhan {
		if v == chon {
			return k
		}
	}
	return "ca_hai"
}

func (x *cuaSoXem) layDoSau() int {
	n, err := strconv.Atoi(x.doSau.Selected)
	if err != nil {
		return tt.DoSauMacDinh
	}
	return max(1, min(25, n))
}

func (x *cuaSoXem) apDangKy() {
	if x.lop != nil {
		v := maNhan(x.nhan.Selected)
		x.lop.DangKy(v != "video", v != "am_thanh")
	}
}

func (x *cuaSoXem) apDoSau() {
	if x.lop != nil {
		x.lop.DatDoSau(x.layDoSau())
		x.log.them(fmt.Sprintf("Bộ đệm jitter: %d khung = %d ms", x.layDoSau(), x.layDoSau()*20))
	}
}

func (x *cuaSoXem) apLoa() {
	if x.lop != nil {
		x.lop.DatLoa(x.loaChon())
	}
}

// chup – lưu khung đang hiện (kèm chú thích) thành out/<tên>_gui_chup_N.png; trả về đường dẫn ("" nếu chưa có).
func (x *cuaSoXem) chup() string {
	var c *tt.AnhChup
	if x.lop != nil {
		c = x.lop.Chup()
	}
	if c == nil {
		x.log.them("Chưa có hình để chụp")
		return ""
	}
	w, h, rgb := tt.AnhChuThich(c)
	os.MkdirAll("out", 0o755)
	x.soChup++
	duong := fmt.Sprintf("out/%s_gui_chup_%d.png", x.lop.Ten, x.soChup)
	tt.GhiPNG(duong, rgb, w, h)
	x.log.them("Đã chụp " + duong)
	return duong
}

// ------------------------------------------------------------ vẽ lại

func (x *cuaSoXem) henGio() {
	t := time.NewTicker(50 * time.Millisecond)
	defer t.Stop()
	dem := 0
	for {
		select {
		case <-x.dong:
			return
		case <-t.C:
		}
		dem++
		var lop *tt.NguoiXem
		fyne.DoAndWait(func() { lop = x.lop })
		if lop == nil {
			continue
		}
		hien, so := lop.Hien()
		var tr *tt.TrangThaiXem
		if dem%4 == 0 {
			t := lop.TrangThai()
			tr = &t
		}
		fyne.Do(func() { x.ve(lop, hien, so, tr) })
	}
}

func (x *cuaSoXem) ve(lop *tt.NguoiXem, hien *tt.KhungHien, so int, t *tt.TrangThaiXem) {
	if x.lop != lop {
		return
	}
	if hien != nil && so != x.soDaVe {
		x.anh.Image = tt.AnhNRGBA(hien.Rgb, hien.W, hien.H)
		x.anh.Refresh()
		x.cho.Text = ""
		x.cho.Refresh()
		tieng := "–"
		if hien.DongHo != nil {
			tieng = fmt.Sprintf("%.3f s", *hien.DongHo/1000)
		}
		x.nhanKhung.SetText(fmt.Sprintf("Khung #%d  hình t=%.3f s  tiếng t=%s", hien.So, float64(hien.Ts)/1000, tieng))
		x.soDaVe = so
	} else if hien == nil && x.soDaVe != -2 {
		x.anh.Image = tt.AnhNRGBA(nil, 4, 3)
		x.anh.Refresh()
		x.soDaVe = -2
	}
	if t == nil {
		return
	}
	x.thanh.datMuc(t.MucAm)
	if t.DaKetNoi {
		x.dinhKy.inNeuDenHan(func() string {
			loa, dongBo := "tắt", "TẮT"
			if x.loaChon() != nil {
				loa = "BẬT"
			}
			if t.DongBo {
				dongBo = "BẬT"
			}
			return fmt.Sprintf("tiếng %d gói, mất %d, mức %.0f dBFS, loa %s | hình I %d P %d, đứng %d, lệch %s, trễ %s | đồng bộ %s",
				t.GoiAm, t.MatAm, t.MucAm, loa, t.KhungI, t.KhungP, t.DungHinh, chuMs(t.LechHien, true),
				chuMs(t.TreHien, false), dongBo)
		})
	}
	g := t.GiaiDoan[len(t.GiaiDoan)-1]
	x.nhanTK["tre"].SetText(chuMs(t.TreHien, false))
	x.nhanTK["lech"].SetText(chuMs(t.LechHien, true))
	if t.LechHien != nil && math.Abs(*t.LechHien) <= 40 {
		x.nhanTK["lech"].Importance = widget.SuccessImportance
	} else {
		x.nhanTK["lech"].Importance = widget.DangerImportance
	}
	x.nhanTK["lech"].Refresh()
	tb := "–"
	if g.N > 0 {
		v := g.Tong / float64(g.N)
		tb = chuMs(&v, true)
	}
	che := "TẮT"
	if g.DongBo {
		che = "BẬT"
	}
	x.nhanTK["lech_tb"].SetText(tb + " (đồng bộ " + che + ")")
	x.nhanTK["mat"].SetText(fmt.Sprintf("%d / %d gói", t.GoiAm, t.MatAm))
	x.nhanTK["khung"].SetText(fmt.Sprintf("%d / %d (hỏng %d)", t.KhungI, t.KhungP, t.Hong))
	x.nhanTK["dung"].SetText(fmt.Sprintf("%d / %d", t.DungHinh, t.PliGui))
	dau := "–"
	if t.KhungDauMs != nil {
		dau = fmt.Sprintf("%.0f ms (%s)", *t.KhungDauMs, t.NguonDau)
	}
	x.nhanTK["dau"].SetText(dau)
	if strings.HasPrefix(x.nhanTt, "đã kết nối") && !t.DaKetNoi {
		x.daNgat("mất kết nối server")
	}
	trang := x.nhanTt
	if t.DaKetNoi {
		trang = fmt.Sprintf("đang xem – nhận %.0f kbit/s TB", t.Kbps)
	}
	x.trangThai.SetText("Trạng thái: " + trang)
}
