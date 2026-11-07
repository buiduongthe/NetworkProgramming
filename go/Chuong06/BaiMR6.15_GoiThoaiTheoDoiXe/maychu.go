// Bài mở rộng MR6.15 – Máy chủ trung gian Gọi thoại & Theo dõi tài xế (bản Go: net/http + gorilla/websocket + Pion).
//
// Một tiến trình phục vụ ba việc, như "tổng đài ẩn số" của các ứng dụng gọi xe (cùng giao thức với bản Python và
// app Android – xem chú thích đầu may_chu.py của bản Python):
//  1. Báo hiệu qua WebSocket /ws (JSON): đăng nhập, danh bạ, gọi – đổ chuông – trả lời/từ chối/hủy – kết thúc,
//     SDP/ICE, giữ máy, tắt micro, thống kê RTT/mất gói, lịch sử.
//  2. WebRTC qua máy chủ (SFU): mỗi điện thoại một PeerConnection tới máy chủ. Gói RTP Opus của bên này được CHUYỂN
//     NGUYÊN sang track gửi tới bên kia (không giải mã, không mã hóa lại – khác bản Python dùng aiortc), đồng thời
//     ghi thẳng gói Opus vào tệp Ogg (pion/webrtc/pkg/media/oggwriter). Sau cuộc gọi trộn hai bên bằng ffmpeg.
//     Mọi kết nối WebRTC dùng chung MỘT cổng UDP (ICE UDP mux) – chỉ cần mở một cổng trên tường lửa.
//  3. MQTT broker (mqtt.go, cổng 1883) cho vị trí tài xế; máy chủ tự đăng ký xe/# để vẽ bản đồ trên bảng điều khiển.
//
// Chạy:  go run . [host] [cổng_http] [cổng_mqtt] [--udp 50000] [--tls] [--thu-muc ban_ghi]
//
//	(mặc định 0.0.0.0 8080 1883; --tls: https/wss với chứng chỉ tự ký, in vân tay SHA-256 để nhập vào app)
package main

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"crypto/x509/pkix"
	_ "embed"
	"encoding/json"
	"encoding/pem"
	"fmt"
	"math/big"
	"net"
	"net/http"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/gorilla/websocket"
	"github.com/pion/ice/v4"
	"github.com/pion/interceptor"
	"github.com/pion/webrtc/v4"
	"github.com/pion/webrtc/v4/pkg/media/oggwriter"
)

//go:embed bang_dieu_khien.html
var trangBangDieuKhien []byte

const doChuongToiDa = 30 * time.Second

func log(format string, a ...any) {
	fmt.Printf("[may_chu] %s %s\n", time.Now().Format("15:04:05"), fmt.Sprintf(format, a...))
}

func mmSS(giay float64) string { g := int(giay); return fmt.Sprintf("%02d:%02d", g/60, g%60) }

type M = map[string]any

// ===================================================================== người dùng, cuộc gọi
type ketNoiWs struct {
	c    *websocket.Conn
	khoa sync.Mutex
}

func (k *ketNoiWs) gui(v any) {
	b, _ := json.Marshal(v)
	k.khoa.Lock()
	defer k.khoa.Unlock()
	k.c.SetWriteDeadline(time.Now().Add(5 * time.Second))
	k.c.WriteMessage(websocket.TextMessage, b)
}

type NguoiDung struct {
	Ma, Ten, VaiTro, BienSo, Xe, Chuyen, DiemDon string
	DongYGhiAm                                   bool
	ws                                           *ketNoiWs // nil = không trực tuyến
	cuoc                                         string    // mã cuộc đang tham gia
}

var khongPhaiChuSo = regexp.MustCompile(`[^0-9A-Za-z]`)

func (n *NguoiDung) maXe() string {
	if m := strings.ToUpper(khongPhaiChuSo.ReplaceAllString(n.BienSo, "")); m != "" {
		return m
	}
	return n.Ma
}

func (n *NguoiDung) moTa() string {
	p := []string{"Khách"}
	if n.VaiTro == "tai_xe" {
		p = []string{"Tài xế"}
		for _, x := range []string{n.BienSo, n.Xe} {
			if x != "" {
				p = append(p, x)
			}
		}
	} else {
		if n.Chuyen != "" {
			p = append(p, "chuyến "+n.Chuyen)
		}
		if n.DiemDon != "" {
			p = append(p, "đón tại "+n.DiemDon)
		}
	}
	return strings.Join(p, " · ")
}

func (n *NguoiDung) the() M {
	return M{"ma": n.Ma, "ten": n.Ten, "vai_tro": n.VaiTro, "mo_ta": n.moTa(), "ma_xe": n.maXe(),
		"truc_tuyen": n.ws != nil, "ban": n.cuoc != ""}
}

func (n *NguoiDung) gui(v any) {
	if ws := n.ws; ws != nil {
		ws.gui(v)
	}
}

// DuongAm: phần của MỘT bên trong cuộc gọi.
type DuongAm struct {
	nd         *NguoiDung
	pc         *webrtc.PeerConnection
	tiep       *webrtc.TrackLocalStaticRTP // track máy chủ gửi tới người này
	ungVienCho []webrtc.ICECandidateInit
	ghi        *oggwriter.OggWriter
	tepGhi     string
	t0         time.Time
	soGoiGhi   int
	rttMs      *float64
	matGoi     *float64
}

type CuocGoi struct {
	ma                      string
	goi, nghe               *NguoiDung
	trangThai               string // do_chuong, dang_noi, ket_thuc
	tTao, tTraLoi, tKetThuc time.Time
	ghiAm                   bool
	giuMay                  atomic.Bool
	khoa                    sync.Mutex // bảo vệ duong và các trường trong DuongAm
	duong                   map[string]*DuongAm
	hanChuong               *time.Timer
}

func (c *CuocGoi) benKia(n *NguoiDung) *NguoiDung {
	if n == c.goi {
		return c.nghe
	}
	return c.goi
}

func (c *CuocGoi) thoiLuong() float64 {
	if c.tTraLoi.IsZero() {
		return 0
	}
	kt := c.tKetThuc
	if kt.IsZero() {
		kt = time.Now()
	}
	return kt.Sub(c.tTraLoi).Seconds()
}

type viTriXe struct {
	MaXe, Ten, Nguon, TrangThai string
	Lat, Lon, TocDo             float64
	tNhan                       time.Time
	Vet                         [][2]float64
	coViTri                     bool
}

type MayChu struct {
	ban, diaChi, thuMucGhi, tepLichSu, ffmpeg string
	mu                                        sync.Mutex
	nguoi                                     map[string]*NguoiDung
	cuoc                                      map[string]*CuocGoi
	lichSu                                    []M
	api                                       *webrtc.API
	mqtt                                      *MqttBroker
	taiXe                                     map[string]*viTriXe
	diemViTri                                 []time.Time
	byteChuyen                                atomic.Int64
	kbitS                                     atomic.Value // float64
	tBatDau                                   time.Time
	dung                                      chan struct{}
}

// ===================================================================== WebRTC: Pion
func taoAPI(congUDP int) (*webrtc.API, error) {
	m := &webrtc.MediaEngine{}
	if err := m.RegisterCodec(webrtc.RTPCodecParameters{
		RTPCodecCapability: webrtc.RTPCodecCapability{MimeType: webrtc.MimeTypeOpus, ClockRate: 48000, Channels: 2,
			SDPFmtpLine: "minptime=10;useinbandfec=1"},
		PayloadType: 111,
	}, webrtc.RTPCodecTypeAudio); err != nil {
		return nil, err
	}
	ir := &interceptor.Registry{}
	if err := webrtc.RegisterDefaultInterceptors(m, ir); err != nil { // NACK, báo cáo RTCP
		return nil, err
	}
	mux, err := ice.NewMultiUDPMuxFromPort(congUDP, ice.UDPMuxFromPortWithLoopback())
	if err != nil {
		return nil, err
	}
	se := webrtc.SettingEngine{}
	se.SetICEUDPMux(mux)
	se.SetIncludeLoopbackCandidate(true) // để thử trên 127.0.0.1
	se.SetNetworkTypes([]webrtc.NetworkType{webrtc.NetworkTypeUDP4})
	return webrtc.NewAPI(webrtc.WithMediaEngine(m), webrtc.WithInterceptorRegistry(ir), webrtc.WithSettingEngine(se)), nil
}

// ===================================================================== danh bạ, báo hiệu
func (mc *MayChu) danhBaLocked() []M {
	ds := make([]*NguoiDung, 0, len(mc.nguoi))
	for _, n := range mc.nguoi {
		ds = append(ds, n)
	}
	sort.Slice(ds, func(i, j int) bool {
		if (ds[i].ws == nil) != (ds[j].ws == nil) {
			return ds[i].ws != nil
		}
		return ds[i].Ten < ds[j].Ten
	})
	kq := make([]M, len(ds))
	for i, n := range ds {
		kq[i] = n.the()
	}
	return kq
}

func (mc *MayChu) phatDanhBa() {
	mc.mu.Lock()
	tin := M{"loai": "danh_ba", "ds": mc.danhBaLocked()}
	var ws []*ketNoiWs
	for _, n := range mc.nguoi {
		if n.ws != nil {
			ws = append(ws, n.ws)
		}
	}
	mc.mu.Unlock()
	for _, w := range ws {
		w.gui(tin)
	}
}

var nangCap = websocket.Upgrader{CheckOrigin: func(*http.Request) bool { return true }}

func chuoiTin(t M, k string) string { s, _ := t[k].(string); return s }

func (mc *MayChu) xuLyWs(w http.ResponseWriter, r *http.Request) {
	c, err := nangCap.Upgrade(w, r, nil)
	if err != nil {
		return
	}
	k := &ketNoiWs{c: c}
	var nd *NguoiDung
	diaChi, _, _ := net.SplitHostPort(r.RemoteAddr)
	defer func() {
		c.Close()
		if nd == nil {
			return
		}
		mc.mu.Lock()
		cuaToi := nd.ws == k
		var cg *CuocGoi
		if cuaToi {
			nd.ws = nil
			cg = mc.cuoc[nd.cuoc]
		}
		mc.mu.Unlock()
		if cuaToi {
			log("%s (%s) rời mạng", nd.Ten, nd.Ma)
			if cg != nil {
				mc.ketThuc(cg, "mat_ket_noi", nd.Ten+" mất kết nối")
			}
			mc.phatDanhBa()
		}
	}()
	c.SetReadLimit(1 << 20)
	c.SetPongHandler(func(string) error { c.SetReadDeadline(time.Now().Add(60 * time.Second)); return nil })
	go func() { // giữ kết nối: ping 20 s/lần
		for range time.Tick(20 * time.Second) {
			k.khoa.Lock()
			err := c.WriteControl(websocket.PingMessage, nil, time.Now().Add(5*time.Second))
			k.khoa.Unlock()
			if err != nil {
				return
			}
		}
	}()
	for {
		c.SetReadDeadline(time.Now().Add(60 * time.Second))
		_, b, err := c.ReadMessage()
		if err != nil {
			return
		}
		var tin M
		if json.Unmarshal(b, &tin) != nil {
			k.gui(M{"loai": "loi", "ly_do": "tin không phải JSON"})
			continue
		}
		loai := chuoiTin(tin, "loai")
		if loai == "dang_nhap" {
			if n := mc.dangNhap(k, tin, diaChi); n != nil {
				nd = n
			}
		} else if nd == nil {
			k.gui(M{"loai": "loi", "ly_do": "chưa đăng nhập"})
		} else {
			mc.xuLyTin(nd, loai, tin)
		}
	}
}

func catChuoi(s string, n int) string {
	r := []rune(strings.TrimSpace(s))
	if len(r) > n {
		r = r[:n]
	}
	return string(r)
}

func (mc *MayChu) dangNhap(k *ketNoiWs, tin M, diaChi string) *NguoiDung {
	ma := catChuoi(fmt.Sprint(tin["ma"]), 40)
	if tin["ma"] == nil || ma == "" || len([]rune(ma)) > 20 {
		k.gui(M{"loai": "loi", "ly_do": "mã người dùng 1–20 ký tự"})
		return nil
	}
	mc.mu.Lock()
	nd := mc.nguoi[ma]
	if nd == nil {
		nd = &NguoiDung{Ma: ma}
		mc.nguoi[ma] = nd
	}
	cu := nd.ws
	nd.ws = k
	nd.Ten = catChuoi(chuoiTin(tin, "ten"), 40)
	if nd.Ten == "" {
		nd.Ten = ma
	}
	nd.VaiTro = "khach"
	if chuoiTin(tin, "vai_tro") == "tai_xe" {
		nd.VaiTro = "tai_xe"
	}
	nd.BienSo, nd.Xe = catChuoi(chuoiTin(tin, "bien_so"), 40), catChuoi(chuoiTin(tin, "xe"), 40)
	nd.Chuyen, nd.DiemDon = catChuoi(chuoiTin(tin, "chuyen"), 40), catChuoi(chuoiTin(tin, "diem_don"), 40)
	nd.DongYGhiAm, _ = tin["dong_y_ghi_am"].(bool)
	moTa, maXe := nd.moTa(), nd.maXe()
	mc.mu.Unlock()
	if cu != nil && cu != k { // đăng nhập lại từ máy khác: đóng kết nối cũ
		cu.c.Close()
	}
	log("%s (%s, %s) đăng nhập từ %s", nd.Ten, ma, moTa, diaChi)
	k.gui(M{"loai": "dang_nhap_ok", "ma": ma, "ma_xe": maXe, "may_chu": mc.ban})
	mc.phatDanhBa()
	return nd
}

func (mc *MayChu) xuLyTin(nd *NguoiDung, loai string, tin M) {
	switch loai {
	case "goi":
		mc.batDauGoi(nd, fmt.Sprint(tin["den"]))
		return
	case "lich_su":
		mc.mu.Lock()
		var ds []M
		for i := len(mc.lichSu) - 1; i >= 0 && len(ds) < 50; i-- {
			x := mc.lichSu[i]
			if x["goi"].(M)["ma"] == nd.Ma || x["nghe"].(M)["ma"] == nd.Ma {
				ds = append(ds, x)
			}
		}
		mc.mu.Unlock()
		if ds == nil {
			ds = []M{}
		}
		nd.gui(M{"loai": "lich_su", "ds": ds})
		return
	}
	maCuoc := chuoiTin(tin, "ma_cuoc")
	mc.mu.Lock()
	c := mc.cuoc[maCuoc]
	if c == nil || (nd != c.goi && nd != c.nghe) {
		mc.mu.Unlock()
		switch loai { // tin trễ của cuộc gọi vừa kết thúc (thống kê, ICE…) là bình thường: bỏ qua
		case "thong_ke", "ice", "sdp", "giu_may", "tat_mic":
		default:
			nd.gui(M{"loai": "loi", "ly_do": "không có cuộc gọi này", "ma_cuoc": maCuoc})
		}
		return
	}
	trangThai := c.trangThai
	switch {
	case loai == "tra_loi" && nd == c.nghe && trangThai == "do_chuong":
		c.trangThai, c.tTraLoi = "dang_noi", time.Now()
		if c.hanChuong != nil {
			c.hanChuong.Stop()
		}
		mc.mu.Unlock()
		log("%s: %s trả lời", c.ma, nd.Ten)
		for _, x := range []*NguoiDung{c.goi, c.nghe} {
			x.gui(M{"loai": "da_tra_loi", "ma_cuoc": c.ma, "ghi_am": c.ghiAm})
		}
		return
	}
	mc.mu.Unlock()
	switch loai {
	case "tu_choi":
		if nd == c.nghe && trangThai == "do_chuong" {
			lyDo := chuoiTin(tin, "ly_do")
			if lyDo == "" {
				lyDo = "Từ chối"
			}
			mc.ketThuc(c, "tu_choi", lyDo)
		}
	case "huy", "ket_thuc":
		if trangThai == "do_chuong" {
			mc.ketThuc(c, "huy", nd.Ten+" hủy")
		} else {
			mc.ketThuc(c, "hoan_thanh", nd.Ten+" kết thúc")
		}
	case "sdp":
		if trangThai == "dang_noi" {
			sdp, _ := tin["sdp"].(map[string]any)
			mc.nhanOffer(c, nd, sdp)
		}
	case "ice":
		u, _ := tin["ung_vien"].(map[string]any)
		mc.nhanIce(c, nd, u)
	case "giu_may":
		bat, _ := tin["bat"].(bool)
		c.giuMay.Store(bat)
		c.benKia(nd).gui(M{"loai": "doi_phuong", "ma_cuoc": c.ma, "giu_may": bat})
	case "tat_mic":
		bat, _ := tin["bat"].(bool)
		c.benKia(nd).gui(M{"loai": "doi_phuong", "ma_cuoc": c.ma, "tat_mic": bat})
	case "thong_ke":
		c.khoa.Lock()
		if d := c.duong[nd.Ma]; d != nil {
			if v, ok := tin["rtt_ms"].(float64); ok {
				d.rttMs = &v
			}
			if v, ok := tin["mat_goi"].(float64); ok {
				d.matGoi = &v
			}
		}
		c.khoa.Unlock()
	}
}

// ===================================================================== vòng đời cuộc gọi
func (mc *MayChu) batDauGoi(nd *NguoiDung, maDen string) {
	mc.mu.Lock()
	den := mc.nguoi[maDen]
	if den == nil || den.ws == nil {
		mc.mu.Unlock()
		nd.gui(M{"loai": "loi", "ly_do": "người nhận không trực tuyến"})
		return
	}
	if den == nd || nd.cuoc != "" {
		mc.mu.Unlock()
		nd.gui(M{"loai": "loi", "ly_do": "không thể gọi lúc này"})
		return
	}
	ma := time.Now().Format("call-20060102-150405")
	for mc.cuoc[ma] != nil || mc.coTrongLichSu(ma) {
		ma += "x"
	}
	c := &CuocGoi{ma: ma, goi: nd, nghe: den, trangThai: "do_chuong", tTao: time.Now(), ghiAm: nd.DongYGhiAm,
		duong: map[string]*DuongAm{}}
	mc.cuoc[ma] = c
	nd.cuoc = ma
	ban := den.cuoc != ""
	if !ban {
		den.cuoc = ma
	}
	theDen, theGoi := den.the(), nd.the()
	mc.mu.Unlock()
	nd.gui(M{"loai": "do_chuong", "ma_cuoc": ma, "den": theDen})
	if ban { // máy bận: kết thúc ngay, vẫn ghi lịch sử
		mc.ketThuc(c, "ban", "Máy bận")
		return
	}
	ghi := ""
	if c.ghiAm {
		ghi = " (ghi âm)"
	}
	log("%s: %s gọi %s%s", ma, nd.Ten, den.Ten, ghi)
	den.gui(M{"loai": "cuoc_goi_den", "ma_cuoc": ma, "tu": theGoi, "ghi_am": c.ghiAm})
	mc.phatDanhBa()
	c.hanChuong = time.AfterFunc(doChuongToiDa, func() {
		mc.mu.Lock()
		conChuong := c.trangThai == "do_chuong"
		mc.mu.Unlock()
		if conChuong {
			mc.ketThuc(c, "nho", "Không trả lời")
		}
	})
}

func (mc *MayChu) coTrongLichSu(ma string) bool {
	for _, x := range mc.lichSu {
		if x["ma_cuoc"] == ma {
			return true
		}
	}
	return false
}

func (mc *MayChu) nhanOffer(c *CuocGoi, nd *NguoiDung, sdp map[string]any) {
	if sdp == nil || sdp["type"] != "offer" {
		nd.gui(M{"loai": "loi", "ly_do": "sdp phải là offer"})
		return
	}
	pc, err := mc.api.NewPeerConnection(webrtc.Configuration{})
	if err != nil {
		nd.gui(M{"loai": "loi", "ly_do": "không tạo được kết nối WebRTC"})
		return
	}
	tiep, _ := webrtc.NewTrackLocalStaticRTP(webrtc.RTPCodecCapability{MimeType: webrtc.MimeTypeOpus, ClockRate: 48000,
		Channels: 2}, "am_thanh", "may_chu-"+nd.Ma)
	sender, _ := pc.AddTrack(tiep)
	go func() { // đọc RTCP của track gửi đi (bắt buộc để bộ chặn NACK/báo cáo hoạt động)
		buf := make([]byte, 1500)
		for {
			if _, _, err := sender.Read(buf); err != nil {
				return
			}
		}
	}()
	c.khoa.Lock()
	cu := c.duong[nd.Ma]
	d := &DuongAm{nd: nd, pc: pc, tiep: tiep}
	if cu != nil {
		d.ungVienCho = cu.ungVienCho
		if cu.pc != nil {
			cu.pc.Close() // gửi lại offer (kết nối lại): bỏ kết nối cũ
		}
	}
	c.duong[nd.Ma] = d
	c.khoa.Unlock()

	pc.OnTrack(func(t *webrtc.TrackRemote, _ *webrtc.RTPReceiver) {
		if t.Kind() != webrtc.RTPCodecTypeAudio {
			return
		}
		log("%s: nhận âm thanh của %s (%s)", c.ma, nd.Ten, t.Codec().MimeType)
		kia := c.benKia(nd).Ma
		var ghi *oggwriter.OggWriter
		if c.ghiAm {
			tep := filepath.Join(mc.thuMucGhi, fmt.Sprintf("%s-%s.ogg", c.ma, nd.Ma))
			if w, err := oggwriter.New(tep, 48000, 2); err == nil {
				ghi = w
				c.khoa.Lock()
				d.ghi, d.tepGhi = w, tep
				c.khoa.Unlock()
			}
		}
		for {
			pkt, _, err := t.ReadRTP()
			if err != nil {
				return
			}
			if !c.giuMay.Load() { // giữ máy: ngừng chuyển cả hai chiều (vẫn ghi âm)
				c.khoa.Lock()
				dk := c.duong[kia]
				c.khoa.Unlock()
				if dk != nil {
					if dk.tiep.WriteRTP(pkt) == nil {
						mc.byteChuyen.Add(int64(pkt.MarshalSize()))
					}
				}
			}
			if ghi != nil {
				c.khoa.Lock()
				if d.ghi != nil { // nil = cuộc gọi đã kết thúc, tệp đã đóng
					if d.soGoiGhi == 0 {
						d.t0 = time.Now()
					}
					if d.ghi.WriteRTP(pkt) == nil {
						d.soGoiGhi++
					}
				}
				c.khoa.Unlock()
			}
		}
	})
	pc.OnConnectionStateChange(func(s webrtc.PeerConnectionState) {
		log("%s: WebRTC của %s: %s", c.ma, nd.Ten, s)
		if s == webrtc.PeerConnectionStateFailed {
			go mc.ketThuc(c, "mat_ket_noi", "WebRTC của "+nd.Ten+" thất bại")
		}
	})
	if err := pc.SetRemoteDescription(webrtc.SessionDescription{Type: webrtc.SDPTypeOffer,
		SDP: fmt.Sprint(sdp["sdp"])}); err != nil {
		nd.gui(M{"loai": "loi", "ly_do": "offer không hợp lệ: " + err.Error()})
		return
	}
	c.khoa.Lock()
	cho := d.ungVienCho
	d.ungVienCho = nil
	c.khoa.Unlock()
	for _, u := range cho {
		pc.AddICECandidate(u)
	}
	answer, err := pc.CreateAnswer(nil)
	if err != nil {
		nd.gui(M{"loai": "loi", "ly_do": err.Error()})
		return
	}
	xong := webrtc.GatheringCompletePromise(pc) // chờ thu thập ICE xong: answer chứa đủ ứng viên (không trickle)
	pc.SetLocalDescription(answer)
	<-xong
	ld := pc.LocalDescription()
	nd.gui(M{"loai": "sdp", "ma_cuoc": c.ma, "sdp": M{"type": ld.Type.String(), "sdp": ld.SDP}})
}

func (mc *MayChu) nhanIce(c *CuocGoi, nd *NguoiDung, u map[string]any) {
	cand, _ := u["candidate"].(string)
	if cand == "" {
		return
	}
	ic := webrtc.ICECandidateInit{Candidate: cand}
	if s, ok := u["sdpMid"].(string); ok {
		ic.SDPMid = &s
	}
	if f, ok := u["sdpMLineIndex"].(float64); ok {
		v := uint16(f)
		ic.SDPMLineIndex = &v
	}
	c.khoa.Lock()
	d := c.duong[nd.Ma]
	if d == nil {
		d = &DuongAm{nd: nd}
		c.duong[nd.Ma] = d
	}
	pc := d.pc
	if pc == nil {
		d.ungVienCho = append(d.ungVienCho, ic)
	}
	c.khoa.Unlock()
	if pc != nil {
		pc.AddICECandidate(ic)
	}
}

func (mc *MayChu) ketThuc(c *CuocGoi, ketQua, lyDo string) {
	mc.mu.Lock()
	if c == nil || mc.cuoc[c.ma] != c {
		mc.mu.Unlock()
		return
	}
	delete(mc.cuoc, c.ma)
	c.trangThai, c.tKetThuc = "ket_thuc", time.Now()
	if c.hanChuong != nil {
		c.hanChuong.Stop()
	}
	for _, x := range []*NguoiDung{c.goi, c.nghe} {
		if x.cuoc == c.ma {
			x.cuoc = ""
		}
	}
	mc.mu.Unlock()
	log("%s: kết thúc (%s: %s), thời lượng %s", c.ma, ketQua, lyDo, mmSS(c.thoiLuong()))
	for _, x := range []*NguoiDung{c.goi, c.nghe} {
		x.gui(M{"loai": "da_ket_thuc", "ma_cuoc": c.ma, "ket_qua": ketQua, "ly_do": lyDo,
			"thoi_luong": float64(int(c.thoiLuong()*10)) / 10})
	}
	banGhi := M{}
	c.khoa.Lock()
	var dong []*webrtc.PeerConnection
	var t0 = map[string]time.Time{}
	for ma, d := range c.duong {
		if d.pc != nil {
			dong = append(dong, d.pc)
		}
		if d.ghi != nil {
			d.ghi.Close()
			d.ghi = nil
			if d.soGoiGhi > 0 {
				banGhi[ma] = filepath.Base(d.tepGhi)
				t0[ma] = d.t0
			}
		}
	}
	c.khoa.Unlock()
	for _, pc := range dong {
		pc.Close()
	}
	if len(banGhi) == 2 {
		if tron := mc.tron(c, t0[c.goi.Ma], t0[c.nghe.Ma]); tron != "" {
			banGhi["tron"] = tron
		}
	}
	rec := M{"ma_cuoc": c.ma, "goi": M{"ma": c.goi.Ma, "ten": c.goi.Ten}, "nghe": M{"ma": c.nghe.Ma, "ten": c.nghe.Ten},
		"bat_dau": c.tTao.Format("2006-01-02 15:04:05"), "thoi_luong": float64(int(c.thoiLuong()*10)) / 10,
		"ket_qua": ketQua, "ly_do": lyDo, "ban_ghi": banGhi}
	if len(banGhi) > 0 {
		var tep []string
		if t, ok := banGhi["tron"].(string); ok {
			tep = []string{t}
		} else {
			for _, v := range banGhi {
				tep = append(tep, v.(string))
			}
		}
		var tong int64
		for _, t := range tep {
			if fi, err := os.Stat(filepath.Join(mc.thuMucGhi, t)); err == nil {
				tong += fi.Size()
			}
		}
		rec["kich_thuoc"] = tong
	}
	mc.mu.Lock()
	mc.lichSu = append(mc.lichSu, rec)
	if len(mc.lichSu) > 500 {
		mc.lichSu = mc.lichSu[len(mc.lichSu)-500:]
	}
	b, _ := json.MarshalIndent(mc.lichSu, "", " ")
	mc.mu.Unlock()
	os.WriteFile(mc.tepLichSu, b, 0o644)
	mc.phatDanhBa()
}

// tron trộn hai bên thành <mã cuộc>.ogg bằng ffmpeg (adelay bù chênh thời điểm bắt đầu ghi của hai tệp).
func (mc *MayChu) tron(c *CuocGoi, t0Goi, t0Nghe time.Time) string {
	if mc.ffmpeg == "" {
		log("không có ffmpeg: bỏ qua bước trộn (app phát hai tệp cùng lúc)")
		return ""
	}
	tre := t0Nghe.Sub(t0Goi).Milliseconds()
	a := filepath.Join(mc.thuMucGhi, fmt.Sprintf("%s-%s.ogg", c.ma, c.goi.Ma))
	b := filepath.Join(mc.thuMucGhi, fmt.Sprintf("%s-%s.ogg", c.ma, c.nghe.Ma))
	ra := filepath.Join(mc.thuMucGhi, c.ma+".ogg")
	loc := fmt.Sprintf("[1:a]adelay=%d|%d[b];[0:a][b]", tre, tre)
	if tre < 0 {
		loc = fmt.Sprintf("[0:a]adelay=%d|%d[a];[a][1:a]", -tre, -tre)
	}
	loc += "amix=inputs=2:duration=longest:normalize=0"
	out, err := exec.Command(mc.ffmpeg, "-y", "-loglevel", "error", "-i", a, "-i", b, "-filter_complex", loc,
		"-ac", "1", "-c:a", "libopus", "-b:a", "32k", ra).CombinedOutput()
	if err != nil {
		log("ffmpeg lỗi: %s %s", err, strings.TrimSpace(string(out)))
		return ""
	}
	log("%s: đã trộn hai bên -> %s (lệch %d ms)", c.ma, filepath.Base(ra), tre)
	return filepath.Base(ra)
}

// ===================================================================== MQTT: tài xế trên bản đồ
func (mc *MayChu) nhanMqtt(chuDe string, nd []byte) {
	p := strings.Split(chuDe, "/")
	if len(p) != 3 {
		return
	}
	mc.mu.Lock()
	defer mc.mu.Unlock()
	x := mc.taiXe[p[1]]
	if x == nil {
		x = &viTriXe{MaXe: p[1], Ten: p[1], TrangThai: "?"}
		mc.taiXe[p[1]] = x
	}
	switch p[2] {
	case "vitri":
		var v struct {
			Lat, Lon *float64
			TocDo    float64 `json:"toc_do"`
			Nguon    string  `json:"nguon"`
			Ten      string  `json:"ten"`
		}
		if json.Unmarshal(nd, &v) != nil || v.Lat == nil || v.Lon == nil {
			return
		}
		x.Lat, x.Lon, x.TocDo, x.Nguon, x.tNhan, x.TrangThai, x.coViTri = *v.Lat, *v.Lon, v.TocDo, v.Nguon, time.Now(), "truc_tuyen", true
		if v.Ten != "" {
			x.Ten = v.Ten
		}
		x.Vet = append(x.Vet, [2]float64{x.Lat, x.Lon})
		if len(x.Vet) > 120 {
			x.Vet = x.Vet[len(x.Vet)-120:]
		}
		mc.diemViTri = append(mc.diemViTri, time.Now())
	case "trangthai":
		x.TrangThai = string(nd)
	}
}

// ===================================================================== bảng điều khiển
func (mc *MayChu) doBangThong() {
	var truoc int64
	for range time.Tick(2 * time.Second) {
		bay := mc.byteChuyen.Load()
		mc.kbitS.Store(float64(bay-truoc) * 8 / 2 / 1000)
		truoc = bay
	}
}

func lamTron(x float64) float64 { return float64(int(x*10+0.5)) / 10 }

func (mc *MayChu) trangThai() M {
	mc.mu.Lock()
	defer mc.mu.Unlock()
	bayGio := time.Now()
	for len(mc.diemViTri) > 0 && bayGio.Sub(mc.diemViTri[0]) > 5*time.Second {
		mc.diemViTri = mc.diemViTri[1:]
	}
	tt, khach, taiXe := 0, 0, 0
	for _, n := range mc.nguoi {
		if n.ws != nil {
			tt++
			if n.VaiTro == "tai_xe" {
				taiXe++
			} else {
				khach++
			}
		}
	}
	cuoc := []M{}
	dangGoi := 0
	for _, c := range mc.cuoc {
		var rtt, mat any
		c.khoa.Lock()
		for _, d := range c.duong {
			if d.rttMs != nil && (rtt == nil || *d.rttMs > rtt.(float64)) {
				rtt = *d.rttMs
			}
			if d.matGoi != nil && (mat == nil || *d.matGoi > mat.(float64)) {
				mat = *d.matGoi
			}
		}
		c.khoa.Unlock()
		tl := "đổ chuông"
		if c.trangThai == "dang_noi" {
			tl = mmSS(c.thoiLuong())
			dangGoi++
		}
		cuoc = append(cuoc, M{"ma": c.ma, "ai": c.goi.Ten + " ↔ " + c.nghe.Ten, "trang_thai": c.trangThai,
			"thoi_luong": tl, "rtt_ms": rtt, "mat_goi": mat, "ghi_am": c.ghiAm, "giu_may": c.giuMay.Load()})
	}
	banGhi := []M{}
	homNay := bayGio.Format("2006-01-02")
	cuocHomNay := 0
	for i := len(mc.lichSu) - 1; i >= 0; i-- {
		x := mc.lichSu[i]
		if strings.HasPrefix(fmt.Sprint(x["bat_dau"]), homNay) {
			cuocHomNay++
		}
		bg, _ := x["ban_ghi"].(M)
		if len(bg) == 0 || len(banGhi) >= 8 {
			continue
		}
		tep, _ := bg["tron"].(string)
		if tep == "" {
			for _, v := range bg {
				tep = fmt.Sprint(v)
				break
			}
		}
		tl, _ := x["thoi_luong"].(float64)
		banGhi = append(banGhi, M{"tep": tep, "cac_tep": bg, "ai": x["goi"].(M)["ten"].(string) + " ↔ " +
			x["nghe"].(M)["ten"].(string), "thoi_luong": mmSS(tl), "kich_thuoc": x["kich_thuoc"], "bat_dau": x["bat_dau"]})
	}
	ds := []M{}
	ma := make([]string, 0, len(mc.taiXe))
	for k := range mc.taiXe {
		ma = append(ma, k)
	}
	sort.Strings(ma)
	for _, k := range ma {
		x := mc.taiXe[k]
		if !x.coViTri {
			continue
		}
		vet := x.Vet
		if len(vet) > 60 {
			vet = vet[len(vet)-60:]
		}
		ds = append(ds, M{"ma_xe": x.MaXe, "ten": x.Ten, "lat": x.Lat, "lon": x.Lon, "toc_do": x.TocDo, "nguon": x.Nguon,
			"trang_thai": x.TrangThai, "cach_s": lamTron(bayGio.Sub(x.tNhan).Seconds()), "vet": vet})
	}
	kb, _ := mc.kbitS.Load().(float64)
	return M{
		"may_chu": M{"ban": mc.ban, "dia_chi": mc.diaChi, "chay_s": int(bayGio.Sub(mc.tBatDau).Seconds()), "ffmpeg": mc.ffmpeg != ""},
		"so_lieu": M{"truc_tuyen": tt, "khach": khach, "tai_xe": taiXe, "dang_goi": dangGoi, "cuoc_hom_nay": cuocHomNay,
			"kbit_s": lamTron(kb), "diem_s": lamTron(float64(len(mc.diemViTri)) / 5), "mqtt_ket_noi": mc.mqtt.soKetNoi(),
			"mqtt_tin": mc.mqtt.soTin.Load()},
		"cuoc": cuoc, "ban_ghi": banGhi, "tai_xe": ds,
	}
}

// ===================================================================== TLS tự ký
func taoChungChi(thuMuc string) (string, string, string, error) {
	cert, key := filepath.Join(thuMuc, "may_chu.crt"), filepath.Join(thuMuc, "may_chu.key")
	if _, err := os.Stat(cert); err != nil {
		os.MkdirAll(thuMuc, 0o755)
		k, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
		if err != nil {
			return "", "", "", err
		}
		so, _ := rand.Int(rand.Reader, new(big.Int).Lsh(big.NewInt(1), 62))
		mau := &x509.Certificate{SerialNumber: so, Subject: pkix.Name{CommonName: "may-chu-goi-thoai-lab"},
			NotBefore: time.Now().Add(-24 * time.Hour), NotAfter: time.Now().Add(825 * 24 * time.Hour),
			KeyUsage: x509.KeyUsageDigitalSignature, ExtKeyUsage: []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth}}
		der, err := x509.CreateCertificate(rand.Reader, mau, mau, &k.PublicKey, k)
		if err != nil {
			return "", "", "", err
		}
		kb, _ := x509.MarshalPKCS8PrivateKey(k)
		os.WriteFile(key, pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: kb}), 0o600)
		os.WriteFile(cert, pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: der}), 0o644)
	}
	b, err := os.ReadFile(cert)
	if err != nil {
		return "", "", "", err
	}
	blk, _ := pem.Decode(b)
	h := sha256.Sum256(blk.Bytes)
	p := make([]string, len(h))
	for i, x := range h {
		p[i] = fmt.Sprintf("%02X", x)
	}
	return cert, key, strings.Join(p, ":"), nil
}

// ===================================================================== main
func main() {
	host, cong, congMqtt, congUDP := "0.0.0.0", 8080, 1883, 50000
	tls, thuMuc := false, ""
	var vt []string
	a := os.Args[1:]
	for i := 0; i < len(a); i++ {
		switch a[i] {
		case "--tls":
			tls = true
		case "--udp", "--thu-muc":
			if i+1 < len(a) {
				if a[i] == "--udp" {
					congUDP, _ = strconv.Atoi(a[i+1])
				} else {
					thuMuc = a[i+1]
				}
				i++
			}
		default:
			vt = append(vt, a[i])
		}
	}
	if len(vt) > 0 {
		host = vt[0]
	}
	if len(vt) > 1 {
		cong, _ = strconv.Atoi(vt[1])
	}
	if len(vt) > 2 {
		congMqtt, _ = strconv.Atoi(vt[2])
	}
	if thuMuc == "" {
		thuMuc = "ban_ghi"
	}
	os.MkdirAll(thuMuc, 0o755)
	api, err := taoAPI(congUDP)
	if err != nil {
		log("không mở được cổng UDP %d cho WebRTC: %v", congUDP, err)
		os.Exit(1)
	}
	mc := &MayChu{ban: "Go (Pion)", thuMucGhi: thuMuc, tepLichSu: filepath.Join(thuMuc, "lich_su.json"),
		nguoi: map[string]*NguoiDung{}, cuoc: map[string]*CuocGoi{}, api: api, taiXe: map[string]*viTriXe{},
		tBatDau: time.Now(), dung: make(chan struct{})}
	if b, err := os.ReadFile(mc.tepLichSu); err == nil {
		var ls []M
		if json.Unmarshal(b, &ls) == nil {
			mc.lichSu = ls
		}
	}
	if p, err := exec.LookPath("ffmpeg"); err == nil {
		mc.ffmpeg = p
	}
	mc.kbitS.Store(0.0)
	mc.mqtt = moiMqttBroker(func(t string) { log("mqtt: %s", t) })
	mc.mqtt.dangKyNoiBo("xe/#", mc.nhanMqtt)
	lnMqtt, err := mc.mqtt.batDau(net.JoinHostPort(host, strconv.Itoa(congMqtt)))
	if err != nil {
		log("không mở được cổng MQTT %d: %v", congMqtt, err)
		os.Exit(1)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /{$}", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		w.Write(trangBangDieuKhien)
	})
	mux.HandleFunc("GET /ws", mc.xuLyWs)
	mux.HandleFunc("GET /api/trangthai", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json; charset=utf-8")
		json.NewEncoder(w).Encode(mc.trangThai())
	})
	mux.HandleFunc("POST /api/ngat/{ma}", func(w http.ResponseWriter, r *http.Request) {
		mc.mu.Lock()
		c := mc.cuoc[r.PathValue("ma")]
		mc.mu.Unlock()
		mc.ketThuc(c, "hoan_thanh", "Máy chủ ngắt")
		w.Write([]byte(`{"ok":true}`))
	})
	mux.HandleFunc("POST /api/dung", func(w http.ResponseWriter, r *http.Request) {
		h, _, _ := net.SplitHostPort(r.RemoteAddr)
		if h != "127.0.0.1" && h != "::1" { // chỉ cho dừng từ chính máy chủ
			http.Error(w, "chỉ dừng được từ máy chủ (127.0.0.1)", http.StatusForbidden)
			return
		}
		w.Write([]byte(`{"ok":true}`))
		go func() { time.Sleep(300 * time.Millisecond); close(mc.dung) }()
	})
	tenTep := regexp.MustCompile(`^[A-Za-z0-9._-]+\.ogg$`)
	mux.HandleFunc("GET /banghi/{ten}", func(w http.ResponseWriter, r *http.Request) {
		ten := r.PathValue("ten")
		if !tenTep.MatchString(ten) {
			http.NotFound(w, r)
			return
		}
		w.Header().Set("Content-Type", "audio/ogg")
		http.ServeFile(w, r, filepath.Join(mc.thuMucGhi, ten))
	})
	srv := &http.Server{Addr: net.JoinHostPort(host, strconv.Itoa(cong)), Handler: mux}
	ln, err := net.Listen("tcp", srv.Addr)
	if err != nil {
		log("không mở được cổng %d: %v", cong, err)
		os.Exit(1)
	}
	lop := "ws"
	if tls {
		cert, key, vanTay, err := taoChungChi("chung_chi")
		if err != nil {
			log("không tạo được chứng chỉ: %v", err)
			os.Exit(1)
		}
		log("TLS: chứng chỉ tự ký, vân tay SHA-256 (nhập vào app): %s", vanTay)
		go srv.ServeTLS(ln, cert, key)
		lop = "wss"
	} else {
		go srv.Serve(ln)
	}
	mc.diaChi = fmt.Sprintf("%s://%s:%d/ws · MQTT %d · UDP %d", lop, host, cong, congMqtt, congUDP)
	ffmpeg := ""
	if mc.ffmpeg == "" {
		ffmpeg = " (không có ffmpeg: không trộn hai bên)"
	}
	log("Báo hiệu %s; bảng điều khiển %s://%s:%d/; ghi âm vào %s%s", mc.diaChi, map[bool]string{false: "http", true: "https"}[tls],
		host, cong, thuMuc, ffmpeg)
	go mc.doBangThong()

	tinHieu := make(chan os.Signal, 1)
	signal.Notify(tinHieu, os.Interrupt, syscall.SIGTERM)
	select {
	case <-tinHieu:
	case <-mc.dung:
	}
	mc.mu.Lock()
	var con []*CuocGoi
	for _, c := range mc.cuoc {
		con = append(con, c)
	}
	mc.mu.Unlock()
	for _, c := range con {
		mc.ketThuc(c, "hoan_thanh", "Máy chủ dừng")
	}
	ctx, huy := context.WithTimeout(context.Background(), 2*time.Second)
	defer huy()
	srv.Shutdown(ctx)
	lnMqtt.Close()
	mc.mu.Lock()
	log("Dừng. Đã phục vụ %d người dùng, %d cuộc gọi trong lịch sử, %d tin MQTT", len(mc.nguoi), len(mc.lichSu), mc.mqtt.soTin.Load())
	mc.mu.Unlock()
}
