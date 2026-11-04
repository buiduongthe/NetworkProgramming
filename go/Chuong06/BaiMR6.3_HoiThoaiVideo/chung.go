package main

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
	rong, cao   = 160, 120 // độ phân giải gốc của camera
	maxDatagram = 1200
	coHeader    = 20
	duLieuToiDa = maxDatagram - coHeader // 1180 byte
	phienBan    = 1
	loaiManh    = 1 // mảnh khung hình
	loaiGiu     = 2 // gói đăng ký/giữ kết nối
	maServer    = 0
	fpsMacDinh  = 10
	manhToiDa   = 256 // tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
)

var t0 = time.Now()

// ---------------------------------------------------------------- Chia mảnh và ghép khung

// header: phiên bản(1) loại(1) mã nguồn(2) số khung(4) chỉ số mảnh(2) tổng mảnh(2) rộng(2) cao(2) ts ms(4)
type header struct {
	loai              byte
	ma                uint16
	soKhung           uint32
	chiSo, tong, w, h uint16
}

func (h header) dongGoi(payload []byte) []byte {
	b := make([]byte, coHeader+len(payload))
	b[0], b[1] = phienBan, h.loai
	be := binary.BigEndian
	be.PutUint16(b[2:], h.ma)
	be.PutUint32(b[4:], h.soKhung)
	be.PutUint16(b[8:], h.chiSo)
	be.PutUint16(b[10:], h.tong)
	be.PutUint16(b[12:], h.w)
	be.PutUint16(b[14:], h.h)
	be.PutUint32(b[16:], uint32(time.Since(t0).Milliseconds()))
	copy(b[coHeader:], payload)
	return b
}

// chiaManh – chia dữ liệu khung nén thành các datagram ≤ 1200 byte.
func chiaManh(ma uint16, soKhung uint32, w, h int, duLieu []byte) [][]byte {
	tong := max(1, (len(duLieu)+duLieuToiDa-1)/duLieuToiDa)
	var ds [][]byte
	for i := 0; i < tong; i++ {
		phan := duLieu[i*duLieuToiDa : min(len(duLieu), (i+1)*duLieuToiDa)]
		ds = append(ds, header{loaiManh, ma, soKhung, uint16(i), uint16(tong), uint16(w), uint16(h)}.dongGoi(phan))
	}
	return ds
}

func goiGiu(ma uint16) []byte { return header{loai: loaiGiu, ma: ma}.dongGoi(nil) }

func tachHeader(b []byte) (header, []byte, bool) {
	if len(b) < coHeader || b[0] != phienBan {
		return header{}, nil, false
	}
	be := binary.BigEndian
	h := header{b[1], be.Uint16(b[2:]), be.Uint32(b[4:]), be.Uint16(b[8:]), be.Uint16(b[10:]),
		be.Uint16(b[12:]), be.Uint16(b[14:])}
	if h.loai == loaiManh && (h.tong == 0 || h.tong > manhToiDa || h.chiSo >= h.tong) {
		return header{}, nil, false
	}
	return h, b[coHeader:], true
}

type khungDo struct {
	tong int
	manh map[int][]byte
	t    time.Time
}

// ghepKhung – ghép các mảnh thành khung cho một luồng. Khi một khung đủ mảnh: trả về khung đó và
// bỏ mọi khung cũ hơn còn dở (video trực tiếp chỉ cần khung mới nhất); khung dở quá 0,5 s cũng bị bỏ.
type ghepKhung struct {
	dang                         map[uint32]*khungDo
	cuoi                         int64 // số khung đã xong/bỏ gần nhất (-1: chưa có)
	manhNhan, khungDu, khungHong int
	manhMat, khungMatHan, tre    int
}

func moiGhep() *ghepKhung { return &ghepKhung{dang: map[uint32]*khungDo{}, cuoi: -1} }

func (g *ghepKhung) bo(so uint32) {
	k := g.dang[so]
	delete(g.dang, so)
	g.khungHong++
	g.manhMat += k.tong - len(k.manh)
}

// them – thêm một mảnh; trả về dữ liệu khung nén khi đủ mảnh, ngược lại nil.
func (g *ghepKhung) them(h header, payload []byte, bayGio time.Time) []byte {
	g.manhNhan++
	if int64(h.soKhung) <= g.cuoi {
		g.tre++ // thuộc khung đã xong hoặc đã bỏ
		return nil
	}
	k := g.dang[h.soKhung]
	if k == nil {
		k = &khungDo{tong: int(h.tong), manh: map[int][]byte{}, t: bayGio}
		g.dang[h.soKhung] = k
	}
	k.manh[int(h.chiSo)] = append([]byte(nil), payload...)
	for s, d := range g.dang {
		if s != h.soKhung && bayGio.Sub(d.t) > 500*time.Millisecond {
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
	delete(g.dang, h.soKhung)
	cu := 0
	for s := range g.dang {
		if s < h.soKhung {
			g.bo(s) // khung cũ còn dở: bỏ
			cu++
		}
	}
	if g.cuoi >= 0 {
		g.khungMatHan += max(0, int(int64(h.soKhung)-g.cuoi-1)-cu)
	}
	g.cuoi = int64(h.soKhung)
	g.khungDu++
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

type mau [3]byte

// toMau – tô hình chữ nhật (x, y, rw, rh) bằng một màu.
func toMau(anh []byte, w, x, y, rw, rh int, m mau) {
	for yy := y; yy < y+rh; yy++ {
		for xx := x; xx < x+rw; xx++ {
			copy(anh[(yy*w+xx)*3:], m[:])
		}
	}
}

// veChu – vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng co lần.
func veChu(anh []byte, w, h, x, y int, chu string, co int, m mau) {
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
					toMau(anh, w, px, py, co, co, m)
				}
			}
		}
		x += 6 * co
	}
}

// thuNho – giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d.
func thuNho(anh []byte, w, h, d int) ([]byte, int, int) {
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

// phongTo – phóng to k lần bằng lặp điểm gần nhất: ảnh độ phân giải thấp thành các ô vuông.
func phongTo(anh []byte, w, h, k int) []byte {
	ra := make([]byte, w*k*h*k*3)
	for y := 0; y < h*k; y++ {
		for x := 0; x < w*k; x++ {
			copy(ra[(y*w*k+x)*3:(y*w*k+x)*3+3], anh[((y/k)*w+x/k)*3:])
		}
	}
	return ra
}

// ---------------------------------------------------------------- Camera tổng hợp

var bangMau = []mau{{230, 120, 60}, {70, 170, 90}, {70, 120, 220}, {200, 80, 180}, {200, 190, 60}}

// cameraTongHop – nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm biến thật,
// trượt ngang theo thời gian; vẽ tên client và số khung.
type cameraTongHop struct {
	ten string
	nen []byte
}

func moiCamera(ten string, ma int) *cameraTongHop {
	m := bangMau[(ma-1)%len(bangMau)]
	r := rand.New(rand.NewSource(int64(ma)))
	nen := make([]byte, rong*cao*3)
	for y := 0; y < cao; y++ {
		for x := 0; x < rong; x++ {
			k := 0.35 + 0.65*(0.5+0.5*math.Sin(2*math.Pi*(float64(x)/40+float64(y)/60)))
			hat := 0
			if r.Float64() < 0.04 { // hạt nhiễu
				hat = r.Intn(24)
			}
			for c := 0; c < 3; c++ {
				nen[(y*rong+x)*3+c] = byte(min(255, int(float64(m[c])*k)/16*16+hat))
			}
		}
	}
	return &cameraTongHop{ten, nen}
}

func (c *cameraTongHop) layKhung(so int) []byte {
	dich := (so * 2) % rong * 3 // trượt 2 điểm ảnh mỗi khung
	anh := make([]byte, 0, len(c.nen))
	for y := 0; y < cao; y++ {
		hang := c.nen[y*rong*3 : (y+1)*rong*3]
		anh = append(anh, hang[dich:]...)
		anh = append(anh, hang[:dich]...)
	}
	toi := mau{20, 20, 20}
	toMau(anh, rong, 6, 6, 6*4*len(c.ten)+6, 7*4+6, toi)
	veChu(anh, rong, cao, 9, 9, c.ten, 4, mau{255, 255, 255})
	nhan := "#" + leftPad(strconv.Itoa(so), 4)
	toMau(anh, rong, 6, cao-34, 6*2*len(nhan)+4, 7*2+4, toi)
	veChu(anh, rong, cao, 8, cao-32, nhan, 2, mau{255, 255, 255})
	return anh
}

func leftPad(s string, n int) string { return strings.Repeat("0", max(0, n-len(s))) + s }

// ---------------------------------------------------------------- Nén, PNG

func nenZlib(b []byte) []byte {
	var buf bytes.Buffer
	z, _ := zlib.NewWriterLevel(&buf, zlib.BestSpeed)
	z.Write(b)
	z.Close()
	return buf.Bytes()
}

func giaiNen(b []byte) ([]byte, error) {
	z, err := zlib.NewReader(bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	defer z.Close()
	return io.ReadAll(io.LimitReader(z, 4<<20)) // giới hạn kích thước sau giải nén (chống "bom nén")
}

// ghiPNG – chuyển RGB thô sang image.NRGBA rồi mã hóa bằng image/png của thư viện chuẩn.
func ghiPNG(duongDan string, anh []byte, w, h int) error {
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

func guiJSON(c net.Conn, v any) error {
	b, _ := json.Marshal(v)
	_, err := c.Write(append(b, '\n'))
	return err
}

func soNguyen(v any, macDinh int) int {
	if f, ok := v.(float64); ok {
		return int(f)
	}
	return macDinh
}
