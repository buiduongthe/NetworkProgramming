package tructiep

// Nguồn "tệp video" cho máy phát: phát lại hình + tiếng của một tệp (mp4, mkv, avi, webm…) như một
// buổi phát trực tiếp (giống OBS "Media Source" hay `ffmpeg -re … rtmp://…`).
//
// Hai tiến trình ffmpeg cùng đọc tệp với -re (đọc theo tốc độ thật, không nhanh hơn thời gian phát)
// và -stream_loop -1 (hết tệp thì phát lại từ đầu):
//
//	hình:  ffmpeg -re -stream_loop -1 -i <tệp> -an -vf scale=160:120:force_original_aspect_ratio=decrease,
//	              pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
//	tiếng: ffmpeg -re -stream_loop -1 -i <tệp> -vn -ac 1 -ar 16000 -f s16le pipe:1
//
// Bộ lọc fps=<fps> cho đúng <fps> khung mỗi giây THỜI GIAN CỦA TỆP, nên khung thứ i có thời điểm trong
// tệp i × 1000 / fps ms; tiếng có 16 000 mẫu mỗi giây của tệp.
//
// Đồng bộ tiếng-hình ở phía phát. Hai tiến trình khởi động lệch nhau vài chục – vài trăm ms; nếu cứ gửi
// "khung mới nhất" thì hình và tiếng của TỆP lệch nhau ngay từ máy phát, người xem đồng bộ theo nhãn
// thời gian cũng không sửa được. Cách làm ở đây (giống bộ ghép kênh – muxer – của bộ mã hóa thật):
//   - Tiếng là đồng hồ chủ: vòng 20 ms của máy phát lấy lần lượt 320 mẫu của tệp. Chưa đủ mẫu (ffmpeg
//     chưa chạy kịp) -> gửi im lặng và lechMs += 20; dồn quá 300 ms -> bỏ bớt 20 ms và lechMs −= 20.
//     Vậy ở thời điểm phương tiện T máy phát đang gửi tiếng của giây T − lechMs trong tệp.
//   - Hình: goroutine đọc giữ ~3 s khung gần nhất kèm thời điểm trong tệp; khi máy phát chụp khung ở
//     thời điểm phương tiện T, chọn khung có thời điểm trong tệp ≤ T − lechMs gần nhất. Nhờ vậy nhãn
//     thời gian gửi đi khớp với nội dung của tệp, và cơ chế đồng bộ ở người xem hoạt động như với camera.
//
// Tệp không có tiếng (hoặc chọn micro khác) -> hình lấy khung mới nhất.

import (
	"bufio"
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
)

const (
	Tep           = "tep" // mã nguồn "tệp video" trong danh sách camera/micro
	donToiDaMs    = 300   // tiếng của tệp dồn quá mức này thì bỏ bớt
	giuHinhMs     = 3000  // giữ khung hình của 3 s gần nhất
	byteKhungAmLE = MauMoiKhung * 2
)

func lenhHinhTep(duongDan string, fps int) []string {
	loc := "scale=160:120:force_original_aspect_ratio=decrease,pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=" +
		strconv.Itoa(fps)
	return []string{"-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan,
		"-an", "-vf", loc, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"}
}

func lenhTiengTep(duongDan string) []string {
	return []string{"-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan,
		"-vn", "-ac", "1", "-ar", strconv.Itoa(TanSoMau), "-f", "s16le", "pipe:1"}
}

type khungTep struct {
	ms    int
	khung []byte
}

// TepVideo – một tệp đang phát: hai tiến trình ffmpeg + bộ đệm tiếng + vòng khung hình có thời điểm.
type TepVideo struct {
	DuongDan, Ten     string
	fps               int
	mu                sync.Mutex
	hinh              []khungTep
	tieng             []byte
	soKhungDoc        int
	LechMs            int // thời điểm phương tiện − thời điểm trong tệp (tiếng)
	dungTieng         bool
	loiHinh, loiTieng string
	hetTieng          bool
	ffHinh, ffTieng   *exec.Cmd
	errHinh, errTieng bytes.Buffer
}

func dongCuoi(b *bytes.Buffer, macDinh string) string {
	s := strings.TrimSpace(b.String())
	if s == "" {
		return macDinh
	}
	d := strings.Split(s, "\n")
	return strings.TrimSpace(d[len(d)-1])
}

func MoTepVideo(duongDan string, fps int, coTieng bool) (*TepVideo, error) {
	if !CoFfmpeg() {
		return nil, errors.New("không có ffmpeg trong PATH")
	}
	if st, err := os.Stat(duongDan); err != nil || st.IsDir() {
		return nil, errors.New("không thấy tệp " + duongDan)
	}
	t := &TepVideo{DuongDan: duongDan, Ten: filepath.Base(duongDan), fps: max(1, fps), hetTieng: !coTieng}
	t.ffHinh = lenhNgam("ffmpeg", lenhHinhTep(duongDan, t.fps)...)
	t.ffHinh.Stderr = &ghiCoKhoa{&t.mu, &t.errHinh}
	out, err := t.ffHinh.StdoutPipe()
	if err != nil {
		return nil, err
	}
	if err := t.ffHinh.Start(); err != nil {
		return nil, err
	}
	go t.docHinh(out)
	if coTieng {
		t.ffTieng = lenhNgam("ffmpeg", lenhTiengTep(duongDan)...)
		t.ffTieng.Stderr = &ghiCoKhoa{&t.mu, &t.errTieng}
		outT, err := t.ffTieng.StdoutPipe()
		if err == nil {
			err = t.ffTieng.Start()
		}
		if err != nil {
			t.Dong()
			return nil, err
		}
		go t.docTieng(outT)
	}
	return t, nil
}

func (t *TepVideo) docHinh(out io.Reader) {
	r := bufio.NewReaderSize(out, Rong*Cao*3)
	for {
		khung := make([]byte, Rong*Cao*3)
		if _, err := io.ReadFull(r, khung); err != nil {
			break
		}
		t.mu.Lock()
		ms := t.soKhungDoc * 1000 / t.fps
		t.soKhungDoc++
		t.hinh = append(t.hinh, khungTep{ms, khung})
		for len(t.hinh) > 0 && t.hinh[0].ms < ms-giuHinhMs {
			t.hinh = t.hinh[1:]
		}
		t.mu.Unlock()
	}
	t.ffHinh.Wait()
	t.mu.Lock()
	t.loiHinh = dongCuoi(&t.errHinh, "ffmpeg (hình) đã dừng")
	t.mu.Unlock()
}

func (t *TepVideo) docTieng(out io.Reader) {
	b := make([]byte, byteKhungAmLE)
	for {
		n, err := out.Read(b)
		if n > 0 {
			t.mu.Lock()
			t.tieng = append(t.tieng, b[:n]...)
			t.mu.Unlock()
		}
		if err != nil {
			break
		}
	}
	t.ffTieng.Wait()
	t.mu.Lock()
	t.hetTieng = true
	t.loiTieng = dongCuoi(&t.errTieng, "tệp không có tiếng")
	t.mu.Unlock()
}

// LayTieng – 320 mẫu tiếp theo của tệp; chưa đủ -> im lặng (LechMs += 20).
func (t *TepVideo) LayTieng() []int16 {
	t.mu.Lock()
	defer t.mu.Unlock()
	don := byteKhungAmLE * donToiDaMs / KhungMs
	for len(t.tieng) > don { // dồn quá nhiều: bỏ 20 ms cũ nhất
		t.tieng = t.tieng[byteKhungAmLE:]
		t.LechMs -= KhungMs
	}
	mau := make([]int16, MauMoiKhung)
	if len(t.tieng) < byteKhungAmLE {
		t.LechMs += KhungMs
		return mau
	}
	for i := range mau { // s16le: byte thấp trước
		mau[i] = int16(binary.LittleEndian.Uint16(t.tieng[2*i:]))
	}
	t.tieng = append([]byte(nil), t.tieng[byteKhungAmLE:]...)
	return mau
}

// LayHinh – khung ứng với thời điểm phương tiện tsMs (nil khi chưa có khung nào).
func (t *TepVideo) LayHinh(tsMs int64) []byte {
	t.mu.Lock()
	defer t.mu.Unlock()
	if len(t.hinh) == 0 {
		return nil
	}
	if !t.dungTieng || t.hetTieng {
		return append([]byte(nil), t.hinh[len(t.hinh)-1].khung...) // không có tiếng của tệp: khung mới nhất
	}
	can := tsMs - int64(t.LechMs) // thời điểm trong tệp ứng với tiếng đang gửi
	var chon []byte
	for _, k := range t.hinh {
		if int64(k.ms) <= can {
			chon = k.khung
		} else {
			break
		}
	}
	if chon == nil {
		chon = t.hinh[0].khung // hình chưa tới kịp: dùng khung sớm nhất đang có
	}
	return append([]byte(nil), chon...)
}

func (t *TepVideo) Dong() {
	for _, p := range []*exec.Cmd{t.ffHinh, t.ffTieng} {
		if p != nil && p.Process != nil {
			p.Process.Kill()
		}
	}
}

// CameraTep – phần hình của TepVideo theo giao diện Camera.
type CameraTep struct{ Tep *TepVideo }

func (c *CameraTep) LayKhung(_ int, tsMs int64) []byte { return c.Tep.LayHinh(tsMs) }
func (c *CameraTep) DatFps(int)                        {} // ffmpeg chạy theo fps lúc mở
func (c *CameraTep) Loi() string                       { c.Tep.mu.Lock(); defer c.Tep.mu.Unlock(); return c.Tep.loiHinh }
func (c *CameraTep) MoTa() string                      { return "TỆP VIDEO – " + c.Tep.Ten }
func (c *CameraTep) That() bool                        { return false }
func (c *CameraTep) Dong()                             { c.Tep.Dong() }

// MicTep – phần tiếng của TepVideo theo giao diện Micro.
type MicTep struct{ Tep *TepVideo }

func (m *MicTep) LayKhung() []int16 { return m.Tep.LayTieng() }
func (m *MicTep) MoTa() string      { return "TIẾNG CỦA TỆP – " + m.Tep.Ten }
func (m *MicTep) That() bool        { return false }
func (m *MicTep) Dong()             { m.Tep.Dong() }

// MoTep – mở tệp video; trả về camera và micro (nil nếu layTieng = false).
func MoTep(duongDan string, fps int, layTieng bool) (*CameraTep, *MicTep, error) {
	t, err := MoTepVideo(duongDan, fps, layTieng)
	if err != nil {
		return nil, nil, err
	}
	if !layTieng {
		return &CameraTep{t}, nil, nil
	}
	t.dungTieng = true
	return &CameraTep{t}, &MicTep{t}, nil
}

// TaoTepMau – tạo tệp mp4 mẫu bằng ffmpeg (để thử khi chưa có tệp): ảnh testsrc2 chớp trắng 0,1 s đầu
// mỗi giây, tiếng 440 Hz nhỏ + bíp 1000 Hz đúng lúc chớp – nhìn/nghe là thấy hình và tiếng có khớp.
func TaoTepMau(duongDan string, giay int) error {
	g := strconv.Itoa(giay)
	hinh := "testsrc2=size=320x240:rate=25:duration=" + g +
		`,drawbox=x=0:y=0:w=iw:h=ih:color=white@0.85:t=fill:enable='lt(mod(t\,1)\,0.1)'`
	tieng := `aevalsrc='0.08*sin(2*PI*440*t)+0.4*sin(2*PI*1000*t)*lt(mod(t\,1)\,0.1)':s=` +
		strconv.Itoa(TanSoMau) + ":d=" + g
	args := []string{"-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", hinh,
		"-f", "lavfi", "-i", tieng, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "25",
		"-c:a", "aac", "-shortest", duongDan}
	chay := func() (string, error) {
		var loi bytes.Buffer
		c := lenhNgam("ffmpeg", args...)
		c.Stderr = &loi
		err := c.Run()
		return dongCuoi(&loi, "ffmpeg lỗi"), err
	}
	loi, err := chay()
	if err != nil { // ffmpeg thiếu libx264: dùng mpeg4
		for i, a := range args {
			if a == "libx264" {
				args[i] = "mpeg4"
			}
		}
		loi, err = chay()
	}
	if err != nil {
		return errors.New(loi)
	}
	return nil
}
