// Bài mở rộng MR5.6 – Mô phỏng thực thi tác vụ: ứng dụng giao diện (Fyne v2).
//
// Chọn chế độ (đơn luồng/đa luồng × đồng bộ/bất đồng bộ) hoặc "So sánh 4 chế độ"; biểu đồ Gantt chạy theo
// thời gian: mỗi hàng là một luồng (luồng hệ điều hành, vòng lặp sự kiện hay luồng thợ), màu là tác vụ đang
// dùng CPU; ô nhạt có gạch chéo: luồng bị chặn chờ I/O; ô viền đứt: sẵn sàng nhưng chờ lõi CPU; ô chấm: chờ
// join. Hàng "I/O chờ" (bất đồng bộ) vẽ các lần chờ I/O không giữ luồng nào. Nút "Chen ngang U ngay" đưa việc
// khẩn U vào đúng thời điểm đang xem và mô phỏng lại (lịch sử trước đó không đổi vì bộ lập lịch tất định).
//
// Mọi trạng thái chỉ được đọc/ghi trên luồng giao diện: mô phỏng rất nhanh (vài mili giây) nên chạy thẳng
// trong hàm xử lý nút; hoạt hình do một goroutine đếm nhịp 30 ms gửi về bằng fyne.Do – goroutine này không
// chạm vào dữ liệu nào, nên không cần khóa.
//
// Chạy (trong thư mục gui/):  go run .            ứng dụng giao diện
//
//	go run . --demo     kịch bản tự thao tác, chụp cửa sổ thành ket_qua_gui.png
package main

import (
	"image/color"
	"os"
	"slices"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"

	L "github.com/buiduongthe/NetworkProgramming/go/Chuong05/BaiMR5.6_MoPhongThucThiTacVu/laplich"
)

// màu của từng tác vụ (đậm: dùng CPU; nhạt: chờ I/O) và màu nền, lưới, chữ
var (
	mau  = map[string]color.NRGBA{"T1": hex("#2457c5"), "T2": hex("#e07b00"), "T3": hex("#1b9e5a"), "U": hex("#d1344b")}
	nhat = map[string]color.NRGBA{"T1": hex("#c9d8f6"), "T2": hex("#fbdcb8"), "T3": hex("#c3ead3"), "U": hex("#f6c5cc")}
	nen  = hex("#ffffff")
	luoi = hex("#e4e7ec")
	chu  = hex("#1d2330")
)

func hex(s string) color.NRGBA {
	v, _ := strconv.ParseUint(strings.TrimPrefix(s, "#"), 16, 32)
	return color.NRGBA{uint8(v >> 16), uint8(v >> 8), uint8(v), 0xff}
}

// giaoDienSang – luôn dùng biến thể sáng của chủ đề mặc định cho hợp với nền trắng của biểu đồ.
type giaoDienSang struct{ fyne.Theme }

func (g giaoDienSang) Color(n fyne.ThemeColorName, _ fyne.ThemeVariant) color.Color {
	return g.Theme.Color(n, theme.VariantLight)
}

// Size – chữ và khoảng đệm nhỏ hơn mặc định một chút để cửa sổ 1280 điểm chứa đủ mọi điều khiển.
func (g giaoDienSang) Size(n fyne.ThemeSizeName) float32 {
	switch n {
	case theme.SizeNameText:
		return 13
	case theme.SizeNamePadding:
		return 3
	case theme.SizeNameInnerPadding:
		return 6
	}
	return g.Theme.Size(n)
}

type ketQuaCheDo struct {
	cheDo string
	kq    *L.KetQua
}

type ungDung struct {
	app      fyne.App
	w        fyne.Window
	ketQua   []ketQuaCheDo // các chế độ đang hiển thị
	T        float64       // thời điểm đang xem (nhịp, số thực để hoạt hình mượt)
	dangChay bool
	last     time.Time
	soLanVe  int
	datThanh bool // đang tự đặt thanh kéo (không coi là người dùng kéo)

	e               map[string]*widget.Entry // t1, t2, t3, lat, t_khan
	selLoi, selToc  *widget.Select
	chkPhu, chkChia *widget.Check
	chkKhan         *widget.Check
	btnChay         *widget.Button
	lblGio          *canvas.Text
	bd              *banDo
	thanh           *widget.Slider
	bang            *fyne.Container
	nk              *widget.Label
	nkCuon          *container.Scroll
	nkCu            string
}

func khung(tieuDe string, noiDung fyne.CanvasObject) fyne.CanvasObject {
	vien := canvas.NewRectangle(color.Transparent)
	vien.StrokeColor = hex("#c9ced8")
	vien.StrokeWidth = 1
	vien.CornerRadius = 4
	td := canvas.NewText(tieuDe, chu)
	td.TextStyle = fyne.TextStyle{Bold: true}
	return container.NewBorder(td, nil, nil, nil, container.NewStack(vien, container.NewPadded(noiDung)))
}

func coDinh(w float32, o fyne.CanvasObject) fyne.CanvasObject {
	return container.NewGridWrap(fyne.NewSize(w, o.MinSize().Height), o)
}

func moi(a fyne.App) *ungDung {
	u := &ungDung{app: a, e: map[string]*widget.Entry{}}
	u.w = a.NewWindow("Mô phỏng thực thi tác vụ – MR5.6")

	// --- điều khiển
	nut := func(ten, m string) *widget.Button { return widget.NewButton(ten, func() { u.chon([]string{m}) }) }
	soSanh := widget.NewButton("So sánh 4 chế độ", func() { u.chon(L.DanhSachCheDo) })
	xoa := widget.NewButton("Xóa", u.xoa)
	chen := widget.NewButtonWithIcon("Chen ngang U ngay", theme.MediaSkipNextIcon(), u.chenNgang)
	chen.Importance = widget.WarningImportance
	u.btnChay = widget.NewButtonWithIcon("Dừng", theme.MediaPauseIcon(), u.chayDung)
	dk := container.NewHBox(container.NewGridWithColumns(2,
		nut("Đơn luồng – đồng bộ", "don_dong_bo"), nut("Đa luồng – đồng bộ", "da_dong_bo"),
		nut("Đơn luồng – bất đồng bộ", "don_bat_dong_bo"), nut("Đa luồng – bất đồng bộ", "da_bat_dong_bo")),
		container.NewGridWithColumns(2, soSanh, chen, xoa, u.btnChay))

	// --- thông số
	o := func(k, mac string) fyne.CanvasObject {
		e := widget.NewEntry()
		e.SetText(mac)
		u.e[k] = e
		return coDinh(56, e)
	}
	nhan := func(s string) fyne.CanvasObject { return widget.NewLabel(s) }
	u.selLoi = widget.NewSelect([]string{"1", "2", "3", "4"}, nil)
	u.selLoi.SetSelected("2")
	u.selToc = widget.NewSelect([]string{"× 0.25", "× 0.5", "× 1", "× 2", "× 4"}, nil)
	u.selToc.SetSelected("× 1")
	u.chkPhu = widget.NewCheck("T3 chờ T1, T2 (join/await)", nil)
	u.chkChia = widget.NewCheck("Chia CPU cuối T1 trên 2 luồng", nil)
	u.chkKhan = widget.NewCheck("Việc khẩn U lúc (s)", nil)
	for _, c := range []*widget.Check{u.chkPhu, u.chkChia, u.chkKhan} {
		c.SetChecked(true)
	}
	cot1 := container.New(layout.NewFormLayout(), nhan("T1 (s)"), o("t1", "4"), nhan("T2 (s)"), o("t2", "3"),
		nhan("T3 (s)"), o("t3", "2"))
	cot2 := container.New(layout.NewFormLayout(), nhan("Số lõi CPU"), coDinh(90, u.selLoi),
		nhan("Lát thời gian (s)"), o("lat", "0.5"), nhan("Tốc độ"), coDinh(90, u.selToc))
	cot3 := container.NewVBox(u.chkPhu, u.chkChia, container.NewHBox(u.chkKhan, o("t_khan", "0.3")))
	ts := container.NewHBox(cot1, cot2, cot3)

	u.lblGio = canvas.NewText("", chu)
	u.lblGio.TextSize = 17
	u.lblGio.TextStyle = fyne.TextStyle{Bold: true}
	tren := container.NewHBox(khung("Điều khiển", dk), khung("Thông số", ts))

	// --- biểu đồ, thanh kéo, bảng chỉ số, nhật ký
	u.bd = moiBanDo(u)
	u.thanh = widget.NewSlider(0, 1000)
	u.thanh.OnChanged = u.keo
	u.bang = container.NewGridWithColumns(7)
	choBang := canvas.NewRectangle(color.Transparent)
	choBang.SetMinSize(fyne.NewSize(10, 5*20+6))
	u.capNhatBang()
	nenBd := canvas.NewRectangle(nen)
	nenBd.StrokeColor = hex("#c9ced8")
	nenBd.StrokeWidth = 1
	trai := container.NewBorder(nil, container.NewVBox(container.NewBorder(nil, nil, container.NewPadded(u.lblGio), nil, u.thanh), container.NewStack(choBang, container.NewVBox(u.bang))), nil, nil,
		container.NewStack(nenBd, u.bd))
	u.nk = widget.NewLabel("")
	u.nk.Wrapping = fyne.TextWrapWord
	u.nk.TextStyle = fyne.TextStyle{Monospace: true}
	u.nk.SizeName = theme.SizeNameCaptionText
	u.nkCuon = container.NewVScroll(u.nk)
	rong := canvas.NewRectangle(color.Transparent)
	rong.SetMinSize(fyne.NewSize(400, 100))
	phai := khung("Nhật ký sự kiện", container.NewStack(rong, u.nkCuon))
	giua := container.NewBorder(nil, nil, nil, phai, trai)

	u.w.SetContent(container.NewPadded(container.NewBorder(tren, nil, nil, nil, giua)))
	u.w.Resize(fyne.NewSize(1280, 860))
	u.ve()
	go func() { // chỉ đếm nhịp; mọi việc thật chạy trên luồng giao diện qua fyne.Do
		for range time.Tick(30 * time.Millisecond) {
			fyne.Do(u.nhip)
		}
	}()
	return u
}

// ------------------------------------------------------------------ thao tác

func (u *ungDung) docThamSo() *L.ThamSo {
	so := func(s string, mac float64) float64 {
		v, err := strconv.ParseFloat(strings.ReplaceAll(strings.TrimSpace(s), ",", "."), 64)
		if err != nil {
			return mac
		}
		return v
	}
	return L.TaoThamSo(max(0.4, so(u.e["t1"].Text, 4)), max(0.4, so(u.e["t2"].Text, 3)), max(0.4, so(u.e["t3"].Text, 2)),
		int(min(4, max(1, so(u.selLoi.Selected, 2)))), max(0.1, so(u.e["lat"].Text, 0.5)),
		u.chkPhu.Checked, u.chkChia.Checked, u.chkKhan.Checked, max(0.0, so(u.e["t_khan"].Text, 0.3)))
}

func (u *ungDung) chon(ds []string) {
	ts := u.docThamSo()
	u.ketQua = nil
	for _, m := range ds {
		u.ketQua = append(u.ketQua, ketQuaCheDo{m, L.Chay(m, ts)})
	}
	u.T, u.dangChay, u.last = 0, true, time.Time{}
	u.datNutChay()
	u.capNhatBang()
	u.ve()
}

func (u *ungDung) xoa() {
	u.ketQua, u.T, u.dangChay = nil, 0, false
	u.datNutChay()
	u.capNhatBang()
	u.ve()
}

func (u *ungDung) chayDung() {
	u.dangChay = !u.dangChay
	u.last = time.Time{}
	u.datNutChay()
}

func (u *ungDung) datNutChay() {
	if u.dangChay {
		u.btnChay.SetText("Dừng")
		u.btnChay.SetIcon(theme.MediaPauseIcon())
	} else {
		u.btnChay.SetText("Chạy")
		u.btnChay.SetIcon(theme.MediaPlayIcon())
	}
}

// chenNgang – đặt việc khẩn U đến ngay thời điểm đang xem rồi mô phỏng lại; giữ nguyên thời điểm xem.
func (u *ungDung) chenNgang() {
	if len(u.ketQua) == 0 {
		return
	}
	u.chkKhan.SetChecked(true)
	u.e["t_khan"].SetText(L.Giay(int(u.T)))
	ts := u.docThamSo()
	for i := range u.ketQua {
		u.ketQua[i].kq = L.Chay(u.ketQua[i].cheDo, ts)
	}
	u.capNhatBang()
	u.ve()
}

func (u *ungDung) keo(v float64) {
	if D := u.doDai(); D > 0 && !u.dangChay && !u.datThanh {
		u.T = v / 1000 * float64(D)
		u.ve()
	}
}

func (u *ungDung) doDai() int {
	D := 0
	for _, x := range u.ketQua {
		D = max(D, x.kq.Tong)
	}
	return D
}

func oBang(s string, dam bool) fyne.CanvasObject {
	t := canvas.NewText(s, chu)
	t.Alignment = fyne.TextAlignCenter
	t.TextSize = 13
	t.TextStyle = fyne.TextStyle{Bold: dam}
	return t
}

func (u *ungDung) capNhatBang() {
	var o []fyne.CanvasObject
	for _, c := range []string{"Chế độ", "Hoàn tất", "Luồng", "Lõi bận", "U chờ", "Chen", "Hết lát"} {
		o = append(o, oBang(c, true))
	}
	for _, x := range u.ketQua {
		s := L.TinhChiSo(x.kq)
		uc := "–"
		if s.UCho >= 0 {
			uc = L.Giay(s.UCho) + " s"
		}
		for _, c := range []string{L.TenNgan[x.cheDo], L.Giay(s.Tong) + " s", strconv.Itoa(s.Luong),
			strconv.Itoa(s.Ban) + " %", uc, strconv.Itoa(s.Chen), strconv.Itoa(s.Lat)} {
			o = append(o, oBang(c, false))
		}
	}
	u.bang.Objects = o
	u.bang.Refresh()
}

func (u *ungDung) nhip() {
	now := time.Now()
	if u.dangChay && len(u.ketQua) > 0 {
		if !u.last.IsZero() {
			toc, _ := strconv.ParseFloat(strings.Fields(u.selToc.Selected)[1], 64)
			u.T += now.Sub(u.last).Seconds() * 10 * toc // × 1: 1 giây mô phỏng mỗi giây thật
			if u.T >= float64(u.doDai()) {
				u.T = float64(u.doDai())
				u.dangChay = false
				u.datNutChay()
			}
		}
		u.last = now
		u.ve()
	}
}

// ------------------------------------------------------------------ vẽ

func (u *ungDung) ve() {
	u.soLanVe++
	D := u.doDai()
	t := min(int(u.T), D)
	if D > 0 {
		u.lblGio.Text = "t = " + L.Giay(t) + " / " + L.Giay(D) + " s"
		u.datThanh = true
		u.thanh.SetValue(1000 * u.T / float64(D))
		u.datThanh = false
	} else {
		u.lblGio.Text = "Chọn một chế độ"
	}
	u.lblGio.Refresh()
	u.bd.Refresh()
	u.veNhatKy(t)
}

func (u *ungDung) veNhatKy(t int) {
	var sb strings.Builder
	for _, x := range u.ketQua {
		if len(u.ketQua) > 1 {
			sb.WriteString("— " + L.TenNgan[x.cheDo] + " —\n")
		}
		for _, d := range x.kq.NhatKy {
			if d.T <= t {
				sb.WriteString(L.Phai(L.Giay(d.T), 4) + " s  " + d.Text + "\n")
			}
		}
	}
	s := strings.TrimSuffix(sb.String(), "\n")
	if s != u.nkCu { // chỉ đặt lại chữ khi nhật ký đổi
		u.nkCu = s
		u.nk.SetText(s)
		u.nkCuon.ScrollToBottom()
	}
}

func main() {
	a := app.NewWithID("vn.edu.ltm.mr56")
	a.Settings().SetTheme(giaoDienSang{theme.DefaultTheme()})
	u := moi(a)
	if slices.Contains(os.Args[1:], "--demo") {
		go demo(u)
	}
	u.w.ShowAndRun()
}
