package main

// Trang flash sale (ứng dụng người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu cầu)
// ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ trang "giờ vàng" như sàn thương mại điện tử: dải FLASH
// SALE, đồng hồ đếm ngược tới giờ mở bán (chạy cục bộ theo mo_con_ms của cap_nhat), thẻ sản phẩm (ảnh vẽ
// bằng bút ảnh, nhãn giảm giá, giá sale cỡ lớn, giá gốc gạch ngang), thanh tồn kho "Đã bán x%" (cam, "SẮP
// CHÁY HÀNG" khi > 80 %, "HẾT HÀNG"), chọn số lượng 1–2, nút MUA NGAY bị khóa tới giờ mở bán, đơn hàng của
// bạn và "vừa mua" trực tiếp.

import (
	"fmt"
	"image"
	"image/color"
	"math"
	"strconv"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.11_FlashSale/tranhchap"
)

var (
	doSale  = mau("#e4002b")
	camSale = mau("#ff6a00")
)

// boGoc – đường bao hình chữ nhật bo góc bán kính r (các cung 90° xấp xỉ bằng đoạn thẳng ngắn).
func boGoc(x0, y0, x1, y1, r float64) []diem {
	var p []diem
	goc := [][3]float64{{x1 - r, y0 + r, -90}, {x1 - r, y1 - r, 0}, {x0 + r, y1 - r, 90}, {x0 + r, y0 + r, 180}}
	for _, g := range goc {
		for i := 0; i <= 8; i++ {
			a := (g[2] + float64(i)*90/8) * math.Pi / 180
			p = append(p, diem{g[0] + r*math.Cos(a), g[1] + r*math.Sin(a)})
		}
	}
	return p
}

// anhTaiNghe – ảnh sản phẩm: hộp sạc và đôi tai nghe không dây, tâm (cx, cy), tỉ lệ k.
func anhTaiNghe(w, h int, cx, cy, k float64, nen color.NRGBA) image.Image {
	b := moiBut(w, h)
	b.chuNhat(0, 0, float64(w), float64(h), nen)
	for i, m := range []string{"#ffe0cc", "#ffd1b3", "#ffc299"} {
		r := (95 - float64(i)*22) * k
		b.elip(cx-r, cy-r+10*k, cx+r, cy+r+10*k, pm(mau(m)), nil, 0, false)
	}
	x0, x1, y0, y1 := cx-52*k, cx+52*k, cy+2*k, cy+62*k // hộp sạc bo góc
	bong := boGoc(x0+3*k, y0+3*k, x1+3*k, y1+3*k, 22*k)
	b.daGiac(bong, mau("#c9b8a8"))
	hop := boGoc(x0, y0, x1, y1, 22*k)
	b.daGiac(hop, mau("#fafafa"))
	b.gapKhuc(append(hop, hop[0]), 1, mau("#9a9a9a"), false)
	b.duong(x0+4*k, y0+22*k, x1-4*k, y0+22*k, 1, mau("#b8b8b8"))
	b.elip(cx-3*k, y0+34*k, cx+3*k, y0+40*k, pm(mau("#2ecc71")), nil, 0, false)
	for _, d := range []float64{-1, 1} { // hai tai nghe
		hx, hy := cx+d*28*k, cy-42*k
		than := splineMin([]diem{{hx - d*4*k, hy + 10*k}, {hx + d*6*k, hy + 10*k}, {hx + d*12*k, hy + 52*k},
			{hx + d*2*k, hy + 54*k}, {hx - d*4*k, hy + 10*k}}, 6)
		b.daGiac(than, mau("#f4f4f4"))
		b.gapKhuc(than, 1, mau("#9a9a9a"), false)
		b.elip(hx-17*k, hy-15*k, hx+17*k, hy+17*k, pm(Trang), pm(mau("#9a9a9a")), 1, false)
		b.elip(hx-8*k, hy-6*k, hx+8*k, hy+8*k, pm(mau("#3a3a3a")), nil, 0, false)
		b.elip(hx-4*k, hy-3*k, hx+1*k, hy+1*k, pm(mau("#7a7a7a")), nil, 0, false)
	}
	return b.anh()
}

// dongHoSale – hh:mm:ss làm tròn lên.
func dongHoSale(giay float64) string {
	g := max(0, int(giay+0.999))
	return fmt.Sprintf("%02d:%02d:%02d", g/3600, g/60%60, g%60)
}

type donCua struct {
	ma       string
	sl, tien int64
	gio      string
}

type cuaSoFlashSale struct {
	*cuaSoNguoiDungCoSo
	moLuc       *time.Time // thời điểm mở bán theo đồng hồ máy này
	don         []donCua   // đơn của tôi
	dangCho     bool
	nhanTrThai  *canvas.Text
	dongHo      *oMau
	nhanTen     *widget.Label
	nhanGia     *canvas.Text
	nhanGoc     *canvas.Text
	gachGoc     *canvas.Line
	nhanGioiHan *canvas.Text
	nhanGiam    *canvas.Text
	hetHang     *fyne.Container
	nenKho      *canvas.Rectangle
	thanhKho    *canvas.Rectangle
	chuKho      *canvas.Text
	chonSL      *widget.RadioGroup
	nutMua      *widget.Button
	thongBao    *oMau
	lsDon       *dsChu
	lsGan       *dsChu
}

func moCuaSoFlashSale(a fyne.App, ten, host string, cong int) *cuaSoFlashSale {
	x := &cuaSoFlashSale{}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Flash sale", ten, host, cong, x)
	x.hoanTat(600)
	return x
}

func (x *cuaSoFlashSale) ve() fyne.CanvasObject {
	fs := chu("FLASH SALE", Trang, 26, true)
	fs.TextStyle.Italic = true
	gv := container.NewVBox(chu("GIỜ VÀNG", mau("#ffe066"), 11, true), chu("GIÁ SỐC", mau("#ffe066"), 11, true))
	x.nhanTrThai = chu("Bắt đầu sau", Trang, 13, true)
	x.dongHo = moiOMau("--:--:--", mau("#1a1a1a"), Trang, 19, 188, 34, true)
	x.dongHo.chu.TextStyle.Monospace = true
	dai := container.NewStack(canvas.NewRectangle(doSale), container.NewPadded(container.NewHBox(fs,
		container.NewCenter(gv), layout.NewSpacer(), container.NewCenter(x.nhanTrThai), x.dongHo.O)))

	anh := oAnhTinh(anhTaiNghe(230, 210, 115, 92, 1, Trang), 230, 210)
	nhan := canvas.NewCircle(doSale)
	nhan.StrokeColor, nhan.StrokeWidth = Trang, 2
	x.nhanGiam = chu("", Trang, 15, true)
	x.nhanGiam.Alignment = fyne.TextAlignCenter
	hh := chu("HẾT HÀNG", Trang, 18, true)
	hh.Alignment = fyne.TextAlignCenter
	x.hetHang = container.NewStack(canvas.NewRectangle(mau("#404040")), container.NewCenter(hh))
	the := container.NewWithoutLayout(anh, nhan, x.nhanGiam, x.hetHang)
	anh.Move(fyne.NewPos(0, 0))
	anh.Resize(fyne.NewSize(230, 210))
	nhan.Move(fyne.NewPos(168, 8))
	nhan.Resize(fyne.NewSize(54, 54))
	x.nhanGiam.Move(fyne.NewPos(168, 24))
	x.nhanGiam.Resize(fyne.NewSize(54, 22))
	x.hetHang.Move(fyne.NewPos(0, 170))
	x.hetHang.Resize(fyne.NewSize(230, 35))
	x.hetHang.Hide()

	x.nhanTen = widget.NewLabel("–")
	x.nhanTen.TextStyle.Bold = true
	x.nhanTen.Wrapping = fyne.TextWrapWord
	x.nhanGia = chu("–", doSale, 28, true)
	x.nhanGoc = chu("", mau("#909090"), 14, false)
	x.gachGoc = canvas.NewLine(mau("#909090"))
	x.gachGoc.StrokeWidth = 1
	goc := container.NewWithoutLayout(x.nhanGoc, x.gachGoc)
	x.nhanGioiHan = chu("", XamChu, 11, false)
	x.nenKho = canvas.NewRectangle(mau("#ffd6c2"))
	x.thanhKho = canvas.NewRectangle(camSale)
	x.chuKho = chu("", mau("#a03000"), 13, true)
	x.chuKho.Alignment = fyne.TextAlignCenter
	kho := container.NewWithoutLayout(x.nenKho, x.thanhKho, x.chuKho)
	x.nenKho.Move(fyne.NewPos(0, 2))
	x.nenKho.Resize(fyne.NewSize(300, 20))
	x.thanhKho.Move(fyne.NewPos(0, 2))
	x.thanhKho.Resize(fyne.NewSize(0, 20))
	x.chuKho.Move(fyne.NewPos(0, 2))
	x.chuKho.Resize(fyne.NewSize(300, 20))
	x.chonSL = widget.NewRadioGroup([]string{"1", "2"}, nil)
	x.chonSL.Horizontal = true
	x.chonSL.SetSelected("1")
	x.chonSL.Required = true
	x.nutMua = nut("CHỜ MỞ BÁN", widget.WarningImportance, func() { x.mua(0) })
	phai := container.NewVBox(coDinh(x.nhanTen, 320, 48),
		container.NewHBox(x.nhanGia, coDinh(goc, 110, 34)), x.nhanGioiHan, coDinh(kho, 300, 24),
		container.NewHBox(widget.NewLabel("Số lượng:"), x.chonSL), coDinh(x.nutMua, 230, 46))
	theSP := container.NewStack(canvas.NewRectangle(Trang),
		container.NewPadded(container.NewHBox(coDinh(the, 230, 210), phai)))
	x.thongBao = moiOMau("Chưa kết nối", XamNhat, Den, 14, 590, 30, true)
	x.lsDon = moiDsChu(4, 11, 330, 80)
	x.lsGan = moiDsChu(4, 11, 240, 80)
	x.nut()
	return container.NewVBox(dai, theSP, x.thongBao.O,
		container.NewHBox(nhom("Đơn hàng của bạn", x.lsDon.O), nhom("Vừa mua (trực tiếp)", x.lsGan.O)))
}

// ------------------------------------------------------------ mua

// mua – bấm MUA NGAY; sl > 0 (kịch bản demo): chọn số lượng trước.
func (x *cuaSoFlashSale) mua(sl int64) {
	if x.kn == nil || x.dangCho {
		return
	}
	if sl > 0 {
		x.chonSL.SetSelected(strconv.FormatInt(sl, 10))
	}
	sl, _ = strconv.ParseInt(x.chonSL.Selected, 10, 64)
	x.dangCho = true
	x.nutMua.SetText("ĐANG XỬ LÝ…")
	x.nutMua.Disable()
	x.log.them(fmt.Sprintf("MUA NGAY %d sản phẩm", sl))
	x.guiYC("mua", tc.T("so_luong", sl))
}

func (x *cuaSoFlashSale) ketQua(_ string, _ tc.TS, tin tc.M, rtt float64) {
	x.dangCho = false
	if ok, _ := tin["ok"].(bool); ok {
		x.don = append(x.don, donCua{tc.Chuoi(tin["ma_don"]), tc.SoNguyen(tin["so_luong"]), tc.SoNguyen(tin["thanh_tien"]),
			time.Now().Format("15:04:05")})
		x.bao(fmt.Sprintf("Đặt hàng thành công! Đơn %s – %d sp", tc.Chuoi(tin["ma_don"]), tc.SoNguyen(tin["so_luong"])),
			mau("#70d070"), Den)
		x.log.them(fmt.Sprintf("  đơn %s, %s (%.1f ms)", tc.Chuoi(tin["ma_don"]), tc.Tien(tc.SoNguyen(tin["thanh_tien"])), rtt))
		x.veDon()
	} else {
		m, _ := tin["ma_loi"].(string)
		text := map[string]string{"chua_mo": "Chưa tới giờ mở bán!", "het_hang": "Rất tiếc, sản phẩm đã HẾT HÀNG",
			"khong_du": fmt.Sprintf("Chỉ còn %s sản phẩm – giảm số lượng", tc.StrPy(tin["ton"])),
			"xung_dot": "Hệ thống đang bận, vui lòng thử lại"}[m]
		if text == "" {
			text = tc.Chuoi(tin["ly_do"])
		}
		if m == "het_hang" {
			x.bao(text, mau("#ff6060"), Trang)
		} else {
			x.bao(text, mau("#ffb347"), Den)
		}
		x.log.them("  từ chối: " + tc.StrPy(tin["ly_do"]))
	}
	x.nut()
}

func (x *cuaSoFlashSale) veDon() {
	var ds []string
	for i := len(x.don) - 1; i >= 0; i-- {
		d := x.don[i]
		ds = append(ds, fmt.Sprintf("%s %s ×%d %11s", d.gio, d.ma, d.sl, tc.Tien(d.tien)))
	}
	x.lsDon.dat(ds, nil)
}

func (x *cuaSoFlashSale) bao(text string, nen, mauChu color.Color) { x.thongBao.dat(text, nen, mauChu) }

func (x *cuaSoFlashSale) daMo() bool { return x.moLuc != nil && !time.Now().Before(*x.moLuc) }

func (x *cuaSoFlashSale) nut() {
	if x.dangCho {
		return
	}
	text, bat := "MUA NGAY", true
	switch {
	case x.kn == nil || x.tt == nil:
		text, bat = "CHỜ MỞ BÁN", false
	case tc.SoNguyen(x.tt["ton"]) <= 0:
		text, bat = "HẾT HÀNG", false
	case !x.daMo():
		text, bat = "CHỜ MỞ BÁN", false
	}
	if x.nutMua.Text != text {
		x.nutMua.SetText(text)
	}
	if bat && x.nutMua.Disabled() {
		x.nutMua.Enable()
	} else if !bat && !x.nutMua.Disabled() {
		x.nutMua.Disable()
	}
}

// trangThaiNut – "normal" / "disabled" như Tk (in trong demo).
func (x *cuaSoFlashSale) trangThaiNut() string {
	if x.nutMua.Disabled() {
		return "disabled"
	}
	return "normal"
}

// ------------------------------------------------------------ nhận trạng thái

func (x *cuaSoFlashSale) capNhat(tt tc.M) {
	sp, _ := tt["sp"].(tc.M)
	t := time.Now().Add(time.Duration(tc.SoThuc(tt["mo_con_ms"]) * float64(time.Millisecond)))
	x.moLuc = &t
	x.nhanTen.SetText(tc.Chuoi(sp["ten"]))
	datChu(x.nhanGia, tc.Tien(tc.SoNguyen(sp["gia_sale"])), doSale)
	datChu(x.nhanGoc, tc.Tien(tc.SoNguyen(sp["gia_goc"])), mau("#909090"))
	kt := x.nhanGoc.MinSize()
	x.nhanGoc.Move(fyne.NewPos(8, 34-kt.Height-4))
	x.nhanGoc.Resize(kt)
	x.gachGoc.Position1, x.gachGoc.Position2 = fyne.NewPos(8, 34-4-kt.Height/2), fyne.NewPos(8+kt.Width, 34-4-kt.Height/2)
	x.gachGoc.Refresh()
	tonDau, ton := tc.SoNguyen(tt["ton_dau"]), tc.SoNguyen(tt["ton"])
	datChu(x.nhanGioiHan, fmt.Sprintf("Mỗi đơn tối đa %d sản phẩm · Số lượng có hạn: %d", tc.SoNguyen(sp["toi_da"]), tonDau),
		XamChu)
	giam := tc.LamTron(100 * (1 - tc.SoThuc(sp["gia_sale"])/tc.SoThuc(sp["gia_goc"])))
	datChu(x.nhanGiam, fmt.Sprintf("-%d%%", giam), Trang)
	if ton <= 0 {
		x.hetHang.Show()
	} else {
		x.hetHang.Hide()
	}
	tiLe := min(1.0, float64(tonDau-ton)/float64(tonDau))
	m := camSale
	if tiLe >= 0.8 {
		m = doSale
	}
	x.thanhKho.FillColor = m
	x.thanhKho.Resize(fyne.NewSize(float32(300*tiLe), 20))
	x.thanhKho.Refresh()
	chuKho := fmt.Sprintf("ĐÃ BÁN %d%%", tc.LamTron(tiLe*100))
	switch {
	case ton <= 0:
		chuKho = "HẾT HÀNG"
	case tiLe >= 0.8:
		chuKho = fmt.Sprintf("SẮP CHÁY HÀNG – còn %d", ton)
	}
	mChu := mau("#a03000")
	if tiLe > 0.45 {
		mChu = Trang
	}
	datChu(x.chuKho, chuKho, mChu)
	var ds []string
	var cuaToi []bool
	for _, d := range tc.DSM(tt["don_gan"]) {
		ds = append(ds, fmt.Sprintf("%s %-6s ×%d", tc.Chuoi(d["gio"]), tc.Chuoi(d["nguoi"]), tc.SoNguyen(d["so_luong"])))
		cuaToi = append(cuaToi, tc.Chuoi(d["nguoi"]) == x.ten)
	}
	x.lsGan.dat(ds, func(i int) color.Color {
		if cuaToi[i] {
			return mau("#0040c0")
		}
		return Den
	})
	if x.thongBao.text() == "Chưa kết nối" {
		x.bao("Đón giờ vàng – nút MUA NGAY mở khi hết đếm ngược", XamNhat, Den)
	}
	x.moiNhip()
}

func (x *cuaSoFlashSale) suKien(text string) {
	x.log.them("* " + text)
	if strings.HasPrefix(text, "Đặt lại") { // đợt sale mới
		x.don = nil
		x.veDon()
		x.bao("Đợt sale mới – chờ giờ mở bán", XamNhat, Den)
	}
}

func (x *cuaSoFlashSale) moiNhip() {
	if x.tt == nil || x.moLuc == nil {
		return
	}
	con := time.Until(*x.moLuc).Seconds()
	switch {
	case con > 0:
		datChu(x.nhanTrThai, "Bắt đầu sau", Trang)
		x.dongHo.dat(dongHoSale(con), mau("#1a1a1a"), Trang)
	case tc.SoNguyen(x.tt["ton"]) <= 0:
		datChu(x.nhanTrThai, "Đã kết thúc", Trang)
		x.dongHo.dat("CHÁY HÀNG", mau("#404040"), Trang)
	default:
		datChu(x.nhanTrThai, "", Trang)
		x.dongHo.dat("ĐANG DIỄN RA", DoDam, Trang)
	}
	x.nut()
}

func (x *cuaSoFlashSale) dongDinhKy() string {
	con := 0.0
	if x.moLuc != nil {
		con = max(0, time.Until(*x.moLuc).Seconds())
	}
	mo := "đang bán"
	if con > 0 {
		mo = fmt.Sprintf("mở sau %.0f s", con)
	}
	return fmt.Sprintf("tồn %d/%d, %s; bạn có %d đơn", tc.SoNguyen(x.tt["ton"]), tc.SoNguyen(x.tt["ton_dau"]), mo, len(x.don))
}

func (x *cuaSoFlashSale) daKetNoi()  { x.nut() }
func (x *cuaSoFlashSale) matKetNoi() { x.nut() }
