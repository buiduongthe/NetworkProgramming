package main

// Máy chủ đăng ký học phần (bản dòng lệnh). Lõi ở tranhchap (MayChuDangKy, khung MayChuCoSo – dùng
// chung với ứng dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-lop 3] [--si-so 50]
//
// Lệnh stdin: che_do x, tre <ms>, dat_lai [k=v …] (so_lop (1–6), si_so (1–500)), thong_ke, thoat. Chế
// độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.10_DangKyHocPhan/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	soLop := fs.Int64("so-lop", 3, "số lớp học phần (1–6)")
	siSo := fs.Int64("si-so", 50, "sĩ số mỗi lớp")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("so_lop", *soLop, "si_so", *siSo))
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
