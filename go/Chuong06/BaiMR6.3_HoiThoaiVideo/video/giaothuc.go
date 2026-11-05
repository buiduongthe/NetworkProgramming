// Package video – các thủ tục dùng chung cho server và client hội thoại video (bản dòng lệnh ở
// thư mục cha và ứng dụng giao diện ở thư mục gui/ cùng dùng gói này).
package video

// Các thủ tục dùng chung cho server và client hội thoại video.
//
// Khung hình là ma trận điểm ảnh; mỗi điểm RGB 3 byte. Khung 160×120:
//   - kích thước thô: 160 × 120 × 3 = 57 600 byte; ở 10 khung/giây (fps) cần
//     57 600 × 8 × 10 ≈ 4,6 Mbit/s – quá lớn, nên phải nén trước khi gửi;
//   - nén bằng zlib (không mất dữ liệu, thư viện chuẩn); khung tổng hợp có "hạt nhiễu" giống cảm
//     biến camera nên nén còn khoảng 7 KB -> 10 fps × 7 KB × 8 ≈ 560 kbit/s;
//   - giảm độ phân giải 1/2 mỗi chiều -> số điểm ảnh còn 1/4, dung lượng giảm tương ứng.
//
// Khung nén lớn hơn một gói UDP an toàn, nên được chia mảnh: mỗi datagram ≤ 1200 byte (mức QUIC –
// RFC 9000 – coi là an toàn), gồm header 20 byte + tối đa 1180 byte dữ liệu. Bên nhận ghép đủ mảnh
// mới giải nén được; mất một mảnh là hỏng cả khung.
//
// Định dạng gói, JSON điều khiển giống hệt bản Python (dùng zlib cho cả hai), nên hai bản nối
// được với nhau. Thư viện chuẩn Go có image/jpeg, nhưng bài giữ zlib để hai bản tương thích.

import (
	"bytes"
	"compress/zlib"
	"encoding/binary"
	"encoding/json"
	"image"
	"image/png"
	"io"
	"math"
	"math/rand"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

const (
	Rong, Cao   = 160, 120 // độ phân giải gốc của camera
	MaxDatagram = 1200
	CoHeader    = 20
	DuLieuToiDa = MaxDatagram - CoHeader // 1180 byte
	PhienBan    = 1
	LoaiManh    = 1 // mảnh khung hình
	LoaiGiu     = 2 // gói đăng ký/giữ kết nối
	MaServer    = 0
	FpsMacDinh  = 10
	ManhToiDa   = 256 // tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
)

var t0 = time.Now()

// ---------------------------------------------------------------- Chia mảnh và ghép khung

// Header: phiên bản(1) loại(1) mã nguồn(2) số khung(4) chỉ số mảnh(2) tổng mảnh(2) rộng(2) cao(2) ts ms(4)
type Header struct {
	Loai              byte
	Ma                uint16
	SoKhung           uint32
	ChiSo, Tong, W, H uint16
}

func (h Header) DongGoi(payload []byte) []byte {
	b := make([]byte, CoHeader+len(payload))
	b[0], b[1] = PhienBan, h.Loai
	be := binary.BigEndian
	be.PutUint16(b[2:], h.Ma)
	be.PutUint32(b[4:], h.SoKhung)
	be.PutUint16(b[8:], h.ChiSo)
	be.PutUint16(b[10:], h.Tong)
	be.PutUint16(b[12:], h.W)
	be.PutUint16(b[14:], h.H)
	be.PutUint32(b[16:], uint32(time.Since(t0).Milliseconds()))
	copy(b[CoHeader:], payload)
	return b
}

// ChiaManh – chia dữ liệu khung nén thành các datagram ≤ 1200 byte.
func ChiaManh(ma uint16, soKhung uint32, w, h int, duLieu []byte) [][]byte {
	tong := max(1, (len(duLieu)+DuLieuToiDa-1)/DuLieuToiDa)
	var ds [][]byte
	for i := 0; i < tong; i++ {
		phan := duLieu[i*DuLieuToiDa : min(len(duLieu), (i+1)*DuLieuToiDa)]
		ds = append(ds, Header{LoaiManh, ma, soKhung, uint16(i), uint16(tong), uint16(w), uint16(h)}.DongGoi(phan))
	}
	return ds
}

func GoiGiu(ma uint16) []byte { return Header{Loai: LoaiGiu, Ma: ma}.DongGoi(nil) }

func TachHeader(b []byte) (Header, []byte, bool) {
	if len(b) < CoHeader || b[0] != PhienBan {
		return Header{}, nil, false
	}
	be := binary.BigEndian
	h := Header{b[1], be.Uint16(b[2:]), be.Uint32(b[4:]), be.Uint16(b[8:]), be.Uint16(b[10:]),
		be.Uint16(b[12:]), be.Uint16(b[14:])}
	if h.Loai == LoaiManh && (h.Tong == 0 || h.Tong > ManhToiDa || h.ChiSo >= h.Tong) {
		return Header{}, nil, false
	}
	return h, b[CoHeader:], true
}

type khungDo struct {
	tong int
	manh map[int][]byte
	t    time.Time
}

// GhepKhung – ghép các mảnh thành khung cho một luồng. Khi một khung đủ mảnh: trả về khung đó và
// bỏ mọi khung cũ hơn còn dở (video trực tiếp chỉ cần khung mới nhất); khung dở quá 0,5 s cũng bị bỏ.
type GhepKhung struct {
	dang                         map[uint32]*khungDo
	cuoi                         int64 // số khung đã xong/bỏ gần nhất (-1: chưa có)
	ManhNhan, KhungDu, KhungHong int
	ManhMat, KhungMatHan, Tre    int
}

func MoiGhep() *GhepKhung { return &GhepKhung{dang: map[uint32]*khungDo{}, cuoi: -1} }

func (g *GhepKhung) bo(so uint32) {
	k := g.dang[so]
	delete(g.dang, so)
	g.KhungHong++
	g.ManhMat += k.tong - len(k.manh)
}

// them – thêm một mảnh; trả về dữ liệu khung nén khi đủ mảnh, ngược lại nil.
func (g *GhepKhung) Them(h Header, payload []byte, bayGio time.Time) []byte {
	g.ManhNhan++
	if int64(h.SoKhung) <= g.cuoi {
		g.Tre++ // thuộc khung đã xong hoặc đã bỏ
		return nil
	}
	k := g.dang[h.SoKhung]
	if k == nil {
		k = &khungDo{tong: int(h.Tong), manh: map[int][]byte{}, t: bayGio}
		g.dang[h.SoKhung] = k
	} else if int(h.Tong) != k.tong {
		return nil // mâu thuẫn với các mảnh trước của khung: bỏ mảnh
	}
	k.manh[int(h.ChiSo)] = append([]byte(nil), payload...)
	for s, d := range g.dang {
		if s != h.SoKhung && bayGio.Sub(d.t) > 500*time.Millisecond {
			g.bo(s) // chờ quá lâu
		}
	}
	if len(k.manh) < k.tong {
		return nil
	}
	var duLieu []byte
	for j := 0; j < k.tong; j++ {
		duLieu = append(duLieu, k.manh[j]...)
	}
	delete(g.dang, h.SoKhung)
	cu := 0
	for s := range g.dang {
		if s < h.SoKhung {
			g.bo(s) // khung cũ còn dở: bỏ
			cu++
		}
	}
	if g.cuoi >= 0 {
		g.KhungMatHan += max(0, int(int64(h.SoKhung)-g.cuoi-1)-cu)
	}
	g.cuoi = int64(h.SoKhung)
	g.KhungDu++
	return duLieu
}

// ---------------------------------------------------------------- Ảnh RGB thô ([]byte)

// phong – phông chữ bitmap 5×7: mỗi ký tự 7 hàng, mỗi hàng 5 bit (bit cao = điểm bên trái), viết hex.
var phong = map[rune]string{
	'A': "0E11111F111111", 'B': "1E11111E11111E", 'C': "0E11101010110E", 'D': "1E11111111111E",
	'E': "1F10101E10101F", 'F': "1F10101E101010", 'G': "0E11101711110F", 'H': "1111111F111111",
	'I': "0E04040404040E", 'J': "0702020202120C", 'K': "11121418141211", 'L': "1010101010101F",
	'M': "111B1515111111", 'N': "11111915131111", 'O': "0E11111111110E", 'P': "1E11111E101010",
	'Q': "0E11111115120D", 'R': "1E11111E141211", 'S': "0F10100E01011E", 'T': "1F040404040404",
	'U': "1111111111110E", 'V': "11111111110A04", 'W': "1111111515150A", 'X': "11110A040A1111",
	'Y': "1111110A040404", 'Z': "1F01020408101F", '0': "0E11131519110E", '1': "040C040404040E",
	'2': "0E11010204081F", '3': "1F02040201110E", '4': "02060A121F0202", '5': "1F101E0101110E",
	'6': "0608101E11110E", '7': "1F010204080808", '8': "0E11110E11110E", '9': "0E11110F01020C",
	'#': "0A0A1F0A1F0A0A", '/': "01010204081010", ':': "000C0C000C0C00", ';': "000C0C000C0408",
	'.': "00000000000C0C", '-': "0000001F000000", '(': "02040808080402", ')': "08040202020408",
	'=': "00001F001F0000", ',': "000000000C0408", '?': "0E110102040004", ' ': "00000000000000",
}

type Mau [3]byte

// ToMau – tô hình chữ nhật (x, y, rw, rh) bằng một màu.
func ToMau(anh []byte, w, x, y, rw, rh int, m Mau) {
	for yy := y; yy < y+rh; yy++ {
		for xx := x; xx < x+rw; xx++ {
			copy(anh[(yy*w+xx)*3:], m[:])
		}
	}
}

// VeChu – vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng co lần.
func VeChu(anh []byte, w, h, x, y int, chu string, co int, m Mau) {
	for _, c := range strings.ToUpper(chu) {
		ma, ok := phong[c]
		if !ok {
			ma = phong['?']
		}
		for hang := 0; hang < 7; hang++ {
			bits, _ := strconv.ParseUint(ma[2*hang:2*hang+2], 16, 8)
			for cot := 0; cot < 5; cot++ {
				px, py := x+cot*co, y+hang*co
				if bits&(0x10>>cot) != 0 && px+co <= w && py+co <= h {
					ToMau(anh, w, px, py, co, co, m)
				}
			}
		}
		x += 6 * co
	}
}

// ThuNho – giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d.
func ThuNho(anh []byte, w, h, d int) ([]byte, int, int) {
	if d == 1 {
		return anh, w, h
	}
	w2, h2 := w/d, h/d
	ra := make([]byte, w2*h2*3)
	for y := 0; y < h2; y++ {
		for x := 0; x < w2; x++ {
			copy(ra[(y*w2+x)*3:(y*w2+x)*3+3], anh[(y*d*w+x*d)*3:])
		}
	}
	return ra, w2, h2
}

// PhongTo – phóng to k lần bằng lặp điểm gần nhất: ảnh độ phân giải thấp thành các ô vuông.
func PhongTo(anh []byte, w, h, k int) []byte {
	ra := make([]byte, w*k*h*k*3)
	for y := 0; y < h*k; y++ {
		for x := 0; x < w*k; x++ {
			copy(ra[(y*w*k+x)*3:(y*w*k+x)*3+3], anh[((y/k)*w+x/k)*3:])
		}
	}
	return ra
}

// ---------------------------------------------------------------- Camera tổng hợp

var bangMau = []Mau{{230, 120, 60}, {70, 170, 90}, {70, 120, 220}, {200, 80, 180}, {200, 190, 60}}

// CameraTongHop – nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm biến thật,
// trượt ngang theo thời gian; vẽ tên client và số khung.
type CameraTongHop struct {
	ten string
	nen []byte
}

func MoiCamera(ten string, ma int) *CameraTongHop {
	m := bangMau[(ma-1)%len(bangMau)]
	r := rand.New(rand.NewSource(int64(ma)))
	nen := make([]byte, Rong*Cao*3)
	for y := 0; y < Cao; y++ {
		for x := 0; x < Rong; x++ {
			k := 0.35 + 0.65*(0.5+0.5*math.Sin(2*math.Pi*(float64(x)/40+float64(y)/60)))
			hat := 0
			if r.Float64() < 0.04 { // hạt nhiễu
				hat = r.Intn(24)
			}
			for c := 0; c < 3; c++ {
				nen[(y*Rong+x)*3+c] = byte(min(255, int(float64(m[c])*k)/16*16+hat))
			}
		}
	}
	return &CameraTongHop{ten, nen}
}

func (c *CameraTongHop) LayKhung(so int) []byte {
	dich := (so * 2) % Rong * 3 // trượt 2 điểm ảnh mỗi khung
	anh := make([]byte, 0, len(c.nen))
	for y := 0; y < Cao; y++ {
		hang := c.nen[y*Rong*3 : (y+1)*Rong*3]
		anh = append(anh, hang[dich:]...)
		anh = append(anh, hang[:dich]...)
	}
	toi := Mau{20, 20, 20}
	ToMau(anh, Rong, 6, 6, 6*4*len(c.ten)+6, 7*4+6, toi)
	VeChu(anh, Rong, Cao, 9, 9, c.ten, 4, Mau{255, 255, 255})
	nhan := "#" + leftPad(strconv.Itoa(so), 4)
	ToMau(anh, Rong, 6, Cao-34, 6*2*len(nhan)+4, 7*2+4, toi)
	VeChu(anh, Rong, Cao, 8, Cao-32, nhan, 2, Mau{255, 255, 255})
	return anh
}

func leftPad(s string, n int) string { return strings.Repeat("0", max(0, n-len(s))) + s }

// ---------------------------------------------------------------- Nén, PNG

func NenZlib(b []byte) []byte {
	var buf bytes.Buffer
	z, _ := zlib.NewWriterLevel(&buf, zlib.BestSpeed)
	z.Write(b)
	z.Close()
	return buf.Bytes()
}

func GiaiNen(b []byte) ([]byte, error) {
	z, err := zlib.NewReader(bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	defer z.Close()
	return io.ReadAll(io.LimitReader(z, 4<<20)) // giới hạn kích thước sau giải nén (chống "bom nén")
}

// GhiPNG – chuyển RGB thô sang image.NRGBA rồi mã hóa bằng image/png của thư viện chuẩn.
func GhiPNG(duongDan string, anh []byte, w, h int) error {
	img := image.NewNRGBA(image.Rect(0, 0, w, h))
	for i := 0; i < w*h; i++ {
		copy(img.Pix[4*i:4*i+3], anh[3*i:3*i+3])
		img.Pix[4*i+3] = 255
	}
	f, err := os.Create(duongDan)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, img)
}

// ---------------------------------------------------------------- Kênh điều khiển JSON

func GuiJSON(c net.Conn, v any) error {
	b, _ := json.Marshal(v)
	_, err := c.Write(append(b, '\n'))
	return err
}

func SoNguyen(v any, macDinh int) int {
	if f, ok := v.(float64); ok {
		return int(f)
	}
	return macDinh
}
