package main

// Người đặt vé xem phim (bản dòng lệnh). Kết nối máy chủ, in diễn biến (ghế mình giữ hết hạn, bị người
// khác lấy) và nhận lệnh:
//
//	giu E7 [E8 …]   giữ chỗ (có thời hạn)          tt [E7 …]    thanh toán (mặc định: các ghế vừa giữ)
//	nha [E7 …]      nhả ghế đang giữ               xem          in sơ đồ ghế          thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app dat_ve <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

// soDoChu – các dòng sơ đồ ghế: . trống, g người khác giữ, b đã bán, G mình giữ, V vé của mình.
func soDoChu(tt tc.M, ten string) []string {
	cot := int(tc.SoNguyen(tt["cot"]))
	so := make([]string, cot)
	for c := 1; c <= cot; c++ {
		so[c-1] = strconv.Itoa(c % 10)
	}
	ds := []string{"      [ MÀN HÌNH ]", "   " + strings.Join(so, " ")}
	chu, _ := tt["chu"].(tc.M)
	soDo := tc.DSChuoi(tt["so_do"])
	for i, r := range []rune(tc.Chuoi(tt["hang"])) {
		if i >= len(soDo) {
			break
		}
		var o []string
		for c, x := range soDo[i] {
			s := string(x)
			if chu[fmt.Sprintf("%c%d", r, c+1)] == ten {
				s = map[string]string{"g": "G", "b": "V"}[s]
			}
			o = append(o, s)
		}
		ds = append(ds, fmt.Sprintf("%c  %s", r, strings.Join(o, " ")))
	}
	return ds
}

type nguoiDatVe struct {
	ten      string
	khoa     sync.Mutex
	tt       tc.M            // cap_nhat mới nhất
	gioHang  []string        // các ghế vừa giữ được (để "tt" không cần gõ lại)
	phienGio int64           // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
	daBao    map[string]bool // ghế trong giỏ đã báo hết hạn / bị lấy
	kn       *tc.KetNoiKhach
}

func (ng *nguoiDatVe) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiDatVe) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	d := tt["dem"].(tc.M)
	ng.log(fmt.Sprintf("Vào rạp: %s, suất %s (%s)", tc.Chuoi(tt["phim"]), tc.Chuoi(tt["suat"]), tc.Chuoi(tt["che_do"])))
	ng.log(fmt.Sprintf("  trống %d, giữ %d, bán %d; vé %s, giữ %s s", tc.SoNguyen(d["trong"]), tc.SoNguyen(d["giu"]),
		tc.SoNguyen(d["ban"]), tc.Tien(tc.SoNguyen(tt["gia_ve"])), tc.FmtG(tc.SoThuc(tt["ttl_s"]))))
	ng.kn.BatDauDoc()
	return nil
}

// capNhat – ghế trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin
// cap_nhat chụp SAU lần giữ của mình (tt["phien"] ≥ phien của kq). Giỏ vẫn giữ nguyên – lệnh "tt" sau
// đó sẽ bị máy chủ từ chối, đúng như khi trang thanh toán mở từ trước bấm "Thanh toán" quá hạn.
func (ng *nguoiDatVe) capNhat(tt tc.M) {
	ng.khoa.Lock()
	ng.tt = tt
	var gio []string
	for _, g := range ng.gioHang {
		if !ng.daBao[g] {
			gio = append(gio, g)
		}
	}
	cu := tc.SoNguyen(tt["phien"]) < ng.phienGio
	ng.khoa.Unlock()
	if cu {
		return
	}
	chu, _ := tt["chu"].(tc.M)
	for _, g := range gio {
		ai, _ := chu[g].(string)
		if ai == ng.ten {
			continue
		}
		if ai == "" {
			ng.log(fmt.Sprintf("  Hết thời gian giữ chỗ: %s đã được nhả", g))
		} else {
			ng.log(fmt.Sprintf("  Ghế %s đã bị %s lấy mất!", g, ai))
		}
		ng.khoa.Lock()
		ng.daBao[g] = true
		ng.khoa.Unlock()
	}
}

func (ng *nguoiDatVe) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

func boGhe(ds, bo []string) []string {
	var kq []string
	for _, g := range ds {
		gap := false
		for _, b := range bo {
			gap = gap || g == b
		}
		if !gap {
			kq = append(kq, g)
		}
	}
	return kq
}

func (ng *nguoiDatVe) gui(loai string, ghe []string, kq func(tin tc.M, rtt float64)) {
	if _, err := ng.kn.GuiYC(loai, kq, tc.T("ghe", ghe)); err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiDatVe) giu(ghe []string) {
	ng.log("> giữ " + strings.Join(ghe, ", "))
	ng.gui("giu_cho", ghe, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = tc.DSChuoi(tin["ghe"])
			ng.phienGio, ng.daBao = tc.SoNguyen(tin["phien"]), map[string]bool{}
			ng.khoa.Unlock()
			ng.log(fmt.Sprintf("  ĐÃ GIỮ (%.1f ms), thanh toán trong %.0f s", rtt, tc.SoThuc(tin["het_han_ms"])/1000))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVe) thanhToan(ghe []string) {
	ng.log("> thanh toán " + strings.Join(ghe, ", "))
	ng.gui("thanh_toan", ghe, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = boGhe(ng.gioHang, tc.DSChuoi(tin["ghe"]))
			ng.khoa.Unlock()
			ng.log(fmt.Sprintf("  ĐẶT VÉ THÀNH CÔNG (%.1f ms): %s", rtt, tc.Tien(tc.SoNguyen(tin["tien"]))))
			ng.log("  mã vé " + strings.Join(tc.DSChuoi(tin["ma_ve"]), ", "))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVe) nha(ghe []string) {
	ng.log("> nhả " + strings.Join(ghe, ", "))
	ng.gui("nha", ghe, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = boGhe(ng.gioHang, tc.DSChuoi(tin["ghe"]))
			ng.khoa.Unlock()
			ng.log("  Đã nhả " + strings.Join(tc.DSChuoi(tin["ghe"]), ", "))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVe) lenh(dong string) {
	p := strings.Fields(dong)
	ng.khoa.Lock()
	gio := append([]string(nil), ng.gioHang...)
	ng.khoa.Unlock()
	hoa := func(ds []string) []string {
		var kq []string
		for _, g := range ds {
			kq = append(kq, strings.ToUpper(g))
		}
		return kq
	}
	switch {
	case p[0] == "giu" && len(p) >= 2:
		ng.giu(hoa(p[1:]))
	case p[0] == "tt" || p[0] == "nha":
		ghe := hoa(p[1:])
		if len(ghe) == 0 {
			ghe = gio
		}
		switch {
		case len(ghe) == 0:
			ng.log("chưa giữ ghế nào")
		case p[0] == "tt":
			ng.thanhToan(ghe)
		default:
			ng.nha(ghe)
		}
	case p[0] == "xem":
		ng.khoa.Lock()
		tt := ng.tt
		ng.khoa.Unlock()
		for _, d := range soDoChu(tt, ng.ten) {
			ng.log(d)
		}
	case p[0] == "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: giu E7 [E8 …] | tt [ghế…] | nha [ghế…] | xem | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("dat_ve")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app dat_ve <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiDatVe{ten: pos[0], daBao: map[string]bool{}}
	if err := ng.ketNoi(lay(pos, 1, "127.0.0.1"), soCong(lay(pos, 2, strconv.Itoa(congMacDinh)))); err != nil {
		ng.log("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	hetStdin := make(chan struct{})
	go tc.DocLenhStdin(t0, ng.lenh, func() { close(hetStdin) })
	if *thoiGian > 0 { // chạy đúng --thoi-gian giây (hoặc tới khi thoat)
		for ng.kn.Mo() && time.Since(t0).Seconds() < *thoiGian {
			time.Sleep(100 * time.Millisecond)
		}
	} else { // tới khi hết stdin / thoat
	cho:
		for ng.kn.Mo() {
			select {
			case <-hetStdin:
				break cho
			case <-time.After(100 * time.Millisecond):
			}
		}
		time.Sleep(300 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	ng.kn.TatBaoDong()
	ng.kn.Dong()
}
