package main

// Người đặt vé xe khách (bản dòng lệnh). Kết nối máy chủ, in diễn biến (chỗ mình giữ hết hạn, bị người khác
// lấy) và nhận lệnh:
//
//	giu SG DL D01 [D02 …]   giữ chỗ đoạn Sài Gòn → Đà Lạt (mã trạm SG, BL, DL, NT; có thời hạn)
//	tt                      thanh toán các chỗ vừa giữ          nha       nhả các chỗ vừa giữ
//	xem                     in sơ đồ xe (mỗi chỗ 3 ký tự, một ký tự mỗi chặng)            thoat
//
// Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chạy (dùng trong kịch bản demo).
//
//	out/app dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]     (0 = chạy tới khi hết stdin/thoat)

import (
	"fmt"
	"os"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.14_DatVeXeKhach/tranhchap"
)

// soDoChu – các dòng sơ đồ xe. Mỗi chỗ: mã rồi một ký tự mỗi chặng (. trống, g người khác giữ, b đã bán,
// G mình giữ, V vé của mình).
func soDoChu(tt tc.M, ten string) []string {
	chu, _ := tt["chu"].(tc.M)
	ghe, soDo := tc.DSChuoi(tt["ghe"]), tc.DSChuoi(tt["so_do"])
	var boTri [][3]int
	soTang, soCot := 1, 1
	for _, v := range tt["bo_tri"].([]any) {
		x := v.([]any)
		b := [3]int{int(tc.SoNguyen(x[0])), int(tc.SoNguyen(x[1])), int(tc.SoNguyen(x[2]))}
		boTri = append(boTri, b)
		soTang, soCot = max(soTang, b[0]+1), max(soCot, b[2]+1)
	}
	o := map[string]string{}
	rong := 0
	for i, g := range ghe {
		var b strings.Builder
		for k, x := range soDo[i] {
			s := string(x)
			if chu[fmt.Sprintf("%s#%d", g, k)] == ten {
				s = map[string]string{"g": "G", "b": "V"}[s]
			}
			b.WriteString(s)
		}
		o[g] = g + ":" + b.String()
		rong = max(rong, len(o[g]))
	}
	var chang []string
	for k := 0; k < tc.SoChang; k++ {
		chang = append(chang, tc.MaTram[k]+"-"+tc.MaTram[k+1])
	}
	ds := []string{"  " + tc.Chuoi(tt["ten_xe"]) + " – chặng: " + strings.Join(chang, " | ")}
	for tang := 0; tang < soTang; tang++ {
		if soTang > 1 {
			ds = append(ds, map[int]string{0: "  Tầng dưới:", 1: "  Tầng trên:"}[tang])
		}
		hang := map[int]map[int]string{}
		var thuTu []int
		for i, b := range boTri {
			if b[0] != tang {
				continue
			}
			if hang[b[1]] == nil {
				hang[b[1]] = map[int]string{}
				thuTu = append(thuTu, b[1])
			}
			hang[b[1]][b[2]] = o[ghe[i]]
		}
		slices.Sort(thuTu)
		for _, h := range thuTu {
			var p []string
			for c := 0; c < soCot; c++ {
				x := hang[h][c]
				p = append(p, x+strings.Repeat(" ", rong-len(x)))
			}
			ds = append(ds, strings.TrimRight("   "+strings.Join(p, " "), " "))
		}
	}
	return ds
}

type gio struct {
	ghe     []string
	tu, den int
}

type nguoiDatVeXe struct {
	ten      string
	khoa     sync.Mutex
	tt       tc.M            // cap_nhat mới nhất
	gioHang  *gio            // chỗ vừa giữ được (để "tt"/"nha" không cần gõ lại)
	phienGio int64           // số thứ tự lần ghi (kq) của lần giữ chỗ gần nhất
	daBao    map[string]bool // ô trong giỏ đã báo hết hạn / bị lấy
	kn       *tc.KetNoiKhach
}

func (ng *nguoiDatVeXe) log(text string) { inDong(ng.ten, text) }

func (ng *nguoiDatVeXe) ketNoi(host string, cong int) error {
	ng.kn = tc.MoiKetNoiKhach()
	ng.kn.KhiCapNhat, ng.kn.KhiTin, ng.kn.KhiDong = ng.capNhat, ng.tinKhac, func() { ng.log("Mất kết nối máy chủ") }
	if _, err := ng.kn.KetNoi(host, cong, ng.ten, "nguoi_dung", 0, 5); err != nil {
		return err
	}
	tt := ng.kn.TrangThai()
	ng.tt = tt
	d := tt["dem"].(tc.M)
	ng.log(fmt.Sprintf("Vào %s: chuyến %s, %s (%s)", tc.Chuoi(tt["nha_xe"]), tc.Chuoi(tt["chuyen"]),
		strings.ToLower(tc.Chuoi(tt["ten_xe"])), tc.Chuoi(tt["che_do"])))
	ng.log(fmt.Sprintf("  tuyến %s; chỗ-chặng trống %d, giữ %d, bán %d; giữ chỗ %s s",
		strings.Join(tc.DSChuoi(tt["tram"]), " – "), tc.SoNguyen(d["trong"]), tc.SoNguyen(d["giu"]),
		tc.SoNguyen(d["ban"]), tc.FmtG(tc.SoThuc(tt["ttl_s"]))))
	ng.kn.BatDauDoc()
	return nil
}

// capNhat – ô trong giỏ không còn mang tên mình: hết hạn (trống) hoặc bị người khác lấy. Chỉ tin cap_nhat
// chụp SAU lần giữ của mình (tt["phien"] ≥ phien của kq).
func (ng *nguoiDatVeXe) capNhat(tt tc.M) {
	ng.khoa.Lock()
	ng.tt = tt
	g := ng.gioHang
	cu := tc.SoNguyen(tt["phien"]) < ng.phienGio
	var o []string
	if g != nil && !cu {
		for _, x := range g.ghe {
			for k := g.tu; k < g.den; k++ {
				if ten := fmt.Sprintf("%s#%d", x, k); !ng.daBao[ten] {
					o = append(o, ten)
				}
			}
		}
	}
	ng.khoa.Unlock()
	chu, _ := tt["chu"].(tc.M)
	var thuTu []string
	bao := map[string]string{}
	for _, x := range o {
		ai, _ := chu[x].(string)
		if ai == ng.ten {
			continue
		}
		cho, _, _ := strings.Cut(x, "#")
		if _, co := bao[cho]; !co {
			bao[cho] = ai
			thuTu = append(thuTu, cho)
		}
		ng.khoa.Lock()
		ng.daBao[x] = true
		ng.khoa.Unlock()
	}
	for _, cho := range thuTu {
		if ai := bao[cho]; ai == "" {
			ng.log(fmt.Sprintf("  Hết thời gian giữ chỗ: %s đã được nhả", cho))
		} else {
			ng.log(fmt.Sprintf("  Chỗ %s đã bị %s lấy mất!", cho, ai))
		}
	}
}

func (ng *nguoiDatVeXe) tinKhac(tin tc.M) {
	if tin["loai"] == "su_kien" {
		ng.log("* " + tc.Chuoi(tin["noi_dung"]))
	}
}

func (ng *nguoiDatVeXe) gui(loai string, g gio, kq func(tin tc.M, rtt float64)) {
	if _, err := ng.kn.GuiYC(loai, kq, tc.T("ghe", g.ghe, "tu", int64(g.tu), "den", int64(g.den))); err != nil {
		ng.log("lỗi: " + err.Error())
	}
}

func (ng *nguoiDatVeXe) giu(g gio) {
	ng.log("> giữ " + strings.Join(g.ghe, ", ") + " " + tc.TenDoan(g.tu, g.den))
	ng.gui("giu_cho", g, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = &gio{tc.DSChuoi(tin["ghe"]), g.tu, g.den}
			ng.phienGio, ng.daBao = tc.SoNguyen(tin["phien"]), map[string]bool{}
			ng.khoa.Unlock()
			ng.log(fmt.Sprintf("  ĐÃ GIỮ (%.1f ms), %s, thanh toán trong %.0f s", rtt, tc.Tien(tc.SoNguyen(tin["tien"])),
				tc.SoThuc(tin["het_han_ms"])/1000))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVeXe) thanhToan(g gio) {
	ng.log("> thanh toán " + strings.Join(g.ghe, ", ") + " " + tc.TenDoan(g.tu, g.den))
	ng.gui("thanh_toan", g, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = nil
			ng.khoa.Unlock()
			ng.log(fmt.Sprintf("  ĐẶT VÉ THÀNH CÔNG (%.1f ms): %s", rtt, tc.Tien(tc.SoNguyen(tin["tien"]))))
			ng.log("  mã vé " + strings.Join(tc.DSChuoi(tin["ma_ve"]), ", "))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVeXe) nha(g gio) {
	ng.log("> nhả " + strings.Join(g.ghe, ", ") + " " + tc.TenDoan(g.tu, g.den))
	ng.gui("nha", g, func(tin tc.M, rtt float64) {
		if ok, _ := tin["ok"].(bool); ok {
			ng.khoa.Lock()
			ng.gioHang = nil
			ng.khoa.Unlock()
			ng.log("  Đã nhả " + strings.Join(tc.DSChuoi(tin["ghe"]), ", "))
		} else {
			ng.log("  TỪ CHỐI: " + tc.StrPy(tin["ly_do"]))
		}
	})
}

func (ng *nguoiDatVeXe) lenh(dong string) {
	p := strings.Fields(dong)
	ng.khoa.Lock()
	g := ng.gioHang
	ng.khoa.Unlock()
	tram := func(s string) int { return slices.Index(tc.MaTram, strings.ToUpper(s)) }
	switch {
	case p[0] == "giu" && len(p) >= 4 && tram(p[1]) >= 0 && tram(p[2]) >= 0:
		var ghe []string
		for _, x := range p[3:] {
			ghe = append(ghe, strings.ToUpper(x))
		}
		ng.giu(gio{ghe, tram(p[1]), tram(p[2])})
	case p[0] == "tt" || p[0] == "nha":
		switch {
		case g == nil:
			ng.log("chưa giữ chỗ nào")
		case p[0] == "tt":
			ng.thanhToan(*g)
		default:
			ng.nha(*g)
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
		ng.log("lệnh: giu SG DL D01 [D02 …] | tt | nha | xem | thoat   (trạm: " + strings.Join(tc.MaTram, ", ") + ")")
	}
}

func chayNguoiDung(a []string) {
	pos, con := viTri(a, 3)
	fs := boTuyChon("dat_ve_xe")
	thoiGian := fs.Float64("thoi-gian", 0, "số giây chạy (0 = tới khi hết stdin)")
	if err := fs.Parse(con); err != nil || len(pos) < 1 {
		fmt.Println("Dùng: out/app dat_ve_xe <tên> [host] [cổng] [--thoi-gian 0]")
		os.Exit(2)
	}
	t0 := time.Now()
	ng := &nguoiDatVeXe{ten: pos[0], daBao: map[string]bool{}}
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
