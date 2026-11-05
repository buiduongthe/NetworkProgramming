package main

// Người tham gia vòng quay may mắn (bản dòng lệnh). Kết nối máy chủ, in diễn biến (ai trúng gì, số quà
// còn lại) và nhận lệnh:
//
//	quay        quay một lượt
//	quay x2     "bấm đúp": gửi HAI yêu cầu quay liên tiếp không chờ (hai ma_yc khác nhau) – máy chủ phải
//	            chấp nhận đúng một (idempotency theo tên người)
//	gui_lai [k] gửi lại ĐÚNG ma_yc k (mặc định: của lượt quay gần nhất), như khi mạng chập chờn client gửi
//	            lại – máy chủ không quay lại mà trả lại kết quả cũ kèm "lap_lai": true (idempotency theo ma_yc)
//	thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app quay_so <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.9_QuaySo/tranhchap"
)

func moTaKq(tin tc.M) string {
	if ok, _ := tin["ok"].(bool); ok {
		if trung, _ := tin["trung"].(bool); trung {
			return "TRÚNG " + tc.Chuoi(tin["ten_qua"]) + "!"
		}
		return "Chúc may mắn lần sau"
	}
	return "TỪ CHỐI: " + tc.StrPy(tin["ly_do"])
}

type nguoiQuay struct {
	ten    string
	khoa   sync.Mutex
	tt     tc.M
	maCuoi int64 // ma_yc của lượt quay gần nhất (cho gui_lai)
	kn     *tc.KetNoiKhach
}

func (ng *nguoiQuay) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiQuay) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	ng.log(fmt.Sprintf("Vào vòng quay (%s): còn %d/%d quà, p %s", tc.Chuoi(tt["che_do"]), tc.SoNguyen(tt["con_lai"]),
		tc.SoNguyen(tt["so_qua"]), tc.ChuP(tc.SoThuc(tt["p"]))))
	ng.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (ng *nguoiQuay) capNhat(tt tc.M) {
	ng.khoa.Lock()
	cu := ng.tt
	ng.tt = tt
	ng.khoa.Unlock()
	if tc.SoNguyen(tt["con_lai"]) != tc.SoNguyen(cu["con_lai"]) {
		ng.log(fmt.Sprintf("  còn %d/%d quà", tc.SoNguyen(tt["con_lai"]), tc.SoNguyen(tt["so_qua"])))
	}
}

func (ng *nguoiQuay) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	} else if lap, _ := tin["lap_lai"].(bool); tin["loai"] == "kq" && lap {
		ng.log(fmt.Sprintf("  (ma_yc %s, lap_lai) %s", tc.StrPy(tin["ma_yc"]), moTaKq(tin)))
	}
}

// ------------------------------------------------------------ gửi

func (ng *nguoiQuay) quay() {
	ma, err := ng.kn.GuiYC("quay", func(tin tc.M, rtt float64) {
		ng.log(fmt.Sprintf("  ma_yc %s: %s (%.1f ms)", tc.StrPy(tin["ma_yc"]), moTaKq(tin), rtt))
	}, nil)
	if err != nil {
		ng.log("lỗi: " + err.Error())
		return
	}
	ng.khoa.Lock()
	ng.maCuoi = ma
	ng.khoa.Unlock()
}

func (ng *nguoiQuay) guiLai(ma int64) {
	if ma == 0 {
		ng.khoa.Lock()
		ma = ng.maCuoi
		ng.khoa.Unlock()
	}
	if ma == 0 {
		ng.log("chưa quay lượt nào")
		return
	}
	ng.log(fmt.Sprintf("> gửi lại ma_yc %d", ma))
	if err := ng.kn.GuiTho(tc.T("loai", "quay", "ma_yc", ma)); err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiQuay) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case len(p) == 1 && p[0] == "quay":
		ng.log("> quay")
		ng.quay()
	case len(p) == 2 && p[0] == "quay" && p[1] == "x2":
		ng.log("> quay x2 (bấm đúp: 2 yêu cầu liền nhau)")
		ng.quay()
		ng.quay()
	case p[0] == "gui_lai" && len(p) <= 2:
		var ma int64
		if len(p) == 2 {
			n, err := strconv.ParseInt(p[1], 10, 64)
			if err != nil {
				ng.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[1]))
				return
			}
			ma = n
		}
		ng.guiLai(ma)
	case p[0] == "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: quay | quay x2 | gui_lai [k] | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("quay_so")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app quay_so <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiQuay{ten: pos[0]}
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
		time.Sleep(500 * time.Millisecond) // chờ trả lời của lệnh cuối
	}
	ng.kn.TatBaoDong()
	ng.kn.Dong()
}
