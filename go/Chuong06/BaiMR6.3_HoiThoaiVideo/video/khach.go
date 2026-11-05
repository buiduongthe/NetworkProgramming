package video

// Khach – lõi client hội thoại video dạng đối tượng, cho ứng dụng giao diện. Giao thức giống bản
// dòng lệnh (client.go ở thư mục cha). Goroutine nền: đọc TCP, nhận/ghép mảnh lưới qua UDP, và vòng
// gửi (lấy khung từ nguồn theo fps cho phép -> thu nhỏ theo yêu cầu server -> nén -> chia mảnh -> gửi).
// Giao diện chỉ gọi phương thức công khai và đọc TrangThai() theo nhịp hẹn giờ.

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"strconv"
	"sync"
	"time"
)

type Khach struct {
	mu                   sync.Mutex
	Ten                  string
	Ma                   uint16
	tcp                  net.Conn
	khoaTCP              sync.Mutex
	udp                  *net.UDPConn
	camMo                bool
	lyDo, tuChoi         string
	fpsServer, chia      int // giới hạn gửi do server đặt
	nhanFps, nhanChia    int // tham số nhận server xác nhận
	nguon                NguonHinh
	fpsCam               int
	ghep                 *GhepKhung
	xemTruoc             []byte // khung vừa gửi (đã thu nhỏ rồi phóng lại 160×120) – để xem trước
	soXemTruoc           int
	luoi                 []byte
	luoiW, luoiH, soLuoi int
	demGui, byteGui      int
	demNhan, byteNhan    int
	fpsGui, kbpsGui      float64
	fpsNhan, kbpsNhan    float64
	ngat                 bool
	lyDoNgat             string
	nhatKy               func(string)
}

// TrangThaiKhach – ảnh chụp cho giao diện; ảnh chỉ được chép khi số thứ tự khác lần trước.
type TrangThaiKhach struct {
	Ma                uint16
	CamMo, CoNguon    bool
	LyDo, TuChoi      string
	FpsServer, Chia   int
	NhanFps, NhanChia int
	FpsGui, KbpsGui   float64
	FpsNhan, KbpsNhan float64
	KhungDu, ManhMat  int
	XemTruoc          []byte
	SoXemTruoc        int
	Luoi              []byte
	LuoiW, LuoiH      int
	SoLuoi            int
	DaNgat            bool
	LyDoNgat          string
}

// KetNoi – vào phòng rồi báo ngay "tu_tat_camera": camera chỉ bật khi người dùng bấm START.
func KetNoi(ten, host string, cong int, nhatKy func(string)) (*Khach, error) {
	tcp, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(cong)), 5*time.Second)
	if err != nil {
		return nil, err
	}
	tcp.(*net.TCPConn).SetNoDelay(true)
	k := &Khach{Ten: ten, tcp: tcp, fpsServer: FpsMacDinh, chia: 1, nhanFps: FpsMacDinh, nhanChia: 1,
		fpsCam: 15, ghep: MoiGhep(), nhatKy: nhatKy, lyDo: "chưa rõ"}
	sc := bufio.NewScanner(tcp)
	k.guiTCP(map[string]any{"loai": "vao", "ten": ten})
	tcp.SetReadDeadline(time.Now().Add(5 * time.Second))
	var chao struct {
		Loai    string `json:"loai"`
		Ma      uint16 `json:"ma"`
		CongUDP int    `json:"cong_udp"`
	}
	if !sc.Scan() || json.Unmarshal(sc.Bytes(), &chao) != nil || chao.Loai != "chao" {
		tcp.Close()
		return nil, errors.New("server không chào (không phải server hội thoại video?)")
	}
	tcp.SetReadDeadline(time.Time{})
	k.Ma = chao.Ma
	ip := net.ParseIP(host)
	if ip == nil {
		if ips, e := net.LookupIP(host); e == nil && len(ips) > 0 {
			ip = ips[0]
		}
	}
	k.udp, err = net.DialUDP("udp", nil, &net.UDPAddr{IP: ip, Port: chao.CongUDP})
	if err != nil {
		tcp.Close()
		return nil, err
	}
	k.udp.SetReadBuffer(1 << 20)
	k.log("Vào phòng, mã %d; video UDP %d", k.Ma, chao.CongUDP)
	k.guiTCP(map[string]any{"loai": "tu_tat_camera"})
	go k.docDieuKhien(sc)
	go k.nhanUDP()
	go k.vongGui()
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
		case "camera":
			k.camMo, _ = tin["mo"].(bool)
			k.lyDo = fmt.Sprint(tin["ly_do"])
			k.fpsServer, k.chia = SoNguyen(tin["fps"], k.fpsServer), SoNguyen(tin["chia"], k.chia)
			if k.camMo {
				k.tuChoi = ""
			}
			k.log("Camera: %v, gửi tối đa %d fps, 1/%d", tin["ly_do"], k.fpsServer, k.chia)
		case "nhan":
			k.nhanFps, k.nhanChia = SoNguyen(tin["fps"], k.nhanFps), SoNguyen(tin["chia"], k.nhanChia)
			k.log("Nhận: %d fps, độ phân giải 1/%d", k.nhanFps, k.nhanChia)
		case "su_kien":
			k.log("Sự kiện: %v", tin["noi_dung"])
		case "tu_choi":
			k.tuChoi = fmt.Sprint(tin["ly_do"])
			k.log("Bị từ chối: %v", tin["ly_do"])
		}
		k.mu.Unlock()
	}
	k.mu.Lock()
	if !k.ngat {
		k.ngat, k.lyDoNgat = true, "server đóng kết nối"
		k.log("Server đóng kết nối")
	}
	k.mu.Unlock()
	k.tcp.Close()
	k.udp.Close()
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
			continue // Windows: lỗi ICMP tạm thời
		}
		h, payload, ok := TachHeader(buf[:n])
		if !ok || h.Loai != LoaiManh {
			continue
		}
		k.mu.Lock()
		nen := k.ghep.Them(h, payload, time.Now())
		k.mu.Unlock()
		if nen == nil {
			continue
		}
		anh, err := GiaiNen(nen)
		if err != nil || len(anh) != int(h.W)*int(h.H)*3 {
			continue
		}
		k.mu.Lock()
		k.luoi, k.luoiW, k.luoiH = anh, int(h.W), int(h.H)
		k.soLuoi++
		k.demNhan++
		k.byteNhan += len(nen)
		k.mu.Unlock()
	}
}

func (k *Khach) vongGui() {
	k.udp.Write(GoiGiu(k.Ma))
	soKhung := 0
	hanGui, mocDo := time.Now(), time.Now()
	hanGiu := mocDo.Add(time.Second)
	for {
		bayGio := time.Now()
		k.mu.Lock()
		if k.ngat {
			k.mu.Unlock()
			return
		}
		nguon, mo, fps, d := k.nguon, k.camMo, min(k.fpsCam, k.fpsServer), k.chia
		k.mu.Unlock()
		if mo && nguon != nil && !bayGio.Before(hanGui) {
			hanGui = hanGui.Add(time.Second / time.Duration(max(1, fps)))
			if hanGui.Before(bayGio) {
				hanGui = bayGio
			}
			if goc := nguon.LayKhung(soKhung); goc != nil {
				anh, w, h := ThuNho(goc, Rong, Cao, d)
				nen := NenZlib(anh)
				for _, g := range ChiaManh(k.Ma, uint32(soKhung), w, h, nen) {
					k.udp.Write(g)
				}
				soKhung++
				k.mu.Lock()
				k.xemTruoc = PhongTo(anh, w, h, d)
				k.soXemTruoc++
				k.demGui++
				k.byteGui += len(nen)
				k.mu.Unlock()
			}
		} else if (!mo || nguon == nil) && !bayGio.Before(hanGiu) {
			k.udp.Write(GoiGiu(k.Ma)) // giữ kết nối khi tắt camera
			hanGiu = bayGio.Add(time.Second)
		}
		if t := bayGio.Sub(mocDo); t >= time.Second {
			k.mu.Lock()
			s := t.Seconds()
			k.fpsGui, k.kbpsGui = float64(k.demGui)/s, float64(k.byteGui)*8/s/1000
			k.fpsNhan, k.kbpsNhan = float64(k.demNhan)/s, float64(k.byteNhan)*8/s/1000
			k.demGui, k.byteGui, k.demNhan, k.byteNhan = 0, 0, 0, 0
			k.mu.Unlock()
			mocDo = bayGio
		}
		time.Sleep(2 * time.Millisecond)
	}
}

// BatCamera – người dùng bấm START: dùng nguồn này và xin server bật camera.
func (k *Khach) BatCamera(n NguonHinh) {
	k.mu.Lock()
	k.nguon, k.tuChoi = n, ""
	k.mu.Unlock()
	k.log("START camera -> xin server bật")
	k.guiTCP(map[string]any{"loai": "tu_bat_camera"})
}

// TatCamera – người dùng bấm CANCEL: tự tắt, có hiệu lực ngay. Nguồn do bên gọi tự đóng.
func (k *Khach) TatCamera() {
	k.mu.Lock()
	k.nguon, k.camMo, k.xemTruoc = nil, false, nil
	k.soXemTruoc++
	k.mu.Unlock()
	k.log("CANCEL camera (tự tắt)")
	k.guiTCP(map[string]any{"loai": "tu_tat_camera"})
}

// DatNhan – tham số nhận lưới (fps, độ phân giải 1 hoặc 2); server xác nhận bằng "nhan".
func (k *Khach) DatNhan(fps, chia int) {
	k.guiTCP(map[string]any{"loai": "nhan", "fps": fps, "chia": chia})
}

func (k *Khach) TrangThai(daXem, daLuoi int) TrangThaiKhach {
	k.mu.Lock()
	defer k.mu.Unlock()
	t := TrangThaiKhach{Ma: k.Ma, CamMo: k.camMo, CoNguon: k.nguon != nil, LyDo: k.lyDo, TuChoi: k.tuChoi,
		FpsServer: k.fpsServer, Chia: k.chia, NhanFps: k.nhanFps, NhanChia: k.nhanChia,
		FpsGui: k.fpsGui, KbpsGui: k.kbpsGui, FpsNhan: k.fpsNhan, KbpsNhan: k.kbpsNhan,
		KhungDu: k.ghep.KhungDu, ManhMat: k.ghep.ManhMat, SoXemTruoc: k.soXemTruoc, SoLuoi: k.soLuoi,
		LuoiW: k.luoiW, LuoiH: k.luoiH, DaNgat: k.ngat, LyDoNgat: k.lyDoNgat}
	if daXem != k.soXemTruoc && k.xemTruoc != nil {
		t.XemTruoc = append([]byte(nil), k.xemTruoc...)
	}
	if daLuoi != k.soLuoi && k.luoi != nil {
		t.Luoi = append([]byte(nil), k.luoi...)
	}
	return t
}

func (k *Khach) Ngat() {
	k.mu.Lock()
	if !k.ngat {
		k.ngat, k.lyDoNgat = true, "đã ngắt kết nối"
	}
	k.mu.Unlock()
	k.tcp.Close()
	k.udp.Close()
}
