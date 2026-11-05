package main

// Người tham gia đấu giá (bản dòng lệnh). Kết nối máy chủ, in diễn biến phiên (giá mới, bị trả giá cao
// hơn, gia hạn, người thắng) và nhận lệnh:
//
//	tra <giá>       trả đúng giá này                tang [n]    trả giá hiện tại + n bước giá (mặc định 1)
//	thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app dau_gia <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.6_DauGia/tranhchap"
)

type nguoiThamGia struct {
	ten  string
	khoa sync.Mutex
	tt   tc.M // cap_nhat mới nhất
	kn   *tc.KetNoiKhach
}

func (ng *nguoiThamGia) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiThamGia) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	con := "không giới hạn"
	if tt["con_lai_ms"] != nil {
		con = fmt.Sprintf("còn %.1f s", tc.SoThuc(tt["con_lai_ms"])/1000)
	}
	mon := tt["mon"].(tc.M)
	ng.log(fmt.Sprintf("Vào phòng: %s (%s)", tc.Chuoi(mon["ten"]), tc.Chuoi(tt["che_do"])))
	ng.log(fmt.Sprintf("  giá %s, bước %s, %s", tc.Tien(tc.SoNguyen(tt["gia"])), tc.Tien(tc.SoNguyen(mon["buoc_gia"])), con))
	ng.kn.BatDauDoc()
	return nil
}

// ------------------------------------------------------------ nhận

func (ng *nguoiThamGia) capNhat(tt tc.M) {
	ng.khoa.Lock()
	cu := ng.tt
	ng.tt = tt
	ng.khoa.Unlock()
	if tc.SoNguyen(tt["gia"]) != tc.SoNguyen(cu["gia"]) || tt["nguoi_dan_dau"] != cu["nguoi_dan_dau"] {
		ng.log(fmt.Sprintf("Giá mới %s – %s dẫn đầu", tc.Tien(tc.SoNguyen(tt["gia"])), tc.StrPy(tt["nguoi_dan_dau"])))
		if cu["nguoi_dan_dau"] == ng.ten && tt["nguoi_dan_dau"] != ng.ten {
			ng.log("  Bạn bị trả giá cao hơn!")
		}
	}
	if kt, _ := tt["ket_thuc"].(bool); kt {
		if ktCu, _ := cu["ket_thuc"].(bool); !ktCu {
			if tt["nguoi_dan_dau"] == ng.ten {
				ng.log("  BẠN THẮNG!")
			} else {
				ng.log("  Bạn không thắng phiên này")
			}
		}
	}
}

func (ng *nguoiThamGia) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

// ------------------------------------------------------------ gửi

func (ng *nguoiThamGia) tra(gia int64, ghiChu string) {
	ng.log(fmt.Sprintf("> trả %s%s", tc.Tien(gia), ghiChu))
	_, err := ng.kn.GuiYC("tra_gia", func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			them := ""
			if gh, _ := tin["gia_han"].(bool); gh {
				them = ", phiên được GIA HẠN"
			}
			ng.log(fmt.Sprintf("  CHẤP NHẬN (%.1f ms)%s", rtt, them))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	}, tc.T("gia", gia))
	if err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiThamGia) lenh(dong string) {
	p := strings.Fields(dong)
	switch {
	case p[0] == "tra" && len(p) == 2:
		gia, err := strconv.ParseInt(p[1], 10, 64)
		if err != nil {
			ng.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[1]))
			return
		}
		ng.tra(gia, "")
	case p[0] == "tang" && len(p) <= 2:
		n := int64(1)
		if len(p) == 2 {
			var err error
			if n, err = strconv.ParseInt(p[1], 10, 64); err != nil {
				ng.log(fmt.Sprintf("lỗi: invalid literal for int() with base 10: '%s'", p[1]))
				return
			}
		}
		ng.khoa.Lock()
		gia := tc.SoNguyen(ng.tt["gia"]) + n*tc.SoNguyen(ng.tt["mon"].(tc.M)["buoc_gia"])
		ng.khoa.Unlock()
		ng.tra(gia, fmt.Sprintf(" (+%d bước)", n))
	case p[0] == "thoat":
		ng.kn.TatBaoDong()
		ng.kn.Dong()
	default:
		ng.log("lệnh: tra <giá> | tang [n] | thoat")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("dau_gia")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app dau_gia <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiThamGia{ten: pos[0]}
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
