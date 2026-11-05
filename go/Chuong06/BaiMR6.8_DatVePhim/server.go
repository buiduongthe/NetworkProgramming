package main

// Máy chủ đặt vé xem phim (bản dòng lệnh). Lõi ở tranhchap (MayChuDatVe, khung MayChuCoSo – dùng chung với ứng
// dụng giao diện); tệp này chỉ đọc tùy chọn rồi chạy vòng lệnh chung (chung_cli.go).
//
//	out/app server [host] [cổng] [--che-do khoa] [--tre-ms 2] [--ttl 30] [--ghe-nong E7]
//
// Lệnh stdin: che_do x, tre <ms>, dat_lai [k=v …] (ttl_s – thời hạn giữ chỗ, ghe_nong – ghế mọi người ảo
// nhắm tới đầu tiên (rỗng = chọn ngẫu nhiên), gia_ve, phim, suat), thong_ke, thoat. Chế độ khong_khoa chỉ
// dùng để minh họa lỗi trong phòng lab (127.0.0.1).

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	tc "github.com/buiduongthe/NetworkProgramming/go/Chuong06/BaiMR6.8_DatVePhim/tranhchap"
)

func chayServer(a []string) {
	pos, con := viTri(a, 2)
	fs := boTuyChon("server")
	cheDo := fs.String("che-do", "khoa", "khong_khoa|khoa|lac_quan|hang_doi")
	treMs := fs.Float64("tre-ms", 2.0, "độ trễ xử lý tối đa (ms)")
	ttl := fs.Float64("ttl", 30.0, "thời hạn giữ chỗ (s)")
	gheNong := fs.String("ghe-nong", "E7", "ghế người ảo nhắm tới đầu tiên ('' = ngẫu nhiên)")
	if err := fs.Parse(con); err != nil || !coTrong(tc.CheDoDS, *cheDo) {
		fmt.Println("tham số sai; --che-do:", strings.Join(tc.CheDoDS, "|"))
		os.Exit(2)
	}
	host, cong := lay(pos, 0, "127.0.0.1"), soCong(lay(pos, 1, strconv.Itoa(congMacDinh)))
	log := func(text string) { inDong("server", text) }
	sv, err := tc.MoiMayChu(*cheDo, *treMs, log, tc.T("ttl_s", *ttl, "ghe_nong", *gheNong))
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
