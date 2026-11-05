package main

// Người mua vé (bản dòng lệnh). Kết nối máy chủ, in thông tin sự kiện, vị trí trong phòng chờ (số người
// phía trước thay đổi theo cap_nhat), lúc tới lượt và kết quả mua; nhận lệnh:
//
//	xep_hang        vào phòng chờ (rồi tự "chờ dài" cho_luot tới khi tới lượt)
//	mua <n>         mua n vé          mua2 <n>   bấm ĐÚP: gửi hai yêu cầu mua liền nhau        thoat
//
// --tu-dong: vào hàng ngay (phòng chờ tắt thì mua ngay), tới lượt thì tự mua --so-ve vé.
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.13_PhongCho/tranhchap"
)

type nguoiMuaVe struct {
	ten     string
	tuDong  bool
	soVe    int64
	khoa    sync.Mutex
	tt      tc.M
	so      *int64 // số thứ tự trong hàng
	truocCu *int64
	kn      *tc.KetNoiKhach
}

func (ng *nguoiMuaVe) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiMuaVe) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	sk, _ := tt["su_kien"].(tc.M)
	ng.log(fmt.Sprintf("Trang bán vé: %s (%s)", tc.Chuoi(sk["ten"]), tc.Chuoi(tt["che_do"])))
	ng.log(fmt.Sprintf("  còn %d/%d vé, %s/vé", tc.SoNguyen(tt["con_lai"]), tc.SoNguyen(sk["tong_ve"]),
		tc.Tien(tc.SoNguyen(sk["gia_ve"]))))
	if pc, _ := tt["phong_cho"].(bool); pc {
		ng.log("  phòng chờ BẬT, cấp lượt " + tc.FmtG(tc.SoThuc(tt["toc_do"])) + " người/s")
	} else {
		ng.log("  phòng chờ TẮT")
	}
	ng.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (ng *nguoiMuaVe) capNhat(tt tc.M) {
	ng.khoa.Lock()
	ng.tt = tt
	if ng.so == nil || ng.truocCu == nil {
		ng.khoa.Unlock()
		return
	}
	truoc := max(0, *ng.so-tc.SoNguyen(tt["dang_phuc_vu"])-1)
	doi := truoc != *ng.truocCu
	ng.truocCu = &truoc
	ng.khoa.Unlock()
	if doi && truoc > 0 {
		ng.log(fmt.Sprintf("  (hàng chờ) còn %d người phía trước, ~%.1f s", truoc, float64(truoc+1)/tc.SoThuc(tt["toc_do"])))
	}
}

func (ng *nguoiMuaVe) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (ng *nguoiMuaVe) guiYC(loai string, khiKq func(tc.M, float64), truong tc.TS) {
	if _, err := ng.kn.GuiYC(loai, khiKq, truong); err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiMuaVe) xepHang() {
	ng.log("> vào phòng chờ")
	denLuot := func(tin tc.M, rtt float64) {
		ng.khoa.Lock()
		ng.truocCu = nil
		ng.khoa.Unlock()
		if ok, _ := tin["ok"].(bool); !ok {
			ng.log("  " + tc.StrPy(tin["ly_do"]))
			return
		}
		ng.log(fmt.Sprintf("  ĐẾN LƯỢT! (chờ %.1f s) còn %.1f s để mua", rtt/1000, tc.SoThuc(tin["han_ms"])/1000))
		if ng.tuDong {
			ng.mua(ng.soVe, 1)
		}
	}
	ng.guiYC("xep_hang", func(tin tc.M, _ float64) {
		if ok, _ := tin["ok"].(bool); !ok {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
			return
		}
		so, truoc := tc.SoNguyen(tin["so"]), tc.SoNguyen(tin["truoc"])
		ng.khoa.Lock()
		ng.so, ng.truocCu = &so, &truoc
		ng.khoa.Unlock()
		ng.log(fmt.Sprintf("  số thứ tự #%d, còn %d người phía trước, ~%.1f s", so, truoc, tc.SoThuc(tin["cho_s"])))
		ng.guiYC("cho_luot", denLuot, nil)
	}, nil)
}

func (ng *nguoiMuaVe) mua(n int64, lan int) {
	them := ""
	if lan > 1 {
		them = " (bấm đúp)"
	}
	ng.log(fmt.Sprintf("> mua %d vé%s", n, them))
	kq := func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.log(fmt.Sprintf("  THÀNH CÔNG (%.1f ms): đơn #%d, %d vé, %s", rtt, tc.SoNguyen(tin["ma_don"]),
				tc.SoNguyen(tin["so_ve"]), tc.Tien(tc.SoNguyen(tin["thanh_tien"]))))
		} else {
			ng.log(fmt.Sprintf("  TỪ CHỐI (%.1f ms): %s", rtt, tc.StrPy(tin["ly_do"])))
		}
	}
	for range lan { // bấm đúp: hai yêu cầu, không chờ trả lời
		ng.guiYC("mua", kq, tc.T("so_ve", n))
	}
}

func (ng *nguoiMuaVe) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case p[0] == "xep_hang":
		ng.xepHang()
	case (p[0] == "mua" || p[0] == "mua2") && len(p) == 2:
		n, err := strconv.ParseInt(p[1], 10, 64)
		if err != nil {
			ng.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[1]))
			return
		}
		lan := 1
		if p[0] == "mua2" {
			lan = 2
		}
		ng.mua(n, lan)
	case p[0] == "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: xep_hang | mua <n> | mua2 <n> | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("mua_ve")
	tuDong := fs.Bool("tu-dong", false, "vào hàng ngay, tới lượt thì tự mua")
	soVe := fs.Int64("so-ve", 2, "số vé tự mua")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app mua_ve <tên> [host] [cổng] [--tu-dong] [--so-ve 2] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiMuaVe{ten: pos[0], tuDong: *tuDong, soVe: *soVe}
	if err := ng.ketNoi(lay(pos, 1, "127.0.0.1"), soCong(lay(pos, 2, strconv.Itoa(congMacDinh)))); err != nil {
		ng.log("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	if *tuDong {
		if pc, _ := ng.tt["phong_cho"].(bool); pc {
			ng.xepHang()
		} else {
			ng.mua(*soVe, 1)
		}
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
