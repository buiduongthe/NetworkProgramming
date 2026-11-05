package main

// Máy chủ bán vé có phòng chờ ảo (bản dòng lệnh). Lõi ở tranhchap (MayChuPhongCho, khung MayChuCoSo –
// dùng chung với ứng dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--tong-ve 500] [--toc-do 50]
//	               [--dung-luong 50] [--luot-s 20] [--gioi-han 64] [--tat-phong-cho]
//
// Lệnh stdin: che_do x, tre <ms>, toc_do <người/s> (đổi tốc độ cấp lượt ngay, không đặt lại), dat_lai
// [k=v …] (tong_ve, ve_toi_da, gia_ve, ten_su_kien, phong_cho (0/1), toc_do, dung_luong, luot_s,
// gioi_han_xu_ly), thong_ke, thoat. Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.13_PhongCho/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	tongVe := fs.Int64("tong-ve", 500, "tổng số vé")
	tocDo := fs.Float64("toc-do", 50.0, "tốc độ cấp lượt (người/s)")
	dungLuong := fs.Int64("dung-luong", 50, "dung lượng thùng token")
	luotS := fs.Float64("luot-s", 20.0, "thời hạn một lượt mua (s)")
	gioiHan := fs.Int64("gioi-han", 64, "số yêu cầu mua đang xử lý tối đa")
	tat := fs.Bool("tat-phong-cho", false, "tắt phòng chờ: mọi người mua trực tiếp")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	pc, chuPC := int64(1), "BẬT"
	if *tat {
		pc, chuPC = 0, "TẮT"
	}
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("tong_ve", *tongVe, "toc_do", *tocDo, "dung_luong", *dungLuong,
		"luot_s", *luotS, "gioi_han_xu_ly", *gioiHan, "phong_cho", pc))
	if err != nil {
		log("lỗi: " + err.Error())
		os.Exit(2)
	}
	if err := sv.BatDau(host, cong); err != nil {
		log("Không khởi động được: " + err.Error())
		os.Exit(1)
	}
	log(fmt.Sprintf("Phòng chờ %s: %d vé, cấp lượt %s người/s", chuPC, *tongVe, tc.FmtG(*tocDo)))
	lenhRieng = func(p []string) (bool, error) { // toc_do <người/s>
		if p[0] != "toc_do" || len(p) != 2 {
			return false, nil
		}
		v, err := strconv.ParseFloat(p[1], 64)
		if err != nil {
			return true, fmt.Errorf("could not convert string to float: '%s'", p[1])
		}
		return true, sv.DatTocDo(v)
	}
	vongServer(sv.MayChuCoSo, log)
}

// chaySoSanhPhongCho – out/app so_sanh_phong_cho [host] [cổng] [--n 1000] [--k 200] [--che-do khoa]: hai
// lượt mô phỏng ở cùng một chế độ (mặc định khoa, đúng): phòng chờ TẮT (mọi người dồn vào "mua", máy chủ
// giới hạn số yêu cầu đang xử lý -> từ chối "qua_tai", người dùng bấm lại) rồi BẬT (xếp hàng, cấp lượt
// theo thùng token). Bảng: số đơn mua được, số lần bị từ chối vì quá tải, số người hết vé, thời gian mua
// p50/p99 (từ lần bấm "mua" đầu tiên tới kết quả cuối, gồm cả các lần bấm lại) và thời gian cả lượt.
func chaySoSanhPhongCho(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("so_sanh_phong_cho")
	n := fs.Int("n", 1000, "số người dùng ảo")
	k := fs.Int("k", 0, "số kết nối (mặc định min(n, 200))")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	kk := *k
	if kk == 0 {
		kk = min(*n, 200)
	}
	bao := func(text string) { inDong("mo_phong", text) }
	hang, err := tc.SoSanhPhongCho(host, cong, *n, kk, bao, *cheDo)
	if err != nil {
		bao("lỗi: " + err.Error())
	}
	bao(fmt.Sprintf("Bật/tắt phòng chờ (%d người, %s, %d kết nối):", *n, *cheDo, min(kk, *n)))
	for _, d := range tc.BangPhongCho(hang) {
		khoaIn.Lock()
		fmt.Println(d)
		khoaIn.Unlock()
	}
}
