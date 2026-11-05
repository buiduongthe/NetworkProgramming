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

// Quy tắc quyền điều khiển (giống bản Python):
//  1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
//  2. Server tắt (khóa) thắng client: khi đang bị khóa, client xin bật mic bị từ chối.
//  3. Server bật = gỡ khóa, KHÔNG bật hộ mic mà người dùng đã tự tắt (quyền riêng tư).
//  4. Độ lợi đầu vào từng client và âm lượng chung do server đặt, áp dụng khi trộn.
//  5. Âm lượng nghe do từng client tự đặt và áp dụng tại client; server không đổi được.
//  6. Server tự thực thi, không tin client: bỏ gói từ client đang tắt mic, bỏ gói sai địa chỉ nguồn.

type khach struct {
	ma          uint32
	ten, ip     string
	conn        net.Conn
	khoaGui     sync.Mutex   // nhiều goroutine có thể cùng gửi trên TCP
	udp         *net.UDPAddr // địa chỉ UDP học được từ gói đầu tiên
	tuTat       bool         // client tự tắt mic
	serverKhoa  bool         // server khóa mic
	doLoi       float64      // độ lợi đầu vào (gain)
	amLuongNghe float64      // client báo (chỉ để hiển thị)
	boDem       *boDemJitter
	seqRa       uint16 // số thứ tự, nhãn thời gian của luồng trộn gửi cho client
	tsRa        uint32
	mucVao      float64
	biChan      int // gói bị bỏ vì mic đang tắt
	saiNguon    int // gói bị bỏ vì sai địa chỉ nguồn
	catNgon     int // số mẫu bị cắt ngọn trong luồng gửi cho client này
}

var (
	khoa         sync.Mutex // bảo vệ mọi trạng thái dùng chung giữa các goroutine
	dsKhach      = map[uint32]*khach{}
	daRoi        []*khach
	maMoi        uint32
	amLuongChung = 1.0
	congUDP      int
	t0           = time.Now()
)

func slog(format string, a ...any) { fmt.Printf("[server] "+format+"\n", a...) }

func (k *khach) micMo() bool { return !k.tuTat && !k.serverKhoa } // quy tắc 1

func (k *khach) lyDo() string {
	switch {
	case k.micMo():
		return "mở"
	case k.tuTat && k.serverKhoa:
		return "tắt (cả hai)"
	case k.serverKhoa:
		return "tắt (server)"
	}
	return "tắt (tự tắt)"
}

func (k *khach) gui(v any) {
	k.khoaGui.Lock()
	defer k.khoaGui.Unlock()
	guiJSON(k.conn, v)
}

func (k *khach) guiMic() {
	k.gui(map[string]any{"loai": "mic", "mo": k.micMo(), "ly_do": k.lyDo()})
	if !k.micMo() {
		k.boDem.Xoa() // bỏ phần âm thanh còn trong bộ đệm
	}
}

// phatSuKien – báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi).
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
	k := &khach{ma: maMoi, ten: ten, ip: addr.IP.String(), conn: c, doLoi: 1, amLuongNghe: 1,
		boDem: moiBoDem(3), mucVao: math.Inf(-1)}
	dsKhach[k.ma] = k
	khoa.Unlock()
	k.gui(map[string]any{"loai": "chao", "ma": k.ma, "cong_udp": congUDP, "tan_so_mau": tanSoMau,
		"khung_ms": khungMs})
	k.gui(map[string]any{"loai": "do_loi", "gia_tri": k.doLoi}) // client hiện "Server Vol"
	khoa.Lock()
	slog("#%d %s vào phòng từ %s", k.ma, k.ten, addr)
	k.guiMic()
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
	case "tu_tat_mic":
		k.tuTat = true
		k.guiMic()
		phatSuKien(k.ten + " tự tắt mic")
	case "tu_bat_mic":
		k.tuTat = false
		if k.serverKhoa { // quy tắc 2
			slog("%s xin bật mic -> từ chối (server đang khóa)", k.ten)
			k.gui(map[string]any{"loai": "tu_choi", "ly_do": "server đang khóa mic của bạn"})
		} else {
			k.guiMic()
			phatSuKien(k.ten + " tự bật mic")
		}
	case "am_luong_nghe": // quy tắc 5: chỉ ghi nhận
		if v, ok := tin["gia_tri"].(float64); ok {
			k.amLuongNghe = v
		}
		slog("%s tự đặt âm lượng nghe %.2f", k.ten, k.amLuongNghe)
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
		g, ok := tachRTP(buf[:n])
		if !ok {
			continue
		}
		khoa.Lock()
		k := dsKhach[g.Ssrc]
		switch {
		case k == nil:
		case k.udp == nil && addr.IP.String() != k.ip:
		default:
			if k.udp == nil { // đăng ký địa chỉ UDP lần đầu
				k.udp = addr
				slog("#%d %s đăng ký UDP %s", k.ma, k.ten, addr)
			}
			if addr.String() != k.udp.String() { // quy tắc 6: sai nguồn
				k.saiNguon++
			} else if g.Pt == ptAmThanh && len(g.Payload) == byteMoiKhung {
				if !k.micMo() { // quy tắc 6: mic tắt
					k.biChan++
				} else {
					k.boDem.NhanGoi(g.Seq, g.Ts, g.Payload, time.Since(t0).Seconds())
				}
			}
		}
		khoa.Unlock()
	}
}

// vongTron – mỗi 20 ms: lấy một khung từ mỗi client đang mở mic, nhân độ lợi, cộng vào tổng T;
// người nghe k nhận T - (tiếng của chính k) – "mix-minus" – nhân âm lượng chung, cắt ngọn, gửi đi.
func vongTron(sock *net.UDPConn) {
	tick := time.NewTicker(khungMs * time.Millisecond) // Ticker giữ nhịp đều, không trôi
	for range tick.C {
		khoa.Lock()
		vao := map[uint32][]float64{}
		tong := make([]float64, mauMoiKhung)
		for _, k := range dsKhach {
			var p []byte
			if k.micMo() {
				p = k.boDem.Lay()
			}
			if p == nil {
				k.mucVao = math.Inf(-1)
				continue
			}
			mau := byteSangPCM(p)
			k.mucVao = mucDBFS(mau)
			v := make([]float64, mauMoiKhung)
			for i, x := range mau {
				v[i] = k.doLoi * float64(x)
				tong[i] += v[i]
			}
			vao[k.ma] = v
		}
		for _, k := range dsKhach {
			if k.udp == nil {
				continue
			}
			rieng := vao[k.ma]
			ra := make([]int16, mauMoiKhung)
			for i := range ra {
				v := tong[i]
				if rieng != nil {
					v -= rieng[i]
				}
				var cat bool
				ra[i], cat = catNgon(v * amLuongChung)
				if cat {
					k.catNgon++
				}
			}
			sock.WriteToUDP(dongGoiRTP(ptAmThanh, k.seqRa, k.tsRa, ssrcServer, pcmSangByte(ra)), k.udp)
			k.seqRa++
			k.tsRa += mauMoiKhung
		}
		khoa.Unlock()
	}
}

// ---------------------------------------------------------------- Bảng điều khiển

// danhSach – các client theo thứ tự mã (map của Go không có thứ tự).
func danhSach(ds map[uint32]*khach, them []*khach) []*khach {
	var kq []*khach
	for _, k := range ds {
		kq = append(kq, k)
	}
	kq = append(kq, them...)
	sort.Slice(kq, func(i, j int) bool { return kq[i].ma < kq[j].ma })
	return kq
}

func chon(ten string) []*khach {
	var kq []*khach
	for _, k := range danhSach(dsKhach, nil) {
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
	switch {
	case (p[0] == "tat_mic" || p[0] == "bat_mic") && len(p) == 2:
		ds := chon(p[1])
		if len(ds) == 0 {
			slog("không có client %s", p[1])
			return true
		}
		var vanTat []string
		for _, k := range ds {
			k.serverKhoa = p[0] == "tat_mic"
			k.guiMic()
			if k.tuTat {
				vanTat = append(vanTat, k.ten)
			}
		}
		ai := p[1]
		if ai == "tat_ca" {
			ai = "tất cả"
		}
		if p[0] == "tat_mic" {
			phatSuKien("server tắt mic " + ai)
		} else {
			them := ""
			if len(vanTat) > 0 { // quy tắc 3
				them = " (" + strings.Join(vanTat, ", ") + " vẫn tự tắt)"
			}
			phatSuKien("server bật mic " + ai + them)
		}
	case p[0] == "do_loi" && len(p) == 3:
		g, _ := strconv.ParseFloat(p[2], 64)
		for _, k := range chon(p[1]) {
			k.doLoi = g
			k.gui(map[string]any{"loai": "do_loi", "gia_tri": g}) // báo client (chỉ để hiển thị)
		}
		phatSuKien(fmt.Sprintf("server đặt độ lợi %s = %.2f", p[1], g))
	case p[0] == "am_luong_chung" && len(p) == 2:
		amLuongChung, _ = strconv.ParseFloat(p[1], 64)
		phatSuKien(fmt.Sprintf("server đặt âm lượng chung %.2f", amLuongChung))
	case p[0] == "trang_thai":
		inTrangThai()
	case p[0] == "thoat":
		return false
	default:
		slog("lệnh không hợp lệ")
	}
	return true
}

func inTrangThai() {
	slog("Trạng thái: %d client, âm lượng chung %.2f", len(dsKhach), amLuongChung)
	slog(" #  tên mic          lợi   mức vào            mất  jit.")
	for _, k := range danhSach(dsKhach, nil) {
		slog("%2d  %-3s %s %4.2f %s %s %3d %4.1fms", k.ma, k.ten, padPhai(k.lyDo(), 12), k.doLoi,
			chuDB(k.mucVao, 5), thanhMuc(k.mucVao, 10), k.boDem.Mat(), k.boDem.JitterMs())
	}
}

// padPhai – căn trái theo số ký tự (rune), vì %-12s của Go đếm byte, tiếng Việt bị lệch cột.
func padPhai(s string, n int) string {
	return s + strings.Repeat(" ", max(0, n-len([]rune(s))))
}

func inTongKet() {
	slog("Tổng kết (gói vào: từ client tới server):")
	slog(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn")
	for _, k := range danhSach(dsKhach, daRoi) {
		b := k.boDem
		slog("%2d  %-3s %7d %3d %4d %5d %3d %7d %8d", k.ma, k.ten, b.Nhan, b.Mat(), b.Tre, b.Thieu,
			b.Can, k.biChan, k.catNgon)
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
	slog("Điều khiển: TCP %s:%d; âm thanh RTP: UDP %s:%d", host, congTCP, host, congUDP)
	slog("PCM 16 bit mono 16 kHz; khung %d ms = %d mẫu = %d B", khungMs, mauMoiKhung, byteMoiKhung)
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			c.(*net.TCPConn).SetNoDelay(true) // tin điều khiển đi ngay
			go phucVu(c)
		}
	}()
	go nhanUDP(udp)
	go vongTron(udp)

	thoat := make(chan struct{})
	go func() { // bảng điều khiển: đọc lệnh từ bàn phím/kịch bản
		if chayKichBan(os.Stdin, lenhServer) {
			close(thoat) // lệnh thoat
		}
		// hết stdin: tiếp tục phục vụ tới khi Ctrl+C
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
