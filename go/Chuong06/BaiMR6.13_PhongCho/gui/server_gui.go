package main

// Bảng điều khiển máy chủ bán vé có phòng chờ ảo. Mọi phần chung (START/STOP, 4 chế độ, độ trễ, chỉ số
// trực tiếp, SAI PHẠM, mô phỏng tải, so sánh 4 chế độ, nhật ký) ở chung_gui.go (cuaSoServerCoSo); tệp này
// chỉ vẽ khung "Tài nguyên": VÉ CÒN LẠI (thanh), độ dài HÀNG ĐỢI, số đang phục vụ, tốc độ cấp lượt (đổi
// được ngay), mức token trong thùng, số yêu cầu mua đang xử lý so với giới hạn, số lần từ chối vì quá tải,
// ô chọn "Bật phòng chờ", tham số đặt lại và nút "So sánh bật/tắt phòng chờ" (bảng hai dòng: mua được, hết
// vé, quá tải, bỏ cuộc, thời gian mua p99).

import (
	"fmt"
	"image/color"
	"strconv"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.13_PhongCho/tranhchap"
)

type cuaSoServerPhongCho struct {
	*cuaSoServerCoSo
	sv                  *tc.MayChuPhongCho
	nhanVe, nhanHang    *canvas.Text
	thanhVe             *widget.ProgressBar
	nhanBan, nhanPhucVu *canvas.Text
	nhanXuLy, nhanToken *canvas.Text
	oTocDo              *widget.Entry
	nutTocDo            *widget.Button
	oTS                 map[string]*widget.Entry
	oPC                 *widget.Check
	nutSoSanhPC         *widget.Button
	bangPC              []*hangBang
	hangPC              []tc.HangPhongCho
}

func moCuaSoServer(a fyne.App, host string, cong int) *cuaSoServerPhongCho {
	s := &cuaSoServerPhongCho{oTS: map[string]*widget.Entry{}}
	s.cuaSoServerCoSo = moCuaSoServerCoSo(a, "Máy chủ bán vé MR6.13", "Tài nguyên: vé và phòng chờ ảo",
		func(bao func(string)) *tc.MayChuCoSo {
			sv, err := tc.MoiMayChu("khoa", 2.0, bao, nil)
			if err != nil {
				panic(err)
			}
			s.sv = sv
			return sv.MayChuCoSo
		}, s, host, cong, 1000, 200, tc.SoLanMacDinh)
	return s
}

func gtTS(ts tc.TS, k string) string {
	v, _ := ts.Lay(k)
	if f, la := v.(float64); la {
		return tc.FmtG(f)
	}
	return tc.StrPy(v)
}

func (s *cuaSoServerPhongCho) veTaiNguyen() fyne.CanvasObject {
	xam := mau("#404040")
	s.nhanVe, s.nhanHang = chu("–", mau("#8b0000"), 21, true), chu("–", mau("#1f4fa0"), 21, true)
	dau := container.New(cotCoDinh{150, 170}, chu("VÉ CÒN LẠI", xam, 10, true), chu("HÀNG ĐỢI", xam, 10, true))
	so := container.New(cotCoDinh{150, 170}, s.nhanVe, s.nhanHang)
	s.thanhVe = widget.NewProgressBar()
	s.thanhVe.TextFormatter = func() string { return "" }
	s.nhanBan, s.nhanPhucVu = chu("", Den, 12, false), chu("", Den, 12, false)
	s.nhanXuLy = chu("", Den, 12, true)
	ts := s.sv.ThamSo()
	s.oTocDo = oNhap(gtTS(ts, "toc_do"))
	s.nutTocDo = nut("Áp dụng ngay", widget.HighImportance, s.doiTocDo)
	s.nhanToken = chu("", Den, 10, false)
	tocDo := container.NewHBox(widget.NewLabel("Cấp lượt:"), coDinh(s.oTocDo, 56, 30), widget.NewLabel("người/s"),
		s.nutTocDo, container.NewCenter(s.nhanToken))
	luoi := container.New(layout.NewGridLayout(4))
	for _, d := range [][2]string{{"tong_ve", "Tổng vé:"}, {"dung_luong", "Thùng token:"}, {"luot_s", "Hạn lượt (s):"},
		{"gioi_han_xu_ly", "Giới hạn xử lý:"}} {
		s.oTS[d[0]] = oNhap(gtTS(ts, d[0]))
		luoi.Add(widget.NewLabel(d[1]))
		luoi.Add(s.oTS[d[0]])
	}
	pc, _ := ts.Lay("phong_cho")
	s.oPC = widget.NewCheck("Bật phòng chờ ảo (có hiệu lực khi Đặt lại)", nil)
	s.oPC.SetChecked(tc.SoNguyen(pc) != 0)
	s.nutSoSanhPC = nut("So sánh bật/tắt phòng chờ", widget.WarningImportance, s.soSanhPC)
	dauBang := moiHangBang([]string{"Phòng chờ", "Mua được", "Hết vé", "Quá tải", "Bỏ cuộc", "p99 ms"}, true)
	dauBang.nen.FillColor = XamNhat
	bang := container.NewVBox(dauBang.c)
	for range 2 {
		h := moiHangBang(make([]string, 6), false)
		s.bangPC = append(s.bangPC, h)
		bang.Add(h.c)
	}
	return container.NewVBox(dau, so, coDinh(s.thanhVe, 360, 14), s.nhanBan, s.nhanPhucVu, s.nhanXuLy, tocDo,
		coDinh(luoi, 400, 64), s.oPC, container.NewHBox(s.nutSoSanhPC), bang)
}

func (s *cuaSoServerPhongCho) thamSoDatLai() (tc.TS, error) {
	so := func(k string) (int64, error) { return strconv.ParseInt(strings.TrimSpace(s.oTS[k].Text), 10, 64) }
	tv, e1 := so("tong_ve")
	dl, e2 := so("dung_luong")
	ls, e3 := strconv.ParseFloat(strings.TrimSpace(s.oTS["luot_s"].Text), 64)
	gh, e4 := so("gioi_han_xu_ly")
	td, e5 := strconv.ParseFloat(strings.TrimSpace(s.oTocDo.Text), 64)
	if e1 != nil || e2 != nil || e3 != nil || e4 != nil || e5 != nil {
		return nil, fmt.Errorf("tham số phải là số")
	}
	pc := int64(0)
	if s.oPC.Checked {
		pc = 1
	}
	return tc.T("tong_ve", tv, "dung_luong", dl, "luot_s", ls, "gioi_han_xu_ly", gh, "toc_do", td, "phong_cho", pc), nil
}

// datThamSo – điền ô tham số / ô chọn (kịch bản demo) – như người dùng gõ.
func (s *cuaSoServerPhongCho) datThamSo(ts tc.TS) {
	for _, kv := range ts {
		switch kv.K {
		case "phong_cho":
			s.oPC.SetChecked(tc.SoNguyen(kv.V) != 0)
		case "toc_do":
			s.oTocDo.SetText(tc.StrPy(kv.V))
		default:
			s.oTS[kv.K].SetText(tc.StrPy(kv.V))
		}
	}
}

func (s *cuaSoServerPhongCho) doiTocDo() {
	v, err := strconv.ParseFloat(strings.TrimSpace(s.oTocDo.Text), 64)
	if err == nil {
		err = s.sv.DatTocDo(v)
	}
	if err != nil {
		s.log.them("Tốc độ không hợp lệ: " + err.Error())
	}
}

// ------------------------------------------------------------ so sánh bật/tắt phòng chờ

func (s *cuaSoServerPhongCho) soSanhPC() {
	n, k, _, err := s.docMoPhong()
	if err != nil {
		s.log.them(err.Error())
		return
	}
	if s.dangMoPhong || !s.lop.Chay() {
		s.log.them("Cần START SERVER và chờ lượt mô phỏng trước xong")
		return
	}
	s.dangMoPhong = true
	batTat(false, s.nutBan, s.nutSoSanh, s.nutSoSanhPC)
	datChu(s.nhanMoPhong, "đang chạy…", DoDam)
	i := strings.LastIndex(s.lop.DiaChi, ":")
	host, cong := s.lop.DiaChi[:i], soNguyen(s.lop.DiaChi[i+1:], 0)
	bao := func(text string) { s.log.them("[mô phỏng] " + text) }
	go func() {
		hang, err := tc.SoSanhPhongCho(host, cong, n, k, bao, "khoa")
		if err != nil {
			bao("lỗi: " + err.Error())
		}
		fyne.Do(func() {
			s.xongMoPhong(nil) // mở lại nút, "xong"
			s.nutSoSanhPC.Enable()
			s.hangPC = hang
			for i, h := range s.bangPC {
				if i >= len(hang) {
					h.dat(nil, color.Transparent, Den)
					continue
				}
				x := hang[i]
				p99 := "–"
				if x.P99Ms != nil {
					p99 = fmt.Sprintf("%.1f", *x.P99Ms)
				}
				gt := []string{x.PhongCho, fmt.Sprint(x.MuaDuoc), fmt.Sprint(x.HetVe), fmt.Sprint(x.QuaTai),
					fmt.Sprint(x.BoCuoc), p99}
				if x.QuaTai > 0 {
					h.dat(gt, mau("#ffc8c8"), DoDam)
				} else {
					h.dat(gt, mau("#d8f5d8"), Den)
				}
			}
			for _, d := range tc.BangPhongCho(hang) {
				s.log.them(strings.TrimSpace(d))
			}
		})
	}()
}

// ------------------------------------------------------------ vẽ lại định kỳ

func (s *cuaSoServerPhongCho) capNhatTaiNguyen(tt, _ tc.M) {
	batTat(!s.dangMoPhong, s.nutSoSanhPC)
	sk, _ := tt["su_kien"].(tc.M)
	tongVe, con := tc.SoNguyen(sk["tong_ve"]), tc.SoNguyen(tt["con_lai"])
	datChu(s.nhanVe, fmt.Sprintf("%d / %d", con, tongVe), mau("#8b0000"))
	s.thanhVe.Max = float64(max(1, tongVe))
	s.thanhVe.SetValue(float64(max(0, con)))
	if pc, _ := tt["phong_cho"].(bool); pc {
		datChu(s.nhanHang, fmt.Sprintf("%d người", tc.SoNguyen(tt["dang_cho"])), mau("#1f4fa0"))
	} else {
		datChu(s.nhanHang, "TẮT", mau("#808080"))
	}
	banSo := tc.SoNguyen(tt["ban_so"])
	ban := fmt.Sprintf("Sổ đã bán: %d vé, %d đơn", banSo, tc.SoNguyen(tt["so_don"]))
	m := Den
	if banSo > tongVe {
		ban += fmt.Sprintf(" – VƯỢT %d vé!", banSo-tongVe)
		m = DoDam
	}
	if het, _ := tt["het_ve"].(bool); het {
		ban += "  [HẾT VÉ]"
	}
	datChu(s.nhanBan, ban, m)
	datChu(s.nhanPhucVu, fmt.Sprintf("Đang phục vụ tới #%d / đã phát #%d · lượt đã cấp %d, hết hạn %d",
		tc.SoNguyen(tt["dang_phuc_vu"]), tc.SoNguyen(tt["so_cap"]), tc.SoNguyen(tt["da_cap_luot"]),
		tc.SoNguyen(tt["het_han"])), Den)
	m = XanhDam
	if tc.SoNguyen(tt["qua_tai"]) > 0 {
		m = DoDam
	}
	datChu(s.nhanXuLy, fmt.Sprintf("Mua đang xử lý: %d / %d (max %d) · từ chối quá tải: %d", tc.SoNguyen(tt["dang_xu_ly"]),
		tc.SoNguyen(tt["gioi_han"]), tc.SoNguyen(tt["xu_ly_max"]), tc.SoNguyen(tt["qua_tai"])), m)
	datChu(s.nhanToken, fmt.Sprintf("thùng %d/%d token", tc.SoNguyen(tt["token"]), tc.SoNguyen(tt["dung_luong"])), Den)
}
