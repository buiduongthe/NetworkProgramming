package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"math/rand"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"
)

// nguonAm – nơi lấy 320 mẫu mỗi 20 ms. Thư viện chuẩn Go không có API micro/loa, nên bản Go
// chỉ dùng âm tổng hợp (sin) hoặc tệp WAV.
type nguonAm interface{ layKhung() []int16 }

type nguonSin struct {
	f, a float64
	n    int
}

// layKhung – sin tần số f, pha liên tục giữa các khung.
func (s *nguonSin) layKhung() []int16 {
	w := 2 * math.Pi * s.f / tanSoMau
	k := make([]int16, mauMoiKhung)
	for i := range k {
		k[i] = int16(math.Round(s.a * math.Sin(w*float64(s.n+i))))
	}
	s.n += mauMoiKhung
	return k
}

type nguonWAV struct {
	mau []int16
	vt  int
}

func (s *nguonWAV) layKhung() []int16 { // phát lặp tệp WAV
	k := make([]int16, mauMoiKhung)
	for i := range k {
		k[i] = s.mau[(s.vt+i)%len(s.mau)]
	}
	s.vt += mauMoiKhung
	return k
}

type moc struct {
	T      float64 `json:"t"`
	SuKien string  `json:"su_kien"`
}

// runClient – mỗi 20 ms: (1) thu một khung từ nguồn, đóng gói RTP, gửi UDP nếu mic mở;
// (2) lấy một khung từ bộ đệm jitter, nhân âm lượng nghe, ghi vào bản ghi, đo mức.
func runClient(ten, host string, cong int, args []string) {
	fs := flag.NewFlagSet("khach", flag.ExitOnError)
	tanSo := fs.Float64("tan-so", 440, "tần số âm tổng hợp (Hz)")
	bienDo := fs.Float64("bien-do", 0.25, "biên độ tương đối 0..1")
	wav := fs.String("wav", "", "dùng tệp WAV làm nguồn âm")
	ghi := fs.String("ghi", "", "ghi những gì nghe được vào tệp WAV")
	thoiGian := fs.Float64("thoi-gian", 10, "số giây chạy")
	matGoi := fs.Float64("mat-goi", 0, "giả lập tỉ lệ mất gói khi gửi")
	dem := fs.Int("dem", 3, "độ sâu bộ đệm jitter (khung)")
	fs.Parse(args)
	tag := "[khach " + ten + "]"
	log := func(format string, a ...any) { fmt.Printf(tag+" "+format+"\n", a...) }

	var nguon nguonAm = &nguonSin{f: *tanSo, a: *bienDo * 32767}
	moTa := fmt.Sprintf("sin %g Hz, biên độ %g", *tanSo, *bienDo)
	if *wav != "" {
		m, err := docWAV(*wav)
		if err != nil {
			log("Lỗi đọc %s: %v", *wav, err)
			os.Exit(1)
		}
		nguon, moTa = &nguonWAV{mau: m}, "tệp "+*wav
	}

	// ---- Kênh điều khiển TCP: vào phòng
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
		Ma      uint32 `json:"ma"`
		CongUDP int    `json:"cong_udp"`
	}
	if !sc.Scan() || json.Unmarshal(sc.Bytes(), &chao) != nil {
		log("Server không chào")
		os.Exit(1)
	}
	log("Vào phòng, mã %d; nguồn: %s", chao.Ma, moTa)
	log("Gửi %d mẫu/%d ms -> 50 gói/s, 256 kbit/s PCM", mauMoiKhung, khungMs)

	// ---- Kênh dữ liệu UDP: DialUDP ("connect") để chỉ nhận gói từ đúng địa chỉ server
	udp, err := net.DialUDP("udp", nil, &net.UDPAddr{IP: net.ParseIP(host), Port: chao.CongUDP})
	if err != nil {
		log("Lỗi UDP: %v", err)
		os.Exit(1)
	}

	var khoa sync.Mutex
	micMo, dung, amLuong := false, false, 1.0
	boDem := moiBoDem(*dem)
	var banGhi []int16
	var dsMoc []moc
	danhMoc := func(text string) {
		khoa.Lock()
		dsMoc = append(dsMoc, moc{math.Round(float64(len(banGhi))/tanSoMau*1000) / 1000, text})
		khoa.Unlock()
	}

	go func() { // đọc kênh điều khiển
		for sc.Scan() {
			var tin map[string]any
			if json.Unmarshal(sc.Bytes(), &tin) != nil {
				break
			}
			switch tin["loai"] {
			case "mic":
				khoa.Lock()
				micMo, _ = tin["mo"].(bool)
				khoa.Unlock()
				log("Mic: %v", tin["ly_do"])
			case "su_kien":
				log("Sự kiện: %v", tin["noi_dung"])
				danhMoc(fmt.Sprint(tin["noi_dung"]))
			case "tu_choi":
				log("Bị từ chối: %v", tin["ly_do"])
			}
		}
		khoa.Lock()
		dung = true // server đóng kết nối -> client dừng
		khoa.Unlock()
	}()

	go func() { // nhận luồng trộn từ server
		buf := make([]byte, 2048)
		for {
			n, err := udp.Read(buf)
			if err != nil {
				return
			}
			if g, ok := tachRTP(buf[:n]); ok && g.pt == ptAmThanh {
				khoa.Lock()
				boDem.nhanGoi(g.seq, g.ts, g.payload, time.Since(t0).Seconds())
				khoa.Unlock()
			}
		}
	}()

	lenh := func(dong string) bool {
		p := strings.Fields(dong)
		switch {
		case p[0] == "tat_mic": // tự tắt: có hiệu lực ngay, không cần server đồng ý
			khoa.Lock()
			micMo = false
			khoa.Unlock()
			log("> tat_mic")
			guiTCP(map[string]any{"loai": "tu_tat_mic"})
		case p[0] == "bat_mic": // tự bật: phải chờ server cho phép
			log("> bat_mic (xin server)")
			guiTCP(map[string]any{"loai": "tu_bat_mic"})
		case p[0] == "am_luong" && len(p) == 2:
			v, _ := strconv.ParseFloat(p[1], 64)
			khoa.Lock()
			amLuong = v
			khoa.Unlock()
			log("> am_luong %.2f (chỉ áp dụng tại client)", v)
			danhMoc(fmt.Sprintf("%s tự đặt âm lượng nghe %.2f", ten, v))
			guiTCP(map[string]any{"loai": "am_luong_nghe", "gia_tri": v})
		case p[0] == "thoat":
			khoa.Lock()
			dung = true
			khoa.Unlock()
		default:
			log("lệnh không hợp lệ: %s", dong)
		}
		return true
	}
	go chayKichBan(os.Stdin, lenh) // thời điểm "@t" tính từ lúc chương trình chạy (t0)

	// ---- Vòng lặp 20 ms
	rng := rand.New(rand.NewSource(int64(chao.Ma))) // giả lập mất gói tất định
	var seq uint16
	var ts uint32
	daGui, boGiaLap, tongByte := 0, 0, 0
	soKhung := int(*thoiGian * 1000 / khungMs)
	giu := dongGoiRTP(ptGiuKetNoi, 0, 0, chao.Ma, nil)
	udp.Write(giu) // đăng ký địa chỉ UDP với server ngay từ đầu
	var cuaSo []int16
	tick := time.NewTicker(khungMs * time.Millisecond)
	for n := 1; n <= soKhung; n++ {
		khoa.Lock()
		if dung {
			khoa.Unlock()
			break
		}
		mo, al := micMo, amLuong
		khoa.Unlock()
		// 1. Thu và gửi
		khung := nguon.layKhung()
		if mo {
			if rng.Float64() < *matGoi {
				boGiaLap++ // "mạng" làm rơi gói: số thứ tự vẫn tăng
			} else {
				goi := dongGoiRTP(ptAmThanh, seq, ts, chao.Ma, pcmSangByte(khung))
				udp.Write(goi)
				daGui++
				tongByte += len(goi) + 28 // + header UDP (8) và IPv4 (20)
			}
			seq++
		} else if n%50 == 0 {
			udp.Write(giu) // mic tắt: mỗi giây một gói giữ kết nối
		}
		ts += mauMoiKhung // nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
		// 2. Nhận và phát
		khoa.Lock()
		p := boDem.lay()
		khoa.Unlock()
		mau := make([]int16, mauMoiKhung)
		if p != nil {
			mau = byteSangPCM(p)
		}
		if al != 1 {
			for i, x := range mau {
				mau[i], _ = catNgon(float64(x) * al)
			}
		}
		khoa.Lock()
		banGhi = append(banGhi, mau...)
		khoa.Unlock()
		cuaSo = append(cuaSo, mau...)
		if n%50 == 0 {
			db := mucDBFS(cuaSo)
			trang := "tắt"
			if mo {
				trang = "mở"
			}
			log("%4.1fs nghe %s %s dBFS mic %s", float64(n*khungMs)/1000, thanhMuc(db, 16), chuDB(db, 5), trang)
			cuaSo = cuaSo[:0]
		}
		<-tick.C
	}

	// ---- Kết thúc: thống kê, lưu bản ghi
	giay := time.Since(t0).Seconds()
	tcp.Close()
	udp.Close()
	log("Kết thúc sau %.1f s", giay)
	log("Gửi %d gói (%.0f kbit/s gồm header), giả lập mất %d", daGui, float64(tongByte)*8/giay/1000, boGiaLap)
	khoa.Lock()
	defer khoa.Unlock()
	b := boDem
	log("Nhận %d gói, mất %d, trễ %d, jitter %.2f ms", b.nhan, b.mat(), b.tre, b.jitterMs())
	log("Phát: thiếu %d khung, cạn %d lần, đệm %d khung = %d ms", b.thieu, b.can, *dem, *dem*khungMs)
	if *ghi != "" {
		os.MkdirAll(filepath.Dir(*ghi), 0o755)
		if err := ghiWAV(*ghi, banGhi); err != nil {
			log("Lỗi ghi: %v", err)
			return
		}
		duongMoc := strings.TrimSuffix(*ghi, filepath.Ext(*ghi)) + "_moc.json"
		if dsMoc == nil {
			dsMoc = []moc{}
		}
		j, _ := json.MarshalIndent(map[string]any{"ten": ten, "moc": dsMoc}, "", " ")
		os.WriteFile(duongMoc, j, 0o644)
		log("Đã ghi %s (%.2f s), %d mốc sự kiện", *ghi, float64(len(banGhi))/tanSoMau, len(dsMoc))
	}
}
