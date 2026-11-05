package main

// Máy chủ đấu giá (bản dòng lệnh). Lõi ở tranhchap (MayChuDauGia, khung MayChuCoSo – dùng chung với
// ứng dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--thoi-gian 0] [--gia 5000000]
//	               [--buoc 100000] [--gia-han 10]
//
// Lệnh stdin: che_do x, tre <ms>, dat_lai [khoa=giá_trị …] (ten_mon, gia_khoi_diem, buoc_gia,
// thoi_gian_s (0 = không giới hạn), gia_han_s), thong_ke, thoat. Chế độ khong_khoa chỉ dùng để minh
// họa lỗi trong phòng lab (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.6_DauGia/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	thoiGian := fs.Float64("thoi-gian", 0.0, "thời gian phiên (s), 0 = không giới hạn")
	gia := fs.Int64("gia", 5_000_000, "giá khởi điểm")
	buoc := fs.Int64("buoc", 100_000, "bước giá")
	giaHan := fs.Float64("gia-han", 10.0, "gia hạn (s) khi có giá trong những giây cuối")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("thoi_gian_s", *thoiGian, "gia_khoi_diem", *gia,
		"buoc_gia", *buoc, "gia_han_s", *giaHan))
	if err != nil {
		log("lỗi: " + err.Error())
		os.Exit(2)
	}
	if err := sv.BatDau(host, cong); err != nil {
		log("Không khởi động được: " + err.Error())
		os.Exit(1)
	}
	vongServer(sv.MayChuCoSo, log)
}
