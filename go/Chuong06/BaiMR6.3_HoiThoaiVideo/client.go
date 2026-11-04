package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"math/rand"
	"net"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

type anhChup struct {
	w, h int
	rgb  []byte
	nhan string
}

// ghepAnh – ghép các ảnh chụp thành một ảnh: 2 cột, nhãn dưới mỗi ảnh (giống bản Python).
func ghepAnh(duongDan string, ds []anhChup) (int, int, error) {
	const cot = 2
	oW := 0
	for _, a := range ds {
		oW = max(oW, a.w)
	}
	oW += 16
	var caoHang []int
	for i := 0; i < len(ds); i += cot {
		h := 0
		for _, a := range ds[i:min(len(ds), i+cot)] {
			h = max(h, a.h)
		}
		caoHang = append(caoHang, h+34)
	}
	W, H := cot*oW+16, 8
	for _, h := range caoHang {
		H += h
	}
	anh := make([]byte, W*H*3)
	for i := range anh {
		anh[i] = 255
	}
	for n, a := range ds {
		x0 := 16 + (n%cot)*oW + (oW-16-a.w)/2
		y0 := 8
		for _, h := range caoHang[:n/cot] {
			y0 += h
		}
		for y := 0; y < a.h; y++ {
			copy(anh[((y0+y)*W+x0)*3:], a.rgb[y*a.w*3:(y+1)*a.w*3])
		}
		nhan := a.nhan
		if len(nhan) > 52 {
			nhan = nhan[:52]
		}
		veChu(anh, W, H, 16+(n%cot)*oW, y0+a.h+6, nhan, 1, mau{0, 0, 0})
	}
	return W, H, ghiPNG(duongDan, anh, W, H)
}

// runClient – vòng gửi theo fps cho phép: lấy khung camera tổng hợp, giảm độ phân giải theo yêu cầu
// server, nén zlib, chia mảnh, gửi UDP. Goroutine nhận ghép mảnh khung lưới của server.
// Thư viện chuẩn Go không có API camera, nên bản Go chỉ dùng camera tổng hợp.
func runClient(ten, host string, cong int, args []string) {
	fs := flag.NewFlagSet("khach", flag.ExitOnError)
	fpsCam := fs.Int("fps", fpsMacDinh, "tốc độ khung của camera")
	thoiGian := fs.Float64("thoi-gian", 10, "số giây chạy")
	matGoi := fs.Float64("mat-goi", 0, "giả lập tỉ lệ mất mảnh khi gửi")
	ghepRa := fs.String("ghep", "", "ghép các ảnh đã chụp thành tệp PNG này khi kết thúc")
	fs.Parse(args)
	tag := "[khach " + ten + "]"
	log := func(format string, a ...any) { fmt.Printf(tag+" "+format+"\n", a...) }

	// ---- Kênh điều khiển TCP
	tcp, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), 5*time.Second)
	if err != nil {
		log("Không kết nối được: %v", err)
		os.Exit(1)
	}
	tcp.(*net.TCPConn).SetNoDelay(true)
	var khoaTCP sync.Mutex
	guiTCP := func(v any) { khoaTCP.Lock(); guiJSON(tcp, v); khoaTCP.Unlock() }
	sc := bufio.NewScanner(tcp)
	guiTCP(map[string]any{"loai": "vao", "ten": ten})
	var chao struct {
		Ma      uint16 `json:"ma"`
		CongUDP int    `json:"cong_udp"`
	}
	if !sc.Scan() || json.Unmarshal(sc.Bytes(), &chao) != nil {
		log("Server không chào")
		os.Exit(1)
	}
	camera := moiCamera(ten, int(chao.Ma))
	log("Vào phòng, mã %d; camera tổng hợp %dx%d, %d fps", chao.Ma, rong, cao, *fpsCam)

	udp, err := net.DialUDP("udp", nil, &net.UDPAddr{IP: net.ParseIP(host), Port: chao.CongUDP})
	if err != nil {
		log("Lỗi UDP: %v", err)
		os.Exit(1)
	}
	udp.SetReadBuffer(1 << 20) // chỉ nhận datagram từ server (DialUDP = connect)

	var khoa sync.Mutex
	mo, fpsServer, chia, dung := false, fpsMacDinh, 1, false
	ghep := moiGhep()
	var xem *anhChup // khung lưới mới nhất
	khungNhan, byteNhan := 0, 0
	var dsChup []anhChup

	go func() { // đọc kênh điều khiển
		for sc.Scan() {
			var tin map[string]any
			if json.Unmarshal(sc.Bytes(), &tin) != nil {
				break
			}
			switch tin["loai"] {
			case "camera":
				khoa.Lock()
				mo, _ = tin["mo"].(bool)
				fpsServer, chia = soNguyen(tin["fps"], fpsServer), soNguyen(tin["chia"], chia)
				khoa.Unlock()
				log("Camera: %v, gửi tối đa %d fps, 1/%d", tin["ly_do"], soNguyen(tin["fps"], 0), soNguyen(tin["chia"], 1))
			case "nhan":
				log("Nhận: %d fps, độ phân giải 1/%d", soNguyen(tin["fps"], 0), soNguyen(tin["chia"], 1))
			case "su_kien":
				log("Sự kiện: %v", tin["noi_dung"])
			case "tu_choi":
				log("Bị từ chối: %v", tin["ly_do"])
			}
		}
		khoa.Lock()
		dung = true
		khoa.Unlock()
	}()

	go func() { // nhận và ghép khung lưới
		buf := make([]byte, 2048)
		for {
			n, err := udp.Read(buf)
			if err != nil {
				return
			}
			h, payload, ok := tachHeader(buf[:n])
			if !ok || h.loai != loaiManh {
				continue
			}
			khoa.Lock()
			nen := ghep.them(h, payload, time.Now())
			khoa.Unlock()
			if nen == nil {
				continue
			}
			anh, err := giaiNen(nen)
			if err != nil || len(anh) != int(h.w)*int(h.h)*3 {
				continue
			}
			khoa.Lock()
			xem = &anhChup{int(h.w), int(h.h), anh, ""}
			khungNhan++
			byteNhan += len(nen)
			khoa.Unlock()
		}
	}()

	lenh := func(dong string) bool {
		p := strings.Fields(dong)
		log("> %s", dong)
		switch {
		case p[0] == "tat_camera": // tự tắt: có hiệu lực ngay
			khoa.Lock()
			mo = false
			khoa.Unlock()
			guiTCP(map[string]any{"loai": "tu_tat_camera"})
		case p[0] == "bat_camera": // tự bật: chờ server cho phép
			guiTCP(map[string]any{"loai": "tu_bat_camera"})
		case p[0] == "nhan_fps" && len(p) == 2:
			n, _ := strconv.Atoi(p[1])
			guiTCP(map[string]any{"loai": "nhan", "fps": n})
		case p[0] == "nhan_do_phan_giai" && len(p) == 2:
			n, _ := strconv.Atoi(p[1])
			guiTCP(map[string]any{"loai": "nhan", "chia": n})
		case p[0] == "chup":
			khoa.Lock()
			a := xem
			khoa.Unlock()
			if a == nil {
				log("chưa có hình để chụp")
				return true
			}
			os.MkdirAll("out", 0o755)
			duong := fmt.Sprintf("out/chup_%d.png", len(dsChup)+1)
			ghiPNG(duong, a.rgb, a.w, a.h)
			a.nhan = strings.Join(p[1:], " ")
			if a.nhan == "" {
				a.nhan = duong
			}
			dsChup = append(dsChup, *a)
			log("Đã chụp %s (%dx%d)", duong, a.w, a.h)
		case p[0] == "thoat":
			khoa.Lock()
			dung = true
			khoa.Unlock()
		default:
			log("lệnh không hợp lệ")
		}
		return true
	}
	go chayKichBan(os.Stdin, lenh)

	// ---- Vòng gửi khung theo fps cho phép; mỗi giây in thống kê
	rng := rand.New(rand.NewSource(int64(chao.Ma)))
	udp.Write(goiGiu(chao.Ma)) // đăng ký địa chỉ UDP với server
	soKhung, guiKhung, guiManh, boGiaLap := 0, 0, 0, 0
	demGui, byteGui := 0, 0
	hanGui, mocDo := time.Now(), time.Now()
	hanGiu := mocDo.Add(time.Second)
	giay := 0
	log("Mỗi giây: fps và kbit/s gửi | nhận, số mảnh mất")
	for time.Since(t0).Seconds() < *thoiGian {
		bayGio := time.Now()
		khoa.Lock()
		if dung {
			khoa.Unlock()
			break
		}
		dangMo, fps, d := mo, min(*fpsCam, fpsServer), chia
		khoa.Unlock()
		if dangMo && !bayGio.Before(hanGui) {
			hanGui = hanGui.Add(time.Second / time.Duration(fps))
			if hanGui.Before(bayGio) {
				hanGui = bayGio
			}
			anh, w, h := thuNho(camera.layKhung(soKhung), rong, cao, d) // giảm độ phân giải rồi nén
			nen := nenZlib(anh)
			for _, g := range chiaManh(chao.Ma, uint32(soKhung), w, h, nen) {
				guiManh++
				if rng.Float64() < *matGoi {
					boGiaLap++ // "mạng" làm rơi mảnh
				} else {
					udp.Write(g)
				}
			}
			soKhung++
			guiKhung++
			demGui++
			byteGui += len(nen)
		} else if !dangMo && !bayGio.Before(hanGiu) {
			udp.Write(goiGiu(chao.Ma)) // giữ kết nối khi tắt camera
			hanGiu = bayGio.Add(time.Second)
		}
		if bayGio.Sub(mocDo) >= time.Second {
			mocDo = mocDo.Add(time.Second)
			giay++
			khoa.Lock()
			kn, bn, mat := khungNhan, byteNhan, ghep.manhMat
			khungNhan, byteNhan = 0, 0
			khoa.Unlock()
			log("%2ds gửi %2d fps %4.0f | nhận %2d fps %5.0f, mất %d", giay, demGui,
				float64(byteGui)*8/1000, kn, float64(bn)*8/1000, mat)
			demGui, byteGui = 0, 0
		}
		time.Sleep(2 * time.Millisecond)
	}

	// ---- Kết thúc
	tcp.Close()
	udp.Close()
	log("Kết thúc sau %.1f s", time.Since(t0).Seconds())
	log("Gửi %d khung = %d mảnh, giả lập mất %d mảnh", guiKhung, guiManh, boGiaLap)
	khoa.Lock()
	g := ghep
	log("Nhận %d khung đủ, hỏng %d, mất hẳn %d, mảnh mất %d", g.khungDu, g.khungHong, g.khungMatHan, g.manhMat)
	khoa.Unlock()
	if *ghepRa != "" && len(dsChup) > 0 {
		W, H, err := ghepAnh(*ghepRa, dsChup)
		if err != nil {
			log("Lỗi ghi %s: %v", *ghepRa, err)
			return
		}
		log("Đã ghép %d ảnh chụp vào %s (%dx%d)", len(dsChup), *ghepRa, W, H)
	}
}
