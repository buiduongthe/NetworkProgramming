package tructiep

// Thiết bị của máy phát và người xem: camera thật / mẫu thử testsrc / camera giả lập "bảng clapper";
// micro thật / micro giả lập (440 Hz + bíp mỗi giây); loa thật. Chép từ MR6.3 (camera) và MR6.2
// (micro, loa) của bản Go, sửa: LayKhung(số khung, nhãn ms) và nguồn giả lập là clapper.
//
// Thư viện chuẩn Go không có API camera/âm thanh, nên bài nhờ chương trình ngoài qua os/exec + ống dẫn:
//
//	camera:  ffmpeg -f <dshow|v4l2|avfoundation> -i <camera> -vf scale=160:120,fps=<fps>
//	         -pix_fmt rgb24 -f rawvideo pipe:1          (mỗi khung đúng 160×120×3 = 57 600 byte)
//	testsrc: ffmpeg -re -f lavfi -i testsrc=size=160x120:rate=<fps> …   (ảnh mẫu thử, không phải camera)
//	micro:   ffmpeg -f <dshow|pulse|alsa|avfoundation> -i <thiết bị> -ac 1 -ar 16000 -f s16le pipe:1
//	loa:     ffplay -nodisp -f s16le -ar 16000 -ch_layout mono -i pipe:0   (bản cũ: -ac 1)
//
// Chú ý thứ tự byte: s16le là little-endian, còn RTP L16 là big-endian (thứ tự mạng).
// Một goroutine đọc ống liên tục và chỉ giữ khung MỚI NHẤT (camera) / vài khung gần nhất (micro) –
// trực tiếp không cần dữ liệu cũ. Không có ffmpeg, hoặc ffmpeg dừng/lỗi (không có camera, camera
// bận…) -> lõi máy phát tự chuyển sang nguồn giả lập (có báo).

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	GiaLap = "gia_lap" // "camera"/"micro" không phải thiết bị thật
	MauThu = "testsrc" // ảnh mẫu thử của ffmpeg
)

// Camera – mọi nguồn hình (thật hay giả lập) có cùng giao diện; LayKhung trả nil khi chưa có khung.
type Camera interface {
	LayKhung(so int, tsMs int64) []byte
	DatFps(fps int)
	Loi() string // khác "" khi thiết bị đã dừng/lỗi
	MoTa() string
	That() bool
	Dong()
}

// Micro – nguồn âm 320 mẫu (20 ms) mỗi lượt.
type Micro interface {
	LayKhung() []int16
	MoTa() string
	That() bool
	Dong()
}

// LoaRa – nơi đưa khung âm thanh ra loa (người xem); nil = không phát.
type LoaRa interface {
	Phat(mau []int16)
	Dong()
}

// ThietBi – một thiết bị tìm thấy: Ma là giá trị dùng cho ffmpeg, Ten để hiển thị.
type ThietBi struct {
	Ma, Ten string
}

// CoLenh – chương trình ngoài (ffmpeg, ffplay, pactl…) có trong PATH không.
func CoLenh(ten string) bool {
	_, err := exec.LookPath(ten)
	return err == nil
}

func CoFfmpeg() bool { return CoLenh("ffmpeg") }

func lenhNgam(ten string, args ...string) *exec.Cmd {
	c := exec.Command(ten, args...)
	anCuaSo(c)
	return c
}

// ---------------------------------------------------------------- Liệt kê thiết bị

// LietKeCamera – camera thật tìm được qua ffmpeg (Windows dshow, macOS avfoundation, Linux /dev/video*).
func LietKeCamera() []ThietBi {
	if !CoFfmpeg() {
		return nil
	}
	var ds []ThietBi
	switch runtime.GOOS {
	case "windows":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy").CombinedOutput()
		for _, ten := range tachDShow(string(out), "video") {
			ds = append(ds, ThietBi{ten, ten})
		}
	case "darwin":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", "").CombinedOutput()
		m := tachAVFoundation(string(out), "video")
		for _, so := range khoaSapXep(m) {
			ds = append(ds, ThietBi{so, m[so]})
		}
	default:
		tep, _ := filepath.Glob("/dev/video*")
		sort.Strings(tep)
		for _, t := range tep {
			ds = append(ds, ThietBi{t, t})
		}
	}
	return ds
}

// LietKeMicro – micro thật: Windows dshow; Linux nguồn PulseAudio/PipeWire (pactl) hoặc ALSA mặc
// định; macOS avfoundation. Ma có dạng "<định dạng>|<đầu vào>" cho ffmpeg.
func LietKeMicro() []ThietBi {
	if !CoFfmpeg() {
		return nil
	}
	var ds []ThietBi
	switch runtime.GOOS {
	case "windows":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy").CombinedOutput()
		for _, ten := range tachDShow(string(out), "audio") {
			ds = append(ds, ThietBi{"dshow|audio=" + ten, ten})
		}
	case "darwin":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", "").CombinedOutput()
		m := tachAVFoundation(string(out), "audio")
		for _, so := range khoaSapXep(m) {
			ds = append(ds, ThietBi{"avfoundation|:" + so, m[so]})
		}
	default:
		if CoLenh("pactl") {
			out, _ := lenhNgam("pactl", "list", "short", "sources").Output()
			for _, dong := range strings.Split(string(out), "\n") {
				if f := strings.Fields(dong); len(f) >= 2 && !strings.HasSuffix(f[1], ".monitor") {
					ds = append(ds, ThietBi{"pulse|" + f[1], "Pulse: " + f[1]})
				}
			}
		}
		if len(ds) == 0 {
			if _, err := os.Stat("/dev/snd"); err == nil {
				ds = append(ds, ThietBi{"alsa|default", "ALSA: default"})
			}
		}
	}
	return ds
}

func khoaSapXep(m map[string]string) []string {
	var k []string
	for s := range m {
		k = append(k, s)
	}
	sort.Slice(k, func(i, j int) bool { a, _ := strconv.Atoi(k[i]); b, _ := strconv.Atoi(k[j]); return a < b })
	return k
}

// tachDShow – bản ffmpeg mới in `"Tên" (video)`; bản cũ chia mục "DirectShow video devices".
func tachDShow(out, kieu string) []string {
	reMoi := regexp.MustCompile(`"([^"]+)"\s+\((audio|video|none)\)`)
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

// ---------------------------------------------------------------- Camera

// CameraGiaLap – bọc NguonClapper theo cùng giao diện Camera.
type CameraGiaLap struct{ nguon *NguonClapper }

func MoiCameraGiaLap(ten string, ma, fps int) *CameraGiaLap {
	return &CameraGiaLap{MoiClapper(ten, ma, fps)}
}
func (c *CameraGiaLap) LayKhung(so int, tsMs int64) []byte { return c.nguon.LayKhung(so, tsMs) }
func (c *CameraGiaLap) DatFps(fps int)                     { c.nguon.Fps = fps }
func (c *CameraGiaLap) Loi() string                        { return "" }
func (c *CameraGiaLap) MoTa() string                       { return "GIẢ LẬP – bảng clapper" }
func (c *CameraGiaLap) That() bool                         { return false }
func (c *CameraGiaLap) Dong()                              {}

// lenhFfmpeg – dòng lệnh ffmpeg thu camera ma (hoặc ảnh mẫu thử testsrc) ra khung RGB thô 160×120.
func lenhFfmpeg(ma string, fps int) []string {
	var vao []string
	switch {
	case ma == MauThu:
		vao = []string{"-re", "-f", "lavfi", "-i", "testsrc=size=160x120:rate=" + strconv.Itoa(fps)}
	case runtime.GOOS == "windows":
		vao = []string{"-f", "dshow", "-i", "video=" + ma}
	case runtime.GOOS == "darwin":
		vao = []string{"-f", "avfoundation", "-framerate", "30", "-i", ma}
	default:
		vao = []string{"-f", "v4l2", "-i", ma}
	}
	args := append([]string{"-hide_banner", "-loglevel", "error", "-nostdin"}, vao...)
	return append(args, "-an", "-vf", "scale=160:120,fps="+strconv.Itoa(fps), "-pix_fmt", "rgb24",
		"-f", "rawvideo", "pipe:1")
}

// CameraFfmpeg – camera qua tiến trình ffmpeg; goroutine nền đọc từng khung 57 600 byte, giữ khung mới nhất.
type CameraFfmpeg struct {
	cmd     *exec.Cmd
	mu      sync.Mutex
	moiNhat []byte
	that    bool
	moTa    string
	loi     string
	stderr  bytes.Buffer
}

func MoCameraFfmpeg(ma, ten string, fps int) (*CameraFfmpeg, error) {
	c := &CameraFfmpeg{cmd: lenhNgam("ffmpeg", lenhFfmpeg(ma, fps)...), that: ma != MauThu}
	if c.that {
		c.moTa = "THẬT – " + ten + " (ffmpeg)"
	} else {
		c.moTa = "MẪU THỬ ffmpeg testsrc (không phải camera)"
	}
	c.cmd.Stderr = &ghiCoKhoa{&c.mu, &c.stderr}
	out, err := c.cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := c.cmd.Start(); err != nil {
		return nil, err
	}
	go func() {
		r := bufio.NewReaderSize(out, Rong*Cao*3)
		for {
			b := make([]byte, Rong*Cao*3)
			if _, err := io.ReadFull(r, b); err != nil { // ffmpeg kết thúc / lỗi mở camera
				break
			}
			c.mu.Lock()
			c.moiNhat = b
			c.mu.Unlock()
		}
		c.cmd.Wait()
		c.mu.Lock()
		d := strings.Split(strings.TrimSpace(c.stderr.String()), "\n")
		c.loi = strings.TrimSpace(d[len(d)-1])
		if c.loi == "" {
			c.loi = "ffmpeg đã dừng"
		}
		c.mu.Unlock()
	}()
	return c, nil
}

func (c *CameraFfmpeg) LayKhung(int, int64) []byte {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.moiNhat == nil {
		return nil // chưa có khung nào
	}
	return append([]byte(nil), c.moiNhat...)
}
func (c *CameraFfmpeg) DatFps(int)   {} // ffmpeg chạy theo fps lúc mở
func (c *CameraFfmpeg) Loi() string  { c.mu.Lock(); defer c.mu.Unlock(); return c.loi }
func (c *CameraFfmpeg) MoTa() string { return c.moTa }
func (c *CameraFfmpeg) That() bool   { return c.that }
func (c *CameraFfmpeg) Dong() {
	if c.cmd.Process != nil {
		c.cmd.Process.Kill()
	}
}

// MoCamera – mở camera đã chọn; gia_lap hoặc không có ffmpeg (hoặc không chạy được) -> camera giả lập.
func MoCamera(ma, tenHienThi, tenMayPhat string, maMayPhat, fps int) Camera {
	if ma == GiaLap || !CoFfmpeg() {
		return MoiCameraGiaLap(tenMayPhat, maMayPhat, fps)
	}
	c, err := MoCameraFfmpeg(ma, tenHienThi, fps)
	if err != nil {
		return MoiCameraGiaLap(tenMayPhat, maMayPhat, fps)
	}
	return c
}

type ghiCoKhoa struct {
	mu *sync.Mutex
	b  *bytes.Buffer
}

func (g *ghiCoKhoa) Write(p []byte) (int, error) {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.b.Len() < 8192 {
		g.b.Write(p)
	}
	return len(p), nil
}

// ---------------------------------------------------------------- Micro

// MicGiaLap – micro giả lập: 440 Hz biên độ thấp + bíp 1000 Hz 100 ms ở mỗi giây tròn.
type MicGiaLap struct{ *NguonAmClapper }

func MoiMicGiaLap() *MicGiaLap    { return &MicGiaLap{MoiAmClapper()} }
func (m *MicGiaLap) MoTa() string { return "GIẢ LẬP – 440 Hz + bíp 1000 Hz mỗi giây" }
func (m *MicGiaLap) That() bool   { return false }
func (m *MicGiaLap) Dong()        {}

// MicThat – tiến trình ffmpeg thu micro; goroutine đọc stdout từng khung 640 byte (s16le) vào kênh
// sức chứa 10 khung (200 ms); kênh đầy thì bỏ khung mới để không tích trễ.
type MicThat struct {
	cmd   *exec.Cmd
	khung chan []int16
	moTa  string
	mu    sync.Mutex
	loi   bytes.Buffer
}

func MoMicThat(tb ThietBi) (*MicThat, error) {
	dinhDang, dauVao, ok := strings.Cut(tb.Ma, "|")
	if !ok {
		return nil, errors.New("thiết bị không hợp lệ")
	}
	args := []string{"-hide_banner", "-loglevel", "error", "-nostdin"}
	if dinhDang == "dshow" {
		args = append(args, "-audio_buffer_size", "40") // DirectShow: bộ đệm 40 ms (mặc định 500 ms)
	}
	args = append(args, "-f", dinhDang, "-i", dauVao, "-ac", "1", "-ar", "16000", "-f", "s16le", "pipe:1")
	m := &MicThat{cmd: lenhNgam("ffmpeg", args...), khung: make(chan []int16, 10), moTa: "THẬT – " + tb.Ten}
	m.cmd.Stderr = &ghiCoKhoa{&m.mu, &m.loi}
	out, err := m.cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := m.cmd.Start(); err != nil {
		return nil, err
	}
	xong := make(chan struct{})
	go func() {
		defer close(xong)
		r := bufio.NewReaderSize(out, 4*ByteMoiKhung)
		b := make([]byte, ByteMoiKhung)
		for {
			if _, err := io.ReadFull(r, b); err != nil {
				m.cmd.Wait()
				return
			}
			mau := make([]int16, MauMoiKhung)
			for i := range mau {
				mau[i] = int16(binary.LittleEndian.Uint16(b[2*i:]))
			}
			select {
			case m.khung <- mau:
			default:
			}
		}
	}()
	select { // ffmpeg không mở được thiết bị thì thoát gần như ngay
	case <-xong:
		return nil, errors.New(m.Loi())
	case <-time.After(300 * time.Millisecond):
	}
	return m, nil
}

// LayKhung – không chờ: chưa có dữ liệu thì coi như im lặng.
func (m *MicThat) LayKhung() []int16 {
	select {
	case k := <-m.khung:
		return k
	default:
		return make([]int16, MauMoiKhung)
	}
}
func (m *MicThat) MoTa() string { return m.moTa }
func (m *MicThat) That() bool   { return true }
func (m *MicThat) Loi() string {
	m.mu.Lock()
	defer m.mu.Unlock()
	d := strings.Split(strings.TrimSpace(m.loi.String()), "\n")
	if s := strings.TrimSpace(d[len(d)-1]); s != "" {
		return s
	}
	return "ffmpeg đã dừng"
}
func (m *MicThat) Dong() {
	if m.cmd.Process != nil {
		m.cmd.Process.Kill()
	}
}

// MoMic – tb == nil -> giả lập; lỗi mở thiết bị thật -> giả lập và thông báo lỗi.
func MoMic(tb *ThietBi) (Micro, string) {
	if tb == nil {
		return MoiMicGiaLap(), ""
	}
	m, err := MoMicThat(*tb)
	if err != nil {
		return MoiMicGiaLap(), "không mở được micro '" + tb.Ten + "' (" + err.Error() + "); chuyển sang giả lập"
	}
	return m, ""
}

// ---------------------------------------------------------------- Loa

// Loa – tiến trình ffplay đọc PCM từ stdin. Phat() không bao giờ chặn: khung vào kênh sức chứa 10
// (200 ms), một goroutine ghi xuống ống dẫn; kênh đầy thì bỏ khung.
type Loa struct {
	cmd  *exec.Cmd
	vao  chan []int16
	xong chan struct{}
	loi  bytes.Buffer
	mu   sync.Mutex
}

// MoLoa – thử "-ch_layout mono" (ffplay ≥ 5.1) rồi "-ac 1" (bản cũ). Chờ tối đa ~0,8 s để biết ffplay
// có chạy được không, nên gọi trong goroutine nền. Trả về (loa hoặc nil, lý do nếu nil).
func MoLoa() (*Loa, string) {
	if !CoLenh("ffplay") {
		return nil, "không có ffplay – chỉ đo mức"
	}
	lyDo := ""
	for _, kenh := range [][]string{{"-ch_layout", "mono"}, {"-ac", "1"}} {
		args := []string{"-hide_banner", "-loglevel", "error", "-nodisp", "-fflags", "nobuffer",
			"-probesize", "32", "-analyzeduration", "0", "-f", "s16le", "-ar", "16000"}
		args = append(append(args, kenh...), "-i", "pipe:0")
		l := &Loa{cmd: lenhNgam("ffplay", args...), vao: make(chan []int16, 10), xong: make(chan struct{})}
		l.cmd.Stderr = &ghiCoKhoa{&l.mu, &l.loi}
		in, err := l.cmd.StdinPipe()
		if err != nil {
			return nil, "không mở được loa (" + err.Error() + ") – chỉ đo mức"
		}
		if err := l.cmd.Start(); err != nil {
			return nil, "không mở được loa (" + err.Error() + ") – chỉ đo mức"
		}
		go func() { l.cmd.Wait(); close(l.xong) }()
		go func() {
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
		for i := 0; i < 20; i++ { // 0,4 s khoảng lặng để ffplay mở thiết bị
			l.Phat(make([]int16, MauMoiKhung))
		}
		select {
		case <-l.xong:
			lyDo = "không có thiết bị phát – chỉ đo mức"
			l.mu.Lock()
			khongThietBi := strings.Contains(l.loi.String(), "audio open failed")
			l.mu.Unlock()
			if s := l.Loi(); s != "" && !khongThietBi {
				lyDo = "không mở được loa (" + s + ") – chỉ đo mức"
			}
			continue
		case <-time.After(800 * time.Millisecond):
			return l, ""
		}
	}
	return nil, lyDo
}

func (l *Loa) Phat(mau []int16) {
	select {
	case l.vao <- mau:
	default: // loa không kịp phát: bỏ khung để không tích trễ
	}
}

func (l *Loa) Loi() string {
	l.mu.Lock()
	defer l.mu.Unlock()
	d := strings.Split(strings.TrimSpace(l.loi.String()), "\n")
	return strings.TrimSpace(d[len(d)-1])
}

func (l *Loa) Dong() {
	if l.cmd.Process != nil {
		l.cmd.Process.Kill()
	}
}

// ConChay – ffplay còn chạy không.
func (l *Loa) ConChay() bool {
	select {
	case <-l.xong:
		return false
	default:
		return true
	}
}
