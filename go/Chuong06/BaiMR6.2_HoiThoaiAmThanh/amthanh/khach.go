package amthanh

// Khach – lõi client hội thoại âm thanh dạng đối tượng, cho ứng dụng giao diện. Giao thức giống
// bản dòng lệnh (client.go ở thư mục cha), thêm thông điệp "do_loi" để hiện "Server Vol".
//
// Ba goroutine nền: đọc kênh điều khiển TCP, nhận RTP qua UDP vào bộ đệm jitter, và vòng 20 ms
// (thu một khung từ nguồn -> gửi nếu mic mở; lấy một khung từ bộ đệm -> nhân âm lượng nghe ->
// đo mức -> phát ra loa nếu bật). Giao diện không chạm vào socket; nó gọi các phương thức công khai
// và đọc TrangThai() theo nhịp hẹn giờ của chính nó.

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net"
	"strconv"
	"sync"
	"time"
)

type Khach struct {
	mu          sync.Mutex
	Ten         string
	Ma          uint32
	tcp         net.Conn
	khoaTCP     sync.Mutex
	udp         *net.UDPConn
	micMo       bool   // server cho phép và client không tự tắt
	lyDoMic     string // "mở", "tắt (server)", …
	tuChoi      string // lý do bị từ chối gần nhất
	nguon       NguonAm
	heSoMic     float64
	amLuongNghe float64
	loa         *Loa
	doLoiServer float64
	boDem       *BoDemJitter
	mucNhan     float64 // đỉnh dBFS kể từ lần đọc trước
	mucMic      float64
	ganDay      []int16 // 2 giây tiếng nghe gần nhất (vòng tròn) – dùng để phân tích trong demo
	viTri       int
	daGui       int
	byteGui     int
	kbpsGui     float64
	ngat        bool
	lyDoNgat    string
	t0          time.Time
	nhatKy      func(string)
}

// TrangThaiKhach – ảnh chụp để giao diện hiển thị.
type TrangThaiKhach struct {
	Ma                  uint32
	MicMo               bool
	LyDoMic, TuChoi     string
	CoNguon             bool
	DoLoiServer         float64
	MucNhan, MucMic     float64
	GoiNhan, Mat        int
	JitterMs, KbpsGui   float64
	DaNgat              bool
	LyDoNgat            string
	DangPhatLoa, LoaLoi bool
}

// KetNoi – vào phòng: gửi "vao", chờ "chao", mở UDP, rồi báo ngay "tu_tat_mic" vì micro chưa
// được bật (giao diện KHÔNG tự bật micro; chỉ khi người dùng bấm Start Mic).
func KetNoi(ten, host string, cong int, nhatKy func(string)) (*Khach, error) {
	tcp, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), 5*time.Second)
	if err != nil {
		return nil, err
	}
	tcp.(*net.TCPConn).SetNoDelay(true)
	k := &Khach{Ten: ten, tcp: tcp, heSoMic: 1, amLuongNghe: 1, doLoiServer: 1, boDem: MoiBoDem(3),
		mucNhan: math.Inf(-1), mucMic: math.Inf(-1), ganDay: make([]int16, 2*TanSoMau), t0: time.Now(),
		nhatKy: nhatKy, lyDoMic: "chưa rõ"}
	sc := bufio.NewScanner(tcp)
	k.guiTCP(map[string]any{"loai": "vao", "ten": ten})
	tcp.SetReadDeadline(time.Now().Add(5 * time.Second))
	var chao struct {
		Loai    string `json:"loai"`
		Ma      uint32 `json:"ma"`
		CongUDP int    `json:"cong_udp"`
	}
	if !sc.Scan() || json.Unmarshal(sc.Bytes(), &chao) != nil || chao.Loai != "chao" {
		tcp.Close()
		return nil, errors.New("server không chào (không phải server hội thoại âm thanh?)")
	}
	tcp.SetReadDeadline(time.Time{})
	k.Ma = chao.Ma
	k.udp, err = net.DialUDP("udp", nil, &net.UDPAddr{IP: net.ParseIP(host), Port: chao.CongUDP})
	if err != nil {
		if ips, e := net.LookupIP(host); e == nil && len(ips) > 0 { // host là tên miền
			k.udp, err = net.DialUDP("udp", nil, &net.UDPAddr{IP: ips[0], Port: chao.CongUDP})
		}
		if err != nil {
			tcp.Close()
			return nil, err
		}
	}
	k.log("Vào phòng, mã %d; RTP UDP %d", k.Ma, chao.CongUDP)
	k.guiTCP(map[string]any{"loai": "tu_tat_mic"})
	go k.docDieuKhien(sc)
	go k.nhanUDP()
	go k.vong()
	return k, nil
}

func (k *Khach) log(format string, a ...any) {
	if k.nhatKy != nil {
		k.nhatKy(fmt.Sprintf(format, a...))
	}
}

func (k *Khach) guiTCP(v any) {
	k.khoaTCP.Lock()
	defer k.khoaTCP.Unlock()
	GuiJSON(k.tcp, v)
}

func (k *Khach) docDieuKhien(sc *bufio.Scanner) {
	for sc.Scan() {
		var tin map[string]any
		if json.Unmarshal(sc.Bytes(), &tin) != nil {
			break
		}
		k.mu.Lock()
		switch tin["loai"] {
		case "mic":
			k.micMo, _ = tin["mo"].(bool)
			k.lyDoMic = fmt.Sprint(tin["ly_do"])
			if k.micMo {
				k.tuChoi = ""
			}
			k.log("Mic: %v", tin["ly_do"])
		case "su_kien":
			k.log("Sự kiện: %v", tin["noi_dung"])
		case "tu_choi":
			k.tuChoi = fmt.Sprint(tin["ly_do"])
			k.log("Bị từ chối: %v", tin["ly_do"])
		case "do_loi":
			if v, ok := tin["gia_tri"].(float64); ok {
				k.doLoiServer = v
			}
		}
		k.mu.Unlock()
	}
	k.mu.Lock()
	if !k.ngat {
		k.ngat, k.lyDoNgat = true, "server đóng kết nối"
		k.log("Server đóng kết nối")
	}
	k.mu.Unlock()
	k.dong()
}

func (k *Khach) nhanUDP() {
	buf := make([]byte, 2048)
	for {
		n, err := k.udp.Read(buf)
		if err != nil {
			k.mu.Lock()
			ngat := k.ngat
			k.mu.Unlock()
			if ngat {
				return
			}
			continue // Windows: ICMP "cổng không tới được" làm Read lỗi tạm thời
		}
		if g, ok := TachRTP(buf[:n]); ok && g.Pt == PtAmThanh {
			k.mu.Lock()
			k.boDem.NhanGoi(g.Seq, g.Ts, g.Payload, time.Since(k.t0).Seconds())
			k.mu.Unlock()
		}
	}
}

// vong – nhịp 20 ms (time.Ticker không trôi theo thời gian xử lý).
func (k *Khach) vong() {
	var seq uint16
	var ts uint32
	giu := DongGoiRTP(PtGiuKetNoi, 0, 0, k.Ma, nil)
	k.udp.Write(giu) // đăng ký địa chỉ UDP với server
	tick := time.NewTicker(KhungMs * time.Millisecond)
	defer tick.Stop()
	mocDo, byteDo := time.Now(), 0
	for n := 1; ; n++ {
		<-tick.C
		k.mu.Lock()
		if k.ngat {
			k.mu.Unlock()
			return
		}
		nguon, mo, heSo := k.nguon, k.micMo, k.heSoMic
		k.mu.Unlock()
		// 1. Thu và gửi: nguồn trả nil (micro thật chưa có khung) thì lượt này không gửi
		var khung []int16
		if nguon != nil {
			khung = nguon.LayKhung()
		}
		if khung != nil && heSo != 1 {
			for i := range khung {
				khung[i], _ = CatNgon(float64(khung[i]) * heSo)
			}
		}
		if mo && khung != nil {
			goi := DongGoiRTP(PtAmThanh, seq, ts, k.Ma, PcmSangByte(khung))
			k.udp.Write(goi)
			seq++
			byteDo += len(goi) + 28
		} else if n%50 == 0 {
			k.udp.Write(giu) // mic tắt: mỗi giây một gói giữ kết nối
		}
		ts += MauMoiKhung
		// 2. Nhận, nhân âm lượng nghe, đo mức, phát
		k.mu.Lock()
		if khung != nil {
			k.mucMic = math.Max(k.mucMic, MucDBFS(khung))
		}
		p := k.boDem.Lay()
		mau := make([]int16, MauMoiKhung)
		if p != nil {
			mau = ByteSangPCM(p)
		}
		if k.amLuongNghe != 1 {
			for i, x := range mau {
				mau[i], _ = CatNgon(float64(x) * k.amLuongNghe)
			}
		}
		if p != nil {
			k.mucNhan = math.Max(k.mucNhan, MucDBFS(mau))
		}
		for _, x := range mau {
			k.ganDay[k.viTri] = x
			k.viTri = (k.viTri + 1) % len(k.ganDay)
		}
		if k.loa != nil {
			k.loa.Ghi(mau)
		}
		if t := time.Since(mocDo); t >= time.Second {
			k.kbpsGui = float64(byteDo) * 8 / t.Seconds() / 1000
			mocDo, byteDo = time.Now(), 0
		}
		k.mu.Unlock()
	}
}

// BatMic – người dùng bấm Start Mic: dùng nguồn này và xin server bật mic (có thể bị từ chối).
func (k *Khach) BatMic(n NguonAm) {
	k.mu.Lock()
	k.nguon, k.tuChoi = n, ""
	k.mu.Unlock()
	k.log("Start Mic -> xin server bật mic")
	k.guiTCP(map[string]any{"loai": "tu_bat_mic"})
}

// TatMic – tự tắt: có hiệu lực ngay, không cần server đồng ý. Nguồn cũ do bên gọi tự đóng.
func (k *Khach) TatMic() {
	k.mu.Lock()
	k.nguon, k.micMo = nil, false
	k.mu.Unlock()
	k.log("Stop Mic (tự tắt)")
	k.guiTCP(map[string]any{"loai": "tu_tat_mic"})
}

func (k *Khach) DatHeSoMic(v float64) { k.mu.Lock(); k.heSoMic = v; k.mu.Unlock() }

// DatAmLuongNghe – chỉ áp dụng tại client; báo server để hiển thị (quy tắc 5).
func (k *Khach) DatAmLuongNghe(v float64) {
	k.mu.Lock()
	doi := k.amLuongNghe != v
	k.amLuongNghe = v
	k.mu.Unlock()
	if doi {
		k.guiTCP(map[string]any{"loai": "am_luong_nghe", "gia_tri": v})
	}
}

// DatLoa – Start Audio (l != nil) / Stop Audio (nil).
func (k *Khach) DatLoa(l *Loa) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.loa != nil && k.loa != l {
		k.loa.Dong()
	}
	k.loa = l
}

func (k *Khach) TrangThai() TrangThaiKhach {
	k.mu.Lock()
	defer k.mu.Unlock()
	t := TrangThaiKhach{k.Ma, k.micMo, k.lyDoMic, k.tuChoi, k.nguon != nil, k.doLoiServer, k.mucNhan,
		k.mucMic, k.boDem.Nhan, k.boDem.Mat(), k.boDem.JitterMs(), k.kbpsGui, k.ngat, k.lyDoNgat,
		k.loa != nil && k.loa.ConChay(), k.loa != nil && !k.loa.ConChay()}
	k.mucNhan, k.mucMic = math.Inf(-1), math.Inf(-1)
	return t
}

// MauGanDay – bản sao 2 giây tiếng nghe gần nhất theo thứ tự thời gian.
func (k *Khach) MauGanDay() []int16 {
	k.mu.Lock()
	defer k.mu.Unlock()
	return append(append([]int16(nil), k.ganDay[k.viTri:]...), k.ganDay[:k.viTri]...)
}

// Ngat – người dùng bấm Disconnect.
func (k *Khach) Ngat() {
	k.mu.Lock()
	if !k.ngat {
		k.ngat, k.lyDoNgat = true, "đã ngắt kết nối"
	}
	k.mu.Unlock()
	k.dong()
}

func (k *Khach) dong() {
	k.tcp.Close()
	k.udp.Close()
	k.mu.Lock()
	if k.loa != nil {
		k.loa.Dong()
		k.loa = nil
	}
	k.mu.Unlock()
}
