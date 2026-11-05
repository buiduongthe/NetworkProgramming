package main

// Cổng đăng ký học phần (ứng dụng người dùng). Khung cửa sổ (kết nối, nhật ký, thanh trạng thái, gửi yêu
// cầu) ở chung_gui.go (cuaSoNguoiDungCoSo); tệp này vẽ cổng đăng ký kiểu trang đào tạo của trường: thanh
// tiêu đề, thông tin sinh viên (số lớp, số tín chỉ đã đăng ký), bảng lớp học phần mở đăng ký (mã lớp, tên
// học phần, tín chỉ, giảng viên, lịch học, thanh sĩ số "đã đăng ký / sĩ số" đổi màu khi gần đủ / đủ, nút
// Đăng ký / Hủy / Đã đủ), danh sách lớp của bạn. Bảng cập nhật trực tiếp từ cap_nhat (≤ 10 lần/giây). Nếu
// máy chủ trả lời "thành công" mà danh sách lớp (theo cap_nhat) liên tục 0,5 s không có tên mình – lượt
// đăng ký bị ghi đè ở chế độ khong_khoa – cửa sổ báo "đăng ký đã bị MẤT".

import (
	"fmt"
	"image/color"
	"sort"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.10_DangKyHocPhan/tranhchap"
)

var xanhDH = mau("#0b3d91") // màu thanh tiêu đề cổng đào tạo

var cotBang = cotCoDinh{78, 108, 28, 132, 128, 118, 76}

var tenCot = []string{"Mã lớp HP", "Tên học phần", "TC", "Giảng viên", "Lịch học", "Đã ĐK / Sĩ số", ""}

// oBang – một ô của bảng (nền + chữ).
type oBang struct {
	nen *canvas.Rectangle
	chu *canvas.Text
	O   fyne.CanvasObject
}

func moiOBang(s string, nen, mauChu color.Color, dam bool) *oBang {
	o := &oBang{nen: canvas.NewRectangle(nen), chu: chu(s, mauChu, 11, dam)}
	o.O = container.NewStack(o.nen, container.New(&leTrai{4}, o.chu))
	return o
}

// leTrai – bố cục: chữ cách lề trái l điểm ảnh, căn giữa theo chiều dọc.
type leTrai struct{ l float32 }

func (b *leTrai) Layout(o []fyne.CanvasObject, kt fyne.Size) {
	for _, x := range o {
		m := x.MinSize()
		x.Move(fyne.NewPos(b.l, (kt.Height-m.Height)/2))
		x.Resize(m)
	}
}

func (b *leTrai) MinSize(o []fyne.CanvasObject) fyne.Size {
	var w, h float32
	for _, x := range o {
		w, h = max(w, x.MinSize().Width+b.l), max(h, x.MinSize().Height)
	}
	return fyne.NewSize(w, h)
}

func (o *oBang) dat(s string, nen color.Color, dam bool) {
	if o.chu.Text != s || o.chu.TextStyle.Bold != dam {
		o.chu.Text, o.chu.TextStyle.Bold = s, dam
		o.chu.Refresh()
	}
	if o.nen.FillColor != nen {
		o.nen.FillColor = nen
		o.nen.Refresh()
	}
}

type hangLop struct {
	o       []*oBang
	nen     color.NRGBA
	nenTS   *canvas.Rectangle // nền ô thanh sĩ số
	thanh   *canvas.Rectangle
	so      *canvas.Text
	nut     *widget.Button
	nhanNut string
}

type cuaSoDangKy struct {
	*cuaSoNguoiDungCoSo
	cuaToi   map[string]string    // mã lớp -> giờ đăng ký (theo kết quả máy chủ)
	thieuTu  map[string]time.Time // mã lớp -> lúc thấy danh sách lớp thiếu tên mình
	dangCho  map[string]bool      // mã lớp đang chờ trả lời
	choCN    map[string]bool      // mã lớp vừa có kq, chưa có cap_nhat nào tới sau
	thuTu    []string
	hang     map[string]*hangLop
	bang     *fyne.Container
	nhanSV   *canvas.Text
	nhanTC   *canvas.Text
	thongBao *oMau
	ls       *dsChu
	dongLs   []string
}

func moCuaSoDangKy(a fyne.App, ten, host string, cong int) *cuaSoDangKy {
	x := &cuaSoDangKy{cuaToi: map[string]string{}, thieuTu: map[string]time.Time{}, dangCho: map[string]bool{},
		choCN: map[string]bool{}, hang: map[string]*hangLop{}}
	x.cuaSoNguoiDungCoSo = moCuaSoNguoiDungCoSo(a, "Cổng đăng ký học phần", ten, host, cong, x)
	x.hoanTat(700)
	return x
}

func (x *cuaSoDangKy) ve() fyne.CanvasObject {
	dau := container.NewStack(canvas.NewRectangle(xanhDH), container.NewPadded(container.NewHBox(
		chu("CỔNG ĐĂNG KÝ HỌC PHẦN", Trang, 18, true), layout.NewSpacer(),
		container.NewCenter(chu("Học kỳ 1 · Năm học 2026–2027", mau("#ffd54f"), 13, true)))))
	x.nhanSV, x.nhanTC = chu("Sinh viên: –", Den, 13, true), chu("", Den, 13, false)
	tt := container.NewStack(canvas.NewRectangle(mau("#e8eef8")), container.NewPadded(container.NewHBox(x.nhanSV,
		layout.NewSpacer(), x.nhanTC)))
	x.thongBao = moiOMau("Chưa kết nối", XamNhat, Den, 14, 680, 30, true)
	var dauBang []fyne.CanvasObject
	for _, t := range tenCot {
		dauBang = append(dauBang, moiOBang(t, mau("#34558b"), Trang, true).O)
	}
	x.bang = container.NewVBox(container.New(cotBang, dauBang...))
	x.ls = moiDsChu(4, 11, 680, 80)
	return container.NewVBox(dau, tt, x.thongBao.O,
		nhom("Danh sách lớp học phần mở đăng ký", container.NewStack(canvas.NewRectangle(mau("#9aa9c4")),
			coDinh(x.bang, 690, 196))),
		nhom("Kết quả đăng ký của bạn", x.ls.O))
}

// ------------------------------------------------------------ bảng lớp

func (x *cuaSoDangKy) taoHang(i int, ma string) *fyne.Container {
	nen := mau("#f3f6fb")
	if i%2 == 1 {
		nen = Trang
	}
	h := &hangLop{nen: nen, nenTS: canvas.NewRectangle(nen)}
	var cot []fyne.CanvasObject
	for range 5 {
		o := moiOBang("", nen, Den, false)
		h.o = append(h.o, o)
		cot = append(cot, o.O)
	}
	nenThanh := canvas.NewRectangle(mau("#dddddd"))
	h.thanh = canvas.NewRectangle(mau("#3cb44b"))
	thanh := container.NewWithoutLayout(nenThanh, h.thanh)
	nenThanh.Resize(fyne.NewSize(70, 12))
	h.thanh.Resize(fyne.NewSize(0, 12))
	h.so = chu("", Den, 11, true)
	cot = append(cot, container.NewStack(h.nenTS, container.NewHBox(widget.NewLabel(""),
		container.NewCenter(coDinh(thanh, 70, 12)), container.NewCenter(h.so))))
	h.nut = nut("Đăng ký", widget.HighImportance, func() { x.bam(ma) })
	cot = append(cot, h.nut)
	x.hang[ma] = h
	return container.New(cotBang, cot...)
}

func (x *cuaSoDangKy) bam(ma string) {
	if x.kn == nil || x.dangCho[ma] {
		return
	}
	loai := "dang_ky"
	if _, co := x.cuaToi[ma]; co {
		loai = "huy"
	}
	x.dangCho[ma] = true
	h := x.hang[ma]
	h.nut.SetText("…")
	h.nhanNut = "…"
	h.nut.Disable()
	if loai == "huy" {
		x.log.them("Hủy " + ma)
	} else {
		x.log.them("Đăng ký " + ma)
	}
	x.guiYC(loai, tc.T("ma_lop", ma))
}

// dangKy – kịch bản demo: bấm nút của lớp ma.
func (x *cuaSoDangKy) dangKy(ma string) {
	if h := x.hang[ma]; h != nil {
		bam(h.nut)
	}
}

func gio() string { return time.Now().Format("15:04:05") }

func (x *cuaSoDangKy) ketQua(loai string, truong tc.TS, tin tc.M, rtt float64) {
	v, _ := truong.Lay("ma_lop")
	ma := tc.Chuoi(v)
	delete(x.dangCho, ma)
	x.choCN[ma] = true // tt đang giữ có thể chụp trước lần ghi này
	if ok, _ := tin["ok"].(bool); ok {
		if loai == "dang_ky" {
			x.cuaToi[ma] = gio()
			x.bao("Đăng ký thành công lớp "+ma, mau("#70d070"), Den)
		} else {
			delete(x.cuaToi, ma)
			x.bao("Đã hủy đăng ký lớp "+ma, mau("#d0d0ff"), Den)
		}
		x.log.them(fmt.Sprintf("  thành công (%d/%d, %.1f ms)", tc.SoNguyen(tin["da_dk"]), tc.SoNguyen(x.tt["si_so"]), rtt))
	} else {
		m, _ := tin["ma_loi"].(string)
		text := map[string]string{"lop_day": "Lớp " + ma + " đã đủ sĩ số", "da_dang_ky": "Bạn đã đăng ký lớp " + ma + " rồi",
			"xung_dot": "Hệ thống bận, vui lòng thử lại"}[m]
		if text == "" {
			text = tc.Chuoi(tin["ly_do"])
		}
		if _, co := x.cuaToi[ma]; m == "da_dang_ky" && !co {
			x.cuaToi[ma] = gio()
		}
		if m == "lop_day" {
			x.bao(text, mau("#ff6060"), Trang)
		} else {
			x.bao(text, mau("#ffb347"), Den)
		}
		x.log.them("  từ chối: " + tc.StrPy(tin["ly_do"]))
	}
	if x.tt != nil {
		x.capNhatBang(x.tt, false)
	}
}

func (x *cuaSoDangKy) bao(text string, nen, mauChu color.Color) { x.thongBao.dat(text, nen, mauChu) }

// ------------------------------------------------------------ nhận trạng thái

// capNhat – chỉ được khung gọi khi có cap_nhat MỚI (tới sau các kq đã nhận): bỏ danh sách "chờ cap_nhat".
func (x *cuaSoDangKy) capNhat(tt tc.M) {
	x.choCN = map[string]bool{}
	x.capNhatBang(tt, true)
}

func coTen(x tc.M, ten string) bool {
	for _, t := range tc.DSChuoi(x["sv"]) {
		if t == ten {
			return true
		}
	}
	return false
}

func (x *cuaSoDangKy) capNhatBang(tt tc.M, kiemMat bool) {
	lop := tc.DSM(tt["lop"])
	var maMoi []string
	for _, l := range lop {
		maMoi = append(maMoi, tc.Chuoi(l["ma"]))
	}
	if strings.Join(maMoi, ",") != strings.Join(x.thuTu, ",") { // số lớp đổi (đặt lại): vẽ lại bảng
		for len(x.bang.Objects) > 1 {
			x.bang.Remove(x.bang.Objects[len(x.bang.Objects)-1])
		}
		x.hang, x.thuTu = map[string]*hangLop{}, maMoi
		for i, ma := range maMoi {
			x.bang.Add(x.taoHang(i, ma))
		}
	}
	if kiemMat {
		x.kiemMat(tt)
	}
	for _, l := range lop {
		ma := tc.Chuoi(l["ma"])
		h := x.hang[ma]
		_, cuaToi := x.cuaToi[ma]
		nen := h.nen
		if cuaToi {
			nen = mau("#d8f5d8")
		}
		for i, gt := range []string{ma, tc.Chuoi(l["ten"]), tc.StrPy(l["tc"]), tc.Chuoi(l["gv"]), tc.Chuoi(l["lich"])} {
			h.o[i].dat(gt, nen, cuaToi)
		}
		if h.nenTS.FillColor != nen {
			h.nenTS.FillColor = nen
			h.nenTS.Refresh()
		}
		daDK, siSo := tc.SoNguyen(l["da_dk"]), tc.SoNguyen(l["si_so"])
		tiLe := min(1.0, float64(daDK)/float64(siSo))
		m := mau("#3cb44b")
		switch {
		case daDK >= siSo:
			m = DoDam
		case tiLe >= 0.8:
			m = mau("#f0a020")
		}
		h.thanh.FillColor = m
		h.thanh.Resize(fyne.NewSize(float32(70*tiLe), 12))
		h.thanh.Refresh()
		mChu := color.Color(Den)
		if daDK >= siSo {
			mChu = DoDam
		}
		datChu(h.so, fmt.Sprintf("%d/%d", daDK, siSo), mChu)
		if x.dangCho[ma] {
			continue
		}
		nhan, muc := "Đăng ký", widget.HighImportance
		switch {
		case cuaToi:
			nhan, muc = "Hủy", widget.DangerImportance
		case daDK >= siSo:
			nhan, muc = "Đã đủ", widget.LowImportance
		}
		if h.nhanNut != nhan || h.nut.Disabled() {
			h.nhanNut = nhan
			h.nut.Importance = muc
			h.nut.SetText(nhan)
			h.nut.Enable()
			h.nut.Refresh()
		}
	}
	soTC := map[string]int64{}
	ten := map[string]string{}
	for _, l := range lop {
		soTC[tc.Chuoi(l["ma"])], ten[tc.Chuoi(l["ma"])] = tc.SoNguyen(l["tc"]), tc.Chuoi(l["ten"])
	}
	var tong int64
	var cua []string
	for m := range x.cuaToi {
		tong += soTC[m]
		cua = append(cua, m)
	}
	sort.Strings(cua)
	datChu(x.nhanSV, "Sinh viên: "+x.ten, Den)
	datChu(x.nhanTC, fmt.Sprintf("Đã đăng ký %d lớp · %d tín chỉ", len(x.cuaToi), tong), Den)
	var ls []string
	for _, m := range cua {
		t, co := ten[m]
		if !co {
			t = "?"
		}
		ls = append(ls, fmt.Sprintf("%s  %-16s %d TC  lúc %s", m, t, soTC[m], x.cuaToi[m]))
	}
	x.dongLs = ls
	x.ls.dat(ls, nil)
	if x.thongBao.text() == "Chưa kết nối" {
		x.bao("Đợt đăng ký đang mở – chọn lớp và bấm Đăng ký", XamNhat, Den)
	}
}

// kiemMat – đối chiếu kết quả mình nhận với danh sách lớp của máy chủ. cap_nhat có thể đến trễ hơn kq,
// nên chỉ báo khi danh sách thiếu tên mình liên tục 0,5 s, và bỏ qua lớp vừa có kq mà chưa có cap_nhat
// nào tới sau (bản tt đang giữ chụp trước lần ghi của mình – ví dụ vừa hủy mà tt cũ vẫn còn tên).
func (x *cuaSoDangKy) kiemMat(tt tc.M) {
	for _, l := range tc.DSM(tt["lop"]) {
		ma := tc.Chuoi(l["ma"])
		_, cuaToi := x.cuaToi[ma]
		co := coTen(l, x.ten)
		cho := x.dangCho[ma] || x.choCN[ma]
		if cho || co || !cuaToi {
			delete(x.thieuTu, ma)
		}
		if cho {
			continue
		}
		if cuaToi && !co {
			t, da := x.thieuTu[ma]
			if !da {
				t = time.Now()
				x.thieuTu[ma] = t
			}
			if time.Since(t) < 500*time.Millisecond {
				continue
			}
			delete(x.thieuTu, ma)
			delete(x.cuaToi, ma) // máy chủ báo thành công nhưng danh sách không có tên
			x.bao("Đăng ký "+ma+" đã bị MẤT (lỗi máy chủ)!", DoDam, Trang)
			x.log.them("CẢNH BÁO: danh sách lớp " + ma + " không có tên bạn")
			x.capNhatBang(tt, false)
		} else if !cuaToi && co {
			x.cuaToi[ma] = "–" // (đăng ký từ thiết bị khác)
		}
	}
}

func (x *cuaSoDangKy) moiNhip() {
	if x.kn != nil && x.tt != nil {
		x.kiemMat(x.tt)
	}
}

func (x *cuaSoDangKy) suKien(text string) {
	x.log.them("* " + text)
	if strings.HasPrefix(text, "Đặt lại") { // đợt đăng ký mới: danh sách cũ không còn
		x.cuaToi, x.thieuTu = map[string]string{}, map[string]time.Time{}
		x.bao("Đợt đăng ký mới – chọn lớp và bấm Đăng ký", XamNhat, Den)
	}
}

func (x *cuaSoDangKy) dongDinhKy() string {
	var lop []string
	for _, l := range tc.DSM(x.tt["lop"]) {
		lop = append(lop, fmt.Sprintf("%s %d/%d", tc.Chuoi(l["ma"]), tc.SoNguyen(l["da_dk"]), tc.SoNguyen(l["si_so"])))
	}
	var cua []string
	for m := range x.cuaToi {
		cua = append(cua, m)
	}
	sort.Strings(cua)
	ban := strings.Join(cua, ", ")
	if ban == "" {
		ban = "chưa có lớp"
	}
	return strings.Join(lop, ", ") + "; bạn: " + ban
}

func (x *cuaSoDangKy) daKetNoi()  {}
func (x *cuaSoDangKy) matKetNoi() {}
