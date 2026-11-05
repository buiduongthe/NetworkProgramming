package amthanh

// Micro và loa thật qua tiến trình ffmpeg / ffplay (chỉ dùng thư viện chuẩn: os/exec + ống dẫn).
//
// Thư viện chuẩn Go không có API âm thanh, nên ứng dụng giao diện nhờ hai chương trình ngoài:
//   - thu (micro):  ffmpeg -f <dshow|pulse|alsa|avfoundation> -i <thiết bị>
//     -ac 1 -ar 16000 -f s16le pipe:1   -> stdout là PCM 16 bit little-endian, mono, 16 kHz;
//   - phát (loa):   ffplay -nodisp -f s16le -ar 16000 -ch_layout mono -i pipe:0
//     (ffplay cũ trước 5.1 dùng -ac 1)  <- ta ghi PCM vào stdin của ffplay.
//
// Chú ý thứ tự byte: s16le là little-endian (giống WAV), còn RTP L16 là big-endian (thứ tự mạng).
// Không có ffmpeg/ffplay hoặc không mở được thiết bị -> chương trình tự chuyển sang nguồn giả lập
// (sóng sin) và chỉ đo mức, không phát ra loa.

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"math"
	"os"
	"os/exec"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"
)

// NguonAm – nơi lấy 320 mẫu (20 ms) mỗi lượt; trả về nil khi chưa có khung (micro thật chưa kịp thu).
type NguonAm interface{ LayKhung() []int16 }

// NguonSin – âm tổng hợp sin tần số F, biên độ A (đơn vị mẫu), pha liên tục giữa các khung.
type NguonSin struct {
	F, A float64
	n    int
}

func (s *NguonSin) LayKhung() []int16 {
	w := 2 * math.Pi * s.F / TanSoMau
	k := make([]int16, MauMoiKhung)
	for i := range k {
		k[i] = int16(math.Round(s.A * math.Sin(w*float64(s.n+i))))
	}
	s.n += MauMoiKhung
	return k
}

// ThietBi – một micro tìm thấy qua ffmpeg: DinhDang là giá trị "-f", DauVao là giá trị "-i".
type ThietBi struct {
	Ten, DinhDang, DauVao string
}

// CoLenh – chương trình ngoài (ffmpeg, ffplay, pactl…) có trong PATH không.
func CoLenh(ten string) bool {
	_, err := exec.LookPath(ten)
	return err == nil
}

// lenhNgam – tạo lệnh chạy ngầm (trên Windows: không bật cửa sổ console, xem an_windows.go).
func lenhNgam(ten string, args ...string) *exec.Cmd {
	c := exec.Command(ten, args...)
	anCuaSo(c)
	return c
}

// LietKeMicro – danh sách micro thật. Windows: ffmpeg liệt kê thiết bị DirectShow (in ra stderr);
// Linux: nguồn PulseAudio/PipeWire (pactl) hoặc ALSA mặc định; macOS: AVFoundation.
func LietKeMicro() []ThietBi {
	if !CoLenh("ffmpeg") {
		return nil
	}
	var ds []ThietBi
	switch runtime.GOOS {
	case "windows":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy").CombinedOutput()
		for _, ten := range tachDShow(string(out), "audio") {
			ds = append(ds, ThietBi{ten, "dshow", "audio=" + ten})
		}
	case "darwin":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", "").CombinedOutput()
		for so, ten := range tachAVFoundation(string(out), "audio") {
			ds = append(ds, ThietBi{ten, "avfoundation", ":" + so})
		}
	default: // Linux
		if CoLenh("pactl") {
			out, _ := lenhNgam("pactl", "list", "short", "sources").Output()
			for _, dong := range strings.Split(string(out), "\n") {
				if f := strings.Fields(dong); len(f) >= 2 && !strings.HasSuffix(f[1], ".monitor") {
					ds = append(ds, ThietBi{"Pulse: " + f[1], "pulse", f[1]})
				}
			}
		}
		if len(ds) == 0 {
			if _, err := os.Stat("/dev/snd"); err == nil {
				ds = append(ds, ThietBi{"ALSA: default", "alsa", "default"})
			}
		}
	}
	return ds
}

// tachDShow – đọc danh sách của "ffmpeg -list_devices true -f dshow": bản mới in mỗi dòng
// `"Tên" (audio)`/`"Tên" (video)`; bản cũ chia mục "DirectShow audio devices" / "… video devices".
func tachDShow(out, kieu string) []string {
	reMoi := regexp.MustCompile(`"([^"]+)"\s+\((audio|video)\)`)
	reCu := regexp.MustCompile(`\]\s+"([^"]+)"\s*$`)
	var ds []string
	muc := ""
	for _, dong := range strings.Split(out, "\n") {
		dong = strings.TrimRight(dong, "\r")
		switch {
		case strings.Contains(dong, "DirectShow audio devices"):
			muc = "audio"
		case strings.Contains(dong, "DirectShow video devices"):
			muc = "video"
		case strings.Contains(dong, "Alternative name"):
			// tên thay thế dạng @device_… – bỏ qua
		default:
			if m := reMoi.FindStringSubmatch(dong); m != nil {
				if m[2] == kieu {
					ds = append(ds, m[1])
				}
			} else if m := reCu.FindStringSubmatch(dong); m != nil && muc == kieu {
				ds = append(ds, m[1])
			}
		}
	}
	return ds
}

// tachAVFoundation – các dòng "[AVFoundation indev @ …] [0] Tên" sau tiêu đề "AVFoundation audio devices:".
func tachAVFoundation(out, kieu string) map[string]string {
	re := regexp.MustCompile(`\]\s+\[(\d+)\]\s+(.+)$`)
	ds := map[string]string{}
	muc := ""
	for _, dong := range strings.Split(out, "\n") {
		switch {
		case strings.Contains(dong, "AVFoundation audio devices"):
			muc = "audio"
		case strings.Contains(dong, "AVFoundation video devices"):
			muc = "video"
		default:
			if m := re.FindStringSubmatch(strings.TrimSpace(dong)); m != nil && muc == kieu {
				ds[m[1]] = m[2]
			}
		}
	}
	return ds
}

// ---------------------------------------------------------------- Thu micro bằng ffmpeg

// MicFFmpeg – tiến trình ffmpeg thu micro. Một goroutine đọc stdout từng khung 640 byte và đẩy
// vào kênh có sức chứa 5 khung (100 ms); kênh đầy thì bỏ khung cũ nhất để không tích trễ.
type MicFFmpeg struct {
	cmd   *exec.Cmd
	khung chan []int16
	xong  chan struct{}
	loi   bytes.Buffer // stderr của ffmpeg (lý do lỗi)
	mu    sync.Mutex
}

func MoMicFFmpeg(tb ThietBi) (*MicFFmpeg, error) {
	args := []string{"-hide_banner", "-loglevel", "error", "-nostdin"}
	if tb.DinhDang == "dshow" {
		args = append(args, "-audio_buffer_size", "40") // DirectShow: bộ đệm 40 ms (mặc định 500 ms)
	}
	args = append(args, "-f", tb.DinhDang, "-i", tb.DauVao, "-ac", "1", "-ar", "16000", "-f", "s16le", "pipe:1")
	m := &MicFFmpeg{cmd: lenhNgam("ffmpeg", args...), khung: make(chan []int16, 5), xong: make(chan struct{})}
	m.cmd.Stderr = &khoaGhi{&m.mu, &m.loi}
	out, err := m.cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := m.cmd.Start(); err != nil {
		return nil, err
	}
	go func() {
		defer close(m.xong)
		r := bufio.NewReaderSize(out, 4*ByteMoiKhung)
		b := make([]byte, ByteMoiKhung)
		for {
			if _, err := io.ReadFull(r, b); err != nil {
				m.cmd.Wait()
				return
			}
			mau := make([]int16, MauMoiKhung)
			for i := range mau {
				mau[i] = int16(binary.LittleEndian.Uint16(b[2*i:])) // s16le
			}
			select {
			case m.khung <- mau:
			default: // đầy: bỏ khung cũ nhất rồi đưa khung mới vào
				select {
				case <-m.khung:
				default:
				}
				m.khung <- mau
			}
		}
	}()
	return m, nil
}

// LayKhung – không chờ: có khung thì trả về, chưa có thì nil.
func (m *MicFFmpeg) LayKhung() []int16 {
	select {
	case k := <-m.khung:
		return k
	default:
		return nil
	}
}

func (m *MicFFmpeg) ConChay() bool {
	select {
	case <-m.xong:
		return false
	default:
		return true
	}
}

// Loi – dòng lỗi cuối cùng ffmpeg in ra (nếu có).
func (m *MicFFmpeg) Loi() string {
	m.mu.Lock()
	defer m.mu.Unlock()
	d := strings.Split(strings.TrimSpace(m.loi.String()), "\n")
	return strings.TrimSpace(d[len(d)-1])
}

func (m *MicFFmpeg) Dong() {
	if m.cmd.Process != nil {
		m.cmd.Process.Kill()
	}
}

type khoaGhi struct {
	mu *sync.Mutex
	b  *bytes.Buffer
}

func (k *khoaGhi) Write(p []byte) (int, error) {
	k.mu.Lock()
	defer k.mu.Unlock()
	if k.b.Len() < 8192 {
		k.b.Write(p)
	}
	return len(p), nil
}

// ---------------------------------------------------------------- Nguồn micro có dự phòng

// NguonMic – micro thật (ffmpeg) nếu mở được, nếu không thì sóng sin giả lập. Khi ffmpeg dừng giữa
// chừng (rút micro, lỗi quyền…) nguồn tự chuyển sang giả lập; GiaLap() cho giao diện biết để hiện.
type NguonMic struct {
	mu     sync.Mutex
	that   *MicFFmpeg
	sin    *NguonSin
	moTa   string
	giaLap bool
}

// MoNguonMic – tb == nil: dùng thẳng giả lập (sin tanSo Hz, biên độ 0,25 toàn thang).
func MoNguonMic(tb *ThietBi, tanSo float64) *NguonMic {
	n := &NguonMic{sin: &NguonSin{F: tanSo, A: 0.25 * 32767}}
	n.moTa = "giả lập (sin " + soNgan(tanSo) + " Hz)"
	n.giaLap = true
	if tb == nil {
		return n
	}
	m, err := MoMicFFmpeg(*tb)
	if err != nil {
		n.moTa = "giả lập – không chạy được ffmpeg: " + err.Error()
		return n
	}
	n.that, n.giaLap, n.moTa = m, false, "thật: "+tb.Ten
	return n
}

func (n *NguonMic) LayKhung() []int16 {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.that != nil {
		if n.that.ConChay() {
			return n.that.LayKhung()
		}
		n.moTa = "giả lập – micro thật dừng: " + n.that.Loi()
		n.that, n.giaLap = nil, true
	}
	return n.sin.LayKhung()
}

// GiaLap – true nếu đang phát sóng sin thay cho micro thật.
func (n *NguonMic) GiaLap() bool { n.mu.Lock(); defer n.mu.Unlock(); return n.giaLap }
func (n *NguonMic) MoTa() string { n.mu.Lock(); defer n.mu.Unlock(); return n.moTa }

func (n *NguonMic) Dong() {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.that != nil {
		n.that.Dong()
	}
}

func soNgan(f float64) string { return strconv.FormatFloat(f, 'f', -1, 64) }

// ---------------------------------------------------------------- Phát ra loa bằng ffplay

// Loa – tiến trình ffplay đọc PCM từ stdin. Ghi() không bao giờ chặn: khung được đưa vào kênh
// sức chứa 10 (200 ms), một goroutine riêng ghi xuống ống dẫn; kênh đầy thì bỏ khung.
type Loa struct {
	cmd  *exec.Cmd
	vao  chan []int16
	xong chan struct{}
	loi  bytes.Buffer
	mu   sync.Mutex
}

// MoLoa – thử "-ch_layout mono" (ffplay ≥ 5.1); nếu ffplay thoát ngay thì thử lại "-ac 1" (bản cũ).
// Hàm chờ tối đa ~0,8 s để biết ffplay có chạy được không, nên gọi trong goroutine nền.
func MoLoa() (*Loa, error) {
	if !CoLenh("ffplay") {
		return nil, errors.New("không tìm thấy ffplay (cài FFmpeg)")
	}
	var loiCuoi error
	for _, kenh := range [][]string{{"-ch_layout", "mono"}, {"-ac", "1"}} {
		args := []string{"-hide_banner", "-loglevel", "error", "-nodisp", "-fflags", "nobuffer",
			"-probesize", "32", "-analyzeduration", "0", "-f", "s16le", "-ar", "16000"}
		args = append(append(args, kenh...), "-i", "pipe:0")
		l := &Loa{cmd: lenhNgam("ffplay", args...), vao: make(chan []int16, 10), xong: make(chan struct{})}
		l.cmd.Stderr = &khoaGhi{&l.mu, &l.loi}
		in, err := l.cmd.StdinPipe()
		if err != nil {
			return nil, err
		}
		if err := l.cmd.Start(); err != nil {
			return nil, err
		}
		go func() { l.cmd.Wait(); close(l.xong) }()
		go func() { // ghi PCM little-endian xuống stdin của ffplay
			b := make([]byte, ByteMoiKhung)
			for {
				select {
				case mau := <-l.vao:
					for i, x := range mau {
						binary.LittleEndian.PutUint16(b[2*i:], uint16(x))
					}
					if _, err := in.Write(b[:2*len(mau)]); err != nil {
						return
					}
				case <-l.xong:
					return
				}
			}
		}()
		// đưa trước 0,4 s khoảng lặng để ffplay mở thiết bị, rồi xem nó còn sống không
		for i := 0; i < 20; i++ {
			l.Ghi(make([]int16, MauMoiKhung))
		}
		select {
		case <-l.xong:
			loiCuoi = errors.New("ffplay dừng: " + l.Loi())
			continue
		case <-time.After(800 * time.Millisecond):
			return l, nil
		}
	}
	return nil, loiCuoi
}

func (l *Loa) Ghi(mau []int16) {
	select {
	case l.vao <- mau:
	default: // loa không kịp phát: bỏ khung để không tích trễ
	}
}

func (l *Loa) ConChay() bool {
	select {
	case <-l.xong:
		return false
	default:
		return true
	}
}

func (l *Loa) Loi() string {
	l.mu.Lock()
	defer l.mu.Unlock()
	if strings.Contains(l.loi.String(), "audio open failed") {
		return "không mở được thiết bị phát âm thanh"
	}
	d := strings.Split(strings.TrimSpace(l.loi.String()), "\n")
	return strings.TrimSpace(d[len(d)-1])
}

func (l *Loa) Dong() {
	if l.cmd.Process != nil {
		l.cmd.Process.Kill()
	}
}
