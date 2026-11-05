package main

// Máy chủ quay số trúng thưởng (bản dòng lệnh). Lõi ở tranhchap (MayChuQuaySo, khung MayChuCoSo – dùng
// chung với ứng dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-qua 10] [--p 0.0005]
//
// Lệnh stdin: che_do x, tre <ms>, dat_lai [k=v …] (so_qua (1–10), p (xác suất trúng mỗi lượt, ví dụ
// 0.0005 = 1/2000)), thong_ke, thoat. Chế độ khong_khoa chỉ dùng để minh họa lỗi trong phòng lab
// (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.9_QuaySo/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	soQua := fs.Int64("so-qua", 10, "số món quà (1–10)")
	p := fs.Float64("p", 0.0005, "xác suất trúng mỗi lượt (0.0005 = 1/2000)")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("so_qua", *soQua, "p", *p))
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
