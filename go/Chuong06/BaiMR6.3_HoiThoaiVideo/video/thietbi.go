package video

// Camera thật qua tiến trình ffmpeg (chỉ thư viện chuẩn: os/exec + ống dẫn), dùng cho ứng dụng giao diện.
//
//	Windows: ffmpeg -f dshow -i video="Tên camera" …     (liệt kê: -list_devices true -f dshow -i dummy)
//	Linux:   ffmpeg -f v4l2 -i /dev/video0 …              (liệt kê: các tệp /dev/video*)
//	macOS:   ffmpeg -f avfoundation -framerate 30 -i "0" …  (liệt kê: -f avfoundation -list_devices true -i "")
//	chung:   -vf scale=160:120,fps=15 -pix_fmt rgb24 -f rawvideo pipe:1
//
// stdout của ffmpeg là chuỗi khung thô liền nhau, mỗi khung đúng 160 × 120 × 3 = 57 600 byte, nên chỉ
// cần đọc đủ từng khối 57 600 byte. Khung đó đi vào đúng đường gửi sẵn có (thu nhỏ, nén zlib, chia
// mảnh). Không có ffmpeg/camera -> "Camera giả lập" (khung tổng hợp CameraTongHop).

import (
	"bufio"
	"bytes"
	"io"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"sort"
	"strings"
	"sync"
)

// NguonHinh – nơi lấy khung RGB 160×120 thứ "so"; nil khi chưa có khung (camera thật đang khởi động).
type NguonHinh interface{ LayKhung(so int) []byte }

// ThietBi – một camera: DinhDang là giá trị "-f", DauVao là giá trị "-i" của ffmpeg.
type ThietBi struct {
	Ten, DinhDang, DauVao string
}

func CoLenh(ten string) bool {
	_, err := exec.LookPath(ten)
	return err == nil
}

func lenhNgam(ten string, args ...string) *exec.Cmd {
	c := exec.Command(ten, args...)
	anCuaSo(c)
	return c
}

// LietKeCamera – danh sách camera thật (rỗng nếu không có ffmpeg).
func LietKeCamera() []ThietBi {
	if !CoLenh("ffmpeg") {
		return nil
	}
	var ds []ThietBi
	switch runtime.GOOS {
	case "windows":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy").CombinedOutput()
		for _, ten := range tachDShow(string(out), "video") {
			ds = append(ds, ThietBi{ten, "dshow", "video=" + ten})
		}
	case "darwin":
		out, _ := lenhNgam("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", "").CombinedOutput()
		for so, ten := range tachAVFoundation(string(out), "video") {
			ds = append(ds, ThietBi{ten, "avfoundation", so})
		}
	default:
		tep, _ := filepath.Glob("/dev/video*")
		sort.Strings(tep)
		for _, t := range tep {
			ds = append(ds, ThietBi{"V4L2: " + t, "v4l2", t})
		}
	}
	return ds
}

// tachDShow – bản ffmpeg mới in `"Tên" (video)`; bản cũ chia mục "DirectShow video devices".
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

// ---------------------------------------------------------------- Thu camera bằng ffmpeg

// CameraFFmpeg – một goroutine đọc khung thô từ stdout và giữ lại khung MỚI NHẤT (video trực tiếp
// không cần khung cũ); vòng gửi lấy khung mới nhất theo nhịp fps được phép.
type CameraFFmpeg struct {
	cmd     *exec.Cmd
	mu      sync.Mutex
	moiNhat []byte
	xong    chan struct{}
	loi     bytes.Buffer
}

func MoCameraFFmpeg(tb ThietBi) (*CameraFFmpeg, error) {
	args := []string{"-hide_banner", "-loglevel", "error", "-nostdin", "-f", tb.DinhDang}
	if tb.DinhDang == "avfoundation" {
		args = append(args, "-framerate", "30")
	}
	args = append(args, "-i", tb.DauVao, "-an", "-vf", "scale=160:120,fps=15", "-pix_fmt", "rgb24",
		"-f", "rawvideo", "pipe:1")
	c := &CameraFFmpeg{cmd: lenhNgam("ffmpeg", args...), xong: make(chan struct{})}
	c.cmd.Stderr = &ghiCoKhoa{&c.mu, &c.loi}
	out, err := c.cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := c.cmd.Start(); err != nil {
		return nil, err
	}
	go func() {
		defer close(c.xong)
		r := bufio.NewReaderSize(out, Rong*Cao*3)
		for {
			b := make([]byte, Rong*Cao*3)
			if _, err := io.ReadFull(r, b); err != nil {
				c.cmd.Wait()
				return
			}
			c.mu.Lock()
			c.moiNhat = b
			c.mu.Unlock()
		}
	}()
	return c, nil
}

func (c *CameraFFmpeg) LayKhung(int) []byte {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.moiNhat
}

func (c *CameraFFmpeg) ConChay() bool {
	select {
	case <-c.xong:
		return false
	default:
		return true
	}
}

func (c *CameraFFmpeg) Loi() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	d := strings.Split(strings.TrimSpace(c.loi.String()), "\n")
	return strings.TrimSpace(d[len(d)-1])
}

func (c *CameraFFmpeg) Dong() {
	if c.cmd.Process != nil {
		c.cmd.Process.Kill()
	}
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

// ---------------------------------------------------------------- Nguồn camera có dự phòng

// NguonCamera – camera thật nếu mở được, nếu không (hoặc ffmpeg dừng giữa chừng) thì khung tổng hợp.
type NguonCamera struct {
	mu     sync.Mutex
	that   *CameraFFmpeg
	giaLap *CameraTongHop
	moTa   string
	laGia  bool
}

// MoNguonCamera – tb == nil: dùng thẳng camera giả lập (vẽ tên và số khung).
func MoNguonCamera(tb *ThietBi, ten string, ma int) *NguonCamera {
	n := &NguonCamera{giaLap: MoiCamera(ten, max(1, ma)), moTa: "Camera giả lập", laGia: true}
	if tb == nil {
		return n
	}
	c, err := MoCameraFFmpeg(*tb)
	if err != nil {
		n.moTa = "giả lập – không chạy được ffmpeg: " + err.Error()
		return n
	}
	n.that, n.laGia, n.moTa = c, false, "thật: "+tb.Ten
	return n
}

func (n *NguonCamera) LayKhung(so int) []byte {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.that != nil {
		if n.that.ConChay() {
			return n.that.LayKhung(so)
		}
		n.moTa = "giả lập – camera thật dừng: " + n.that.Loi()
		n.that, n.laGia = nil, true
	}
	return n.giaLap.LayKhung(so)
}

func (n *NguonCamera) GiaLap() bool { n.mu.Lock(); defer n.mu.Unlock(); return n.laGia }
func (n *NguonCamera) MoTa() string { n.mu.Lock(); defer n.mu.Unlock(); return n.moTa }

func (n *NguonCamera) Dong() {
	n.mu.Lock()
	defer n.mu.Unlock()
	if n.that != nil {
		n.that.Dong()
	}
}
