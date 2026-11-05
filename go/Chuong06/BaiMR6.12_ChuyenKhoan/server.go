package main

// Máy chủ ngân hàng (bản dòng lệnh). Lõi ở tranhchap (MayChuNganHang, khung MayChuCoSo – dùng
// chung với ứng dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--so-tk 10] [--so-du 5000000]
//	               [--khoa-ngay-tho]
//
// Lệnh stdin: che_do x, tre <ms>, dat_lai [k=v …] (so_tk (5–20), so_du_dau, khoa_ngay_tho (0/1)),
// thong_ke, thoat. --khoa-ngay-tho: chế độ khoa khóa nguồn rồi đích (có thể khóa chết). Chế độ
// khong_khoa chỉ dùng để minh họa lỗi trong phòng lab (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.12_ChuyenKhoan/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	soTK := fs.Int64("so-tk", 10, "số tài khoản (5–20)")
	soDu := fs.Int64("so-du", 5_000_000, "số dư ban đầu mỗi tài khoản")
	ngayTho := fs.Bool("khoa-ngay-tho", false, "chế độ khoa: khóa nguồn rồi đích (có thể khóa chết)")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("so_tk", *soTK, "so_du_dau", *soDu, "khoa_ngay_tho", map[bool]int64{true: 1}[*ngayTho]))
	if err != nil {
		log("lỗi: " + err.Error())
		os.Exit(2)
	}
	if err := sv.BatDau(host, cong); err != nil {
		log("Không khởi động được: " + err.Error())
		os.Exit(1)
	}
	if *ngayTho {
		log("Khóa ngây thơ: nguồn rồi đích (có thể khóa chết)")
	}
	vongServer(sv.MayChuCoSo, log)
}
