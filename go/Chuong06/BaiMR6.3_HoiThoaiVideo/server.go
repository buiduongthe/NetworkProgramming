package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"math"
	"net"
	"os"
	"os/signal"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Server ghép lưới kiểu MCU (Multipoint Control Unit): giải nén khung của mọi client, ghép thành
// lưới (mỗi ô 160×120), nén lại, gửi cho từng người nhận theo fps/độ phân giải người đó chọn.
//
// Quy tắc quyền điều khiển (giống mic ở MR6.2):
//  1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
//  2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
//  3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
//  4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và fps chung do server đặt.
//  5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn, không vượt fps chung.
//  6. Server tự thực thi: bỏ khung của camera đang tắt, khung đến nhanh hơn fps cho phép,
//     datagram sai địa chỉ nguồn.

type khach struct {
	ma                uint16
	ten, ip           string
	conn              net.Conn
	khoaGui           sync.Mutex
	udp               *net.UDPAddr
	tuTat, serverTat  bool
	fpsToiDa, chia    int // giới hạn gửi do server đặt
	nhanFps, nhanChia int // tham số nhận do client chọn
	ghep              *ghepKhung
	o                 []byte // ô 160×120 đã phóng to, có nhãn – dùng khi ghép lưới
	lucNhanCuoi       time.Time
	boVuotFps, boTat  int
	saiNguon          int
	demKhung, demByte int // cửa sổ 1 giây để đo fps, kbit/s vào
	fpsVao, kbpsVao   float64
	soKhungRa         uint32
	hanGuiRa          time.Time
	demRa, byteRa     int
	fpsRa, kbpsRa     float64
}

var (
	khoa     sync.Mutex
	dsKhach  = map[uint16]*khach{}
	daRoi    []*khach
	maMoi    uint16
	fpsChung = fpsMacDinh // giới hạn fps đầu ra chung cho mọi người nhận
	congUDP  int
)

func slog(format string, a ...any) { fmt.Printf("[server] "+format+"\n", a...) }

func (k *khach) camMo() bool { return !k.tuTat && !k.serverTat } // quy tắc 1

func (k *khach) lyDo() string {
	switch {
	case k.camMo():
		return "mở"
	case k.tuTat && k.serverTat:
		return "tắt (cả hai)"
	case k.serverTat:
		return "tắt (server)"
	}
	return "tắt (tự tắt)"
}

func (k *khach) gui(v any) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	guiJSON(k.conn, v)
}

// guiCamera – báo cho client tham số gửi hiện hành.
func (k *khach) guiCamera() {
	k.gui(map[string]any{"loai": "camera", "mo": k.camMo(), "ly_do": k.lyDo(), "fps": k.fpsToiDa, "chia": k.chia})
	k.veO(nil, 0, 0)
}

func (k *khach) guiNhan() {
	k.gui(map[string]any{"loai": "nhan", "fps": min(k.nhanFps, fpsChung), "chia": k.nhanChia})
}

// veO – vẽ ô của client trong lưới: hình đã phóng về 160×120 + dải nhãn trạng thái.
func (k *khach) veO(anh []byte, w, h int) {
	var o []byte
	switch {
	case anh != nil:
		o = phongTo(anh, w, h, rong/w)
	case k.camMo() && k.o != nil:
		o = k.o
	default:
		o = make([]byte, rong*cao*3)
		toMau(o, rong, 0, 0, rong, cao, mau{40, 40, 40})
		if !k.camMo() {
			veChu(o, rong, cao, 20, 44, "TAT CAMERA", 2, mau{255, 255, 255})
			lydo := "(TU TAT)"
			if k.serverTat {
				lydo = "(SERVER)"
			}
			veChu(o, rong, cao, 44, 66, lydo, 2, mau{255, 255, 255})
		}
	}
	toMau(o, rong, 0, cao-11, rong, 11, mau{0, 0, 0})
	nhan := fmt.Sprintf("%s %dFPS", k.ten, k.fpsToiDa)
	if k.chia > 1 {
		nhan += fmt.Sprintf(" 1/%d", k.chia)
	}
	veChu(o, rong, cao, 2, cao-9, nhan, 1, mau{255, 255, 0})
	k.o = o
}

func phatSuKien(text string) {
	slog("  => %s", text)
	for _, k := range dsKhach {
		k.gui(map[string]any{"loai": "su_kien", "noi_dung": text})
	}
}

// ---------------------------------------------------------------- Kênh điều khiển TCP

func phucVu(c net.Conn) {
	defer c.Close()
	r := bufio.NewScanner(c)
	if !r.Scan() {
		return
	}
	var tin map[string]any
	if json.Unmarshal(r.Bytes(), &tin) != nil || tin["loai"] != "vao" {
		return
	}
	ten, _ := tin["ten"].(string)
	if ten == "" {
		return
	}
	if len(ten) > 8 {
		ten = ten[:8]
	}
	addr := c.RemoteAddr().(*net.TCPAddr)
	khoa.Lock()
	maMoi++
	k := &khach{ma: maMoi, ten: ten, ip: addr.IP.String(), conn: c, fpsToiDa: fpsMacDinh, chia: 1,
		nhanFps: fpsMacDinh, nhanChia: 1, ghep: moiGhep()}
	dsKhach[k.ma] = k
	khoa.Unlock()
	k.gui(map[string]any{"loai": "chao", "ma": k.ma, "cong_udp": congUDP, "rong": rong, "cao": cao})
	khoa.Lock()
	slog("#%d %s vào phòng từ %s", k.ma, k.ten, addr)
	k.guiCamera()
	k.guiNhan()
	phatSuKien(k.ten + " vào phòng")
	khoa.Unlock()
	for r.Scan() {
		var tin map[string]any
		if json.Unmarshal(r.Bytes(), &tin) != nil {
			break
		}
		khoa.Lock()
		xuLyTinClient(k, tin)
		khoa.Unlock()
	}
	khoa.Lock()
	delete(dsKhach, k.ma)
	daRoi = append(daRoi, k)
	slog("#%d %s rời phòng", k.ma, k.ten)
	phatSuKien(k.ten + " rời phòng")
	khoa.Unlock()
}

func xuLyTinClient(k *khach, tin map[string]any) {
	switch tin["loai"] {
	case "tu_tat_camera":
		k.tuTat = true
		k.guiCamera()
		phatSuKien(k.ten + " tự tắt camera")
	case "tu_bat_camera":
		k.tuTat = false
		if k.serverTat { // quy tắc 2
			slog("%s xin bật camera -> từ chối (server đang tắt)", k.ten)
			k.gui(map[string]any{"loai": "tu_choi", "ly_do": "server đang tắt camera của bạn"})
		} else {
			k.guiCamera()
			phatSuKien(k.ten + " tự bật camera")
		}
	case "nhan": // quy tắc 5
		k.nhanFps = max(1, min(30, soNguyen(tin["fps"], k.nhanFps)))
		if soNguyen(tin["chia"], k.nhanChia) == 2 {
			k.nhanChia = 2
		} else {
			k.nhanChia = 1
		}
		slog("%s chọn nhận %d fps, độ phân giải 1/%d", k.ten, k.nhanFps, k.nhanChia)
		k.guiNhan()
	}
}

// ---------------------------------------------------------------- Kênh dữ liệu UDP

func nhanUDP(sock *net.UDPConn) {
	buf := make([]byte, 2048)
	for {
		n, addr, err := sock.ReadFromUDP(buf)
		if err != nil {
			return
		}
		h, payload, ok := tachHeader(buf[:n])
		if !ok {
			continue
		}
		bayGio := time.Now()
		khoa.Lock()
		xuLyManh(h, payload, addr, bayGio)
		khoa.Unlock()
	}
}

func xuLyManh(h header, payload []byte, addr *net.UDPAddr, bayGio time.Time) {
	k := dsKhach[h.Ma]
	if k == nil {
		return
	}
	if k.udp == nil && addr.IP.String() == k.ip {
		k.udp = addr
		slog("#%d %s đăng ký UDP %s", k.ma, k.ten, addr)
	}
	if k.udp == nil || addr.String() != k.udp.String() { // quy tắc 6
		k.saiNguon++
		return
	}
	if h.Loai != loaiManh {
		return
	}
	nen := k.ghep.Them(h, payload, bayGio)
	if nen == nil {
		return
	}
	if !k.camMo() { // quy tắc 6: camera tắt
		k.boTat++
		return
	}
	if bayGio.Sub(k.lucNhanCuoi).Seconds() < 0.8/float64(k.fpsToiDa) { // vượt fps (dung sai 20%)
		k.boVuotFps++
		return
	}
	anh, err := giaiNen(nen)
	w, c := int(h.W), int(h.H)
	if err != nil || w == 0 || w*c*3 != len(anh) || rong%w != 0 || w*cao != c*rong {
		return // kích thước không hợp lệ
	}
	k.lucNhanCuoi = bayGio
	k.demKhung++
	k.demByte += len(nen)
	k.veO(anh, w, c)
}

// ---------------------------------------------------------------- Ghép lưới và gửi

// ghepLuoi – ghép ô của mọi client thành lưới cot × hang (cot = ⌈√n⌉).
func ghepLuoi(ds []*khach) ([]byte, int, int) {
	n := max(1, len(ds))
	cot := int(math.Ceil(math.Sqrt(float64(n))))
	hang := (n + cot - 1) / cot
	W, H := cot*rong, hang*cao
	ra := make([]byte, W*H*3)
	toMau(ra, W, 0, 0, W, H, mau{15, 15, 15})
	for i, k := range ds {
		x0, y0 := (i%cot)*rong, (i/cot)*cao
		for y := 0; y < cao; y++ {
			copy(ra[((y0+y)*W+x0)*3:], k.o[y*rong*3:(y+1)*rong*3])
		}
	}
	return ra, W, H
}

// vongGui – mỗi 5 ms kiểm tra người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ theo
// độ phân giải nhận, nén, chia mảnh và gửi. Mỗi giây tính lại fps, kbit/s.
func vongGui(sock *net.UDPConn) {
	mocDo := time.Now()
	for {
		bayGio := time.Now()
		khoa.Lock()
		var ds []*khach
		for _, k := range dsKhach {
			if k.o != nil {
				ds = append(ds, k)
			}
		}
		sort.Slice(ds, func(i, j int) bool { return ds[i].ma < ds[j].ma })
		for _, k := range dsKhach {
			fps := min(k.nhanFps, fpsChung)
			if k.udp == nil || len(ds) == 0 || bayGio.Before(k.hanGuiRa) {
				continue
			}
			k.hanGuiRa = k.hanGuiRa.Add(time.Second / time.Duration(fps))
			if k.hanGuiRa.Before(bayGio) {
				k.hanGuiRa = bayGio
			}
			anh, w, h := ghepLuoi(ds)
			anh, w, h = thuNho(anh, w, h, k.nhanChia)
			nen := nenZlib(anh)
			for _, g := range chiaManh(maServer, k.soKhungRa, w, h, nen) {
				sock.WriteToUDP(g, k.udp)
			}
			k.soKhungRa++
			k.demRa++
			k.byteRa += len(nen)
		}
		if bayGio.Sub(mocDo) >= time.Second {
			for _, k := range dsKhach {
				k.fpsVao, k.kbpsVao = float64(k.demKhung), float64(k.demByte)*8/1000
				k.fpsRa, k.kbpsRa = float64(k.demRa), float64(k.byteRa)*8/1000
				k.demKhung, k.demByte, k.demRa, k.byteRa = 0, 0, 0, 0
			}
			mocDo = mocDo.Add(time.Second)
		}
		khoa.Unlock()
		time.Sleep(5 * time.Millisecond)
	}
}

// ---------------------------------------------------------------- Bảng điều khiển

func danhSach(them []*khach) []*khach {
	var kq []*khach
	for _, k := range dsKhach {
		kq = append(kq, k)
	}
	kq = append(kq, them...)
	sort.Slice(kq, func(i, j int) bool { return kq[i].ma < kq[j].ma })
	return kq
}

func chon(ten string) []*khach {
	var kq []*khach
	for _, k := range danhSach(nil) {
		if ten == "tat_ca" || k.ten == ten {
			kq = append(kq, k)
		}
	}
	return kq
}

func lenhServer(dong string) bool {
	p := strings.Fields(dong)
	if len(p) == 0 {
		return true
	}
	slog("t=%.1fs > %s", time.Since(t0).Seconds(), dong)
	khoa.Lock()
	defer khoa.Unlock()
	ai := ""
	if len(p) > 1 {
		ai = p[1]
		if ai == "tat_ca" {
			ai = "tất cả"
		}
	}
	switch {
	case (p[0] == "tat_camera" || p[0] == "bat_camera") && len(p) == 2:
		var vanTat []string
		for _, k := range chon(p[1]) {
			k.serverTat = p[0] == "tat_camera"
			k.guiCamera()
			if k.tuTat {
				vanTat = append(vanTat, k.ten)
			}
		}
		if p[0] == "tat_camera" {
			phatSuKien("server tắt camera " + ai)
		} else {
			them := ""
			if len(vanTat) > 0 { // quy tắc 3
				them = " (" + strings.Join(vanTat, ", ") + " vẫn tự tắt)"
			}
			phatSuKien("server bật camera " + ai + them)
		}
	case p[0] == "fps_toi_da" && len(p) == 3:
		n, _ := strconv.Atoi(p[2])
		for _, k := range chon(p[1]) {
			k.fpsToiDa = max(1, min(30, n))
			k.guiCamera()
		}
		phatSuKien(fmt.Sprintf("server giới hạn %s gửi tối đa %s fps", ai, p[2]))
	case p[0] == "do_phan_giai" && len(p) == 3 && (p[2] == "1" || p[2] == "2" || p[2] == "4"):
		n, _ := strconv.Atoi(p[2])
		for _, k := range chon(p[1]) {
			k.chia = n
			k.guiCamera()
		}
		phatSuKien(fmt.Sprintf("server đặt độ phân giải %s = 1/%s", ai, p[2]))
	case p[0] == "fps_chung" && len(p) == 2:
		n, _ := strconv.Atoi(p[1])
		fpsChung = max(1, min(30, n))
		for _, k := range dsKhach {
			k.guiNhan()
		}
		phatSuKien(fmt.Sprintf("server đặt fps chung tối đa %d", fpsChung))
	case p[0] == "trang_thai":
		inTrangThai()
	case p[0] == "thoat":
		return false
	default:
		slog("lệnh không hợp lệ")
	}
	return true
}

// padPhai – căn trái theo số ký tự (rune); %-12s của Go đếm byte nên tiếng Việt bị lệch cột.
func padPhai(s string, n int) string { return s + strings.Repeat(" ", max(0, n-len([]rune(s)))) }

func inTrangThai() {
	slog("Trạng thái: %d client, fps chung %d", len(dsKhach), fpsChung)
	slog("                     giới hạn vào        ra (lưới)")
	slog(" #  tên camera       fps 1/n  fps  kbit  fps  kbit")
	for _, k := range danhSach(nil) {
		slog("%2d  %-3s %s %3d %3d %4.0f %5.0f %4.0f %5.0f", k.ma, k.ten, padPhai(k.lyDo(), 12),
			k.fpsToiDa, k.chia, k.fpsVao, k.kbpsVao, k.fpsRa, k.kbpsRa)
	}
}

func inTongKet() {
	slog("Tổng kết luồng vào (từ client tới server):")
	slog(" #  tên mảnh khung hỏng mảnh mất  bỏ:tắt vượt fps")
	slog("        nhận    đủ        mất hẳn")
	for _, k := range danhSach(daRoi) {
		g := k.ghep
		slog("%2d  %-3s %4d %5d %4d %4d %4d %6d %6d", k.ma, k.ten, g.ManhNhan, g.KhungDu, g.KhungHong,
			g.ManhMat, g.KhungMatHan, k.boTat, k.boVuotFps)
	}
}

func runServer(host string, congTCP, congU int) {
	congUDP = congU
	ln, err := net.Listen("tcp", net.JoinHostPort(host, strconv.Itoa(congTCP)))
	if err != nil {
		fmt.Println("[server] Lỗi:", err)
		os.Exit(1)
	}
	udp, err := net.ListenUDP("udp", &net.UDPAddr{IP: net.ParseIP(host), Port: congUDP})
	if err != nil {
		fmt.Println("[server] Lỗi:", err)
		os.Exit(1)
	}
	udp.SetReadBuffer(1 << 20) // đệm nhận lớn: mảnh đến dồn dập
	slog("TCP %s:%d điều khiển; UDP :%d video", host, congTCP, congUDP)
	slog("Khung %dx%d RGB %d B, nén zlib, mảnh ≤ 1200 B", rong, cao, rong*cao*3)
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			c.(*net.TCPConn).SetNoDelay(true)
			go phucVu(c)
		}
	}()
	go nhanUDP(udp)
	go vongGui(udp)
	thoat := make(chan struct{})
	go func() {
		if chayKichBan(os.Stdin, lenhServer) {
			close(thoat) // lệnh thoat
		}
	}()
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, os.Interrupt, syscall.SIGTERM)
	select {
	case <-sig:
	case <-thoat:
	}
	slog("Dừng server (Ctrl+C)")
	khoa.Lock()
	inTongKet()
	khoa.Unlock()
}

// chayKichBan – đọc từng dòng lệnh; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy;
// phần sau dấu # là chú thích. Hàm lenh trả về false để dừng; khi đó chayKichBan trả về true.
func chayKichBan(f *os.File, lenh func(string) bool) bool {
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		dong := strings.TrimSpace(strings.SplitN(sc.Text(), "#", 2)[0])
		if strings.HasPrefix(dong, "@") {
			moc, con, _ := strings.Cut(dong, " ")
			t, _ := strconv.ParseFloat(moc[1:], 64)
			time.Sleep(time.Duration(t*float64(time.Second)) - time.Since(t0))
			dong = strings.TrimSpace(con)
		}
		if dong != "" && !lenh(dong) {
			return true
		}
	}
	return false
}
