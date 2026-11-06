package main

// Phần dòng lệnh dùng chung của nhóm MR6.6–MR6.14 (giống nhau ở 9 bài, như server.py/khach.py/
// mo_phong.py của bản Python): vòng lệnh của máy chủ, công cụ quản trị, bộ mô phỏng tải.

import (
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"time"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

// vongServer – in nhật ký, cứ 5 giây in một dòng số liệu (khi có yêu cầu mới), nhận lệnh bàn phím:
// che_do x, tre ms, dat_lai [k=v …], thong_ke, thoat. Ctrl+C (công cụ demo dừng server bằng SIGINT):
// in tổng kết rồi dừng.
func vongServer(sv *tc.MayChuCoSo, log func(string)) {
	xong := make(chan struct{}, 1)
	lenh := func(dong string) {
		p := strings.Fields(dong)
		log("> " + dong)
		var err error
		switch {
		case p[0] == "che_do" && len(p) == 2:
			err = sv.DatCheDo(p[1])
		case p[0] == "tre" && len(p) == 2:
			var ms float64
			if ms, err = strconv.ParseFloat(p[1], 64); err != nil {
				err = fmt.Errorf("could not convert string to float: '%s'", p[1])
			} else {
				err = sv.DatTre(ms)
			}
		case p[0] == "dat_lai":
			var ts tc.TS
			if ts, err = tc.DocThamSo(p[1:]); err == nil {
				err = sv.DatLai(ts)
			}
		case p[0] == "thong_ke":
			for _, d := range tc.DongThongKe(sv.ThongKeDayDu()) {
				log(d)
			}
		case p[0] == "thoat":
			xong <- struct{}{}
		default:
			log("lệnh không hợp lệ")
		}
		if err != nil {
			log("lỗi: " + err.Error())
		}
	}
	go tc.DocLenhStdin(time.Now(), lenh, nil)
	ngat := make(chan os.Signal, 1)
	signal.Notify(ngat, os.Interrupt)
	t := time.NewTicker(5 * time.Second)
	defer t.Stop()
	var daIn int64
vong:
	for {
		select {
		case <-t.C: // mỗi 5 s: một dòng số liệu nếu có yêu cầu mới
			tk := sv.ThongKeDayDu()
			if n := tc.SoNguyen(tk["so_yc"]); n != daIn {
				daIn = n
				log(fmt.Sprintf("%s: %d yc, tc %d, từ chối %d, sai phạm %d", tk["che_do"], n,
					tc.SoNguyen(tk["thanh_cong"]), tc.SoNguyen(tk["tu_choi"]), tc.SoNguyen(tk["sai_pham_tong"])))
			}
		case <-xong:
			break vong
		case <-ngat:
			break vong
		}
	}
	log("Tổng kết (từ lần đặt lại cuối):")
	for _, d := range tc.DongThongKe(sv.ThongKeDayDu()) {
		log("  " + d)
	}
	sv.Dung()
}

// chayQuanTri – out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke
func chayQuanTri(a []string) {
	if len(a) < 3 {
		fmt.Println("Dùng: out/app khach <host> <cổng> che_do <x> | tre <ms> | dat_lai k=v … | thong_ke")
		return
	}
	host, cong, lenh, ts := a[0], soCong(a[1]), a[2], a[3:]
	bao := func(text string) { inDong("quan_tri", text) }
	qt, err := tc.MoiQuanTri(host, cong, "quan_tri")
	if err != nil {
		bao("Không kết nối được: " + err.Error())
		os.Exit(1)
	}
	defer qt.Dong()
	bao("> " + strings.Join(append([]string{lenh}, ts...), " "))
	switch {
	case lenh == "che_do" && len(ts) == 1:
		err = qt.CheDo(ts[0])
	case lenh == "tre" && len(ts) == 1:
		var ms float64
		if ms, err = strconv.ParseFloat(ts[0], 64); err == nil {
			err = qt.Tre(ms)
		}
	case lenh == "dat_lai":
		var t tc.TS
		if t, err = tc.DocThamSo(ts); err == nil {
			err = qt.DatLai(t)
		}
	case lenh != "thong_ke":
		bao("lệnh không hợp lệ")
		return
	}
	if err != nil {
		bao("Bị từ chối: " + err.Error())
		return
	}
	bao("OK")
	if lenh == "thong_ke" {
		tk, err := qt.ThongKe()
		if err != nil {
			bao("Bị từ chối: " + err.Error())
			return
		}
		for _, d := range tc.DongThongKe(tk) {
			bao(d)
		}
	}
}

// chayMoPhong – out/app mo_phong [host] [cổng] [--n 200] [--k 200] [--lan …] [--che-do x] [--dat-lai]
// [--so-sanh] [--tre-ms 2]
func chayMoPhong(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("mo_phong")
	n := fs.Int("n", 200, "số người dùng ảo")
	k := fs.Int("k", 0, "số kết nối (mặc định min(n, 200))")
	lan := fs.Int("lan", tc.SoLanMacDinh, "số yêu cầu mỗi người")
	cheDo := fs.String("che-do", "", "đổi chế độ máy chủ trước khi chạy")
	datLai := fs.Bool("dat-lai", false, "đặt lại trạng thái trước khi chạy")
	soSanh := fs.Bool("so-sanh", false, "chạy cả 4 chế độ và in bảng so sánh")
	treS := fs.String("tre-ms", "", "đặt độ trễ xử lý tối đa của máy chủ")
	if err := fs.Parse(con); err != nil {
		fmt.Println("tham số sai:", err)
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	if *cheDo != "" && !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("--che-do: chọn một trong", strings.Join(tc.CheDoDS, ", "))
		os.Exit(2)
	}
	var treMs *float64
	if *treS != "" {
		v, err := strconv.ParseFloat(*treS, 64)
		if err != nil {
			fmt.Println("--tre-ms không hợp lệ")
			os.Exit(2)
		}
		treMs = &v
	}
	kk := *k
	if kk == 0 {
		kk = min(*n, 200)
	}
	bao := func(text string) { inDong("mo_phong", text) }
	if *soSanh {
		hang, err := tc.SoSanh(host, cong, *n, kk, *lan, bao, treMs)
		if err != nil {
			bao("lỗi: " + err.Error())
		}
		bao(fmt.Sprintf("Bảng so sánh (%d người × %d lần, %d kết nối):", *n, *lan, min(kk, *n)))
		for _, d := range tc.BangSoSanh(hang) {
			khoaIn.Lock()
			fmt.Println(d)
			khoaIn.Unlock()
		}
		return
	}
	if _, err := tc.ChayLuot(host, cong, *n, kk, *lan, bao, *cheDo, *datLai, treMs); err != nil {
		bao("lỗi: " + err.Error())
	}
}

func coTrong(ds []string, x string) bool {
	for _, v := range ds {
		if v == x {
			return true
		}
	}
	return false
}
