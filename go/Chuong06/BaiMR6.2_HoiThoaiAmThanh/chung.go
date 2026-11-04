package main

// Các thủ tục dùng chung cho server và client hội thoại âm thanh.
//
// Âm thanh số PCM (Pulse-Code Modulation) 16 bit, mono, tần số lấy mẫu 16000 Hz:
//   - lấy mẫu: mỗi giây đo biên độ sóng âm 16000 lần; mỗi mẫu là số nguyên 16 bit có dấu
//     (-32768 … 32767). Theo định lý Nyquist, 16 kHz biểu diễn được âm tới 8 kHz – đủ cho giọng nói;
//   - tốc độ bit: 16000 mẫu/s × 16 bit = 256 kbit/s (32000 byte/s);
//   - khung 20 ms: 16000 × 0,020 = 320 mẫu = 640 byte -> 50 gói mỗi giây;
//   - mỗi gói thêm 12 byte RTP + 8 byte UDP + 20 byte IPv4 = 40 byte phụ phí
//     -> 50 × 40 × 8 = 16 kbit/s, tổng ≈ 272 kbit/s mỗi chiều cho mỗi client.
//
// Mức âm đo bằng dBFS (decibel so với toàn thang): 0 dBFS là mức RMS lớn nhất (32768). Sóng sin
// biên độ 0,25 toàn thang có RMS 0,25/√2 -> -15,1 dBFS; nhân biên độ 2 -> +6,02 dB.

import (
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net"
	"os"
	"strings"
)

const (
	tanSoMau     = 16000                     // mẫu/giây
	khungMs      = 20                        // độ dài một khung (gói) âm thanh
	mauMoiKhung  = tanSoMau * khungMs / 1000 // 320 mẫu
	byteMoiKhung = mauMoiKhung * 2           // 640 byte
	ptAmThanh    = 96                        // payload type động (RFC 3551) cho L16/16000/mono
	ptGiuKetNoi  = 127                       // gói rỗng: đăng ký địa chỉ UDP, giữ kết nối khi mic tắt
	ssrcServer   = 0                         // mã nguồn của luồng trộn do server gửi
)

// ---------------------------------------------------------------- Gói RTP (RFC 3550)

// dongGoiRTP – header cố định 12 byte: V=2|P|X|CC, M|PT, số thứ tự 16 bit, nhãn thời gian
// 32 bit (đơn vị: mẫu), SSRC 32 bit (ở đây là mã client).
func dongGoiRTP(pt byte, seq uint16, ts uint32, ssrc uint32, payload []byte) []byte {
	b := make([]byte, 12+len(payload))
	b[0], b[1] = 0x80, pt&0x7F
	binary.BigEndian.PutUint16(b[2:], seq)
	binary.BigEndian.PutUint32(b[4:], ts)
	binary.BigEndian.PutUint32(b[8:], ssrc)
	copy(b[12:], payload)
	return b
}

type goiRTP struct {
	pt      byte
	seq     uint16
	ts      uint32
	ssrc    uint32
	payload []byte
}

func tachRTP(b []byte) (goiRTP, bool) {
	if len(b) < 12 || b[0]>>6 != 2 { // phiên bản RTP phải là 2
		return goiRTP{}, false
	}
	return goiRTP{b[1] & 0x7F, binary.BigEndian.Uint16(b[2:]), binary.BigEndian.Uint32(b[4:]),
		binary.BigEndian.Uint32(b[8:]), b[12:]}, true
}

// ---------------------------------------------------------------- PCM <-> byte

// pcmSangByte – RTP truyền L16 theo thứ tự byte mạng (big-endian), RFC 3551 mục 4.5.11.
func pcmSangByte(mau []int16) []byte {
	b := make([]byte, 2*len(mau))
	for i, x := range mau {
		binary.BigEndian.PutUint16(b[2*i:], uint16(x))
	}
	return b
}

func byteSangPCM(b []byte) []int16 {
	m := make([]int16, len(b)/2)
	for i := range m {
		m[i] = int16(binary.BigEndian.Uint16(b[2*i:]))
	}
	return m
}

// catNgon – cắt ngọn (clipping): giá trị vượt phạm vi 16 bit bị giữ ở biên.
func catNgon(x float64) (int16, bool) {
	v := math.Round(x)
	if v > 32767 {
		return 32767, true
	}
	if v < -32768 {
		return -32768, true
	}
	return int16(v), false
}

// ---------------------------------------------------------------- Mức âm

// mucDBFS – mức RMS (căn bậc hai của trung bình bình phương) theo dBFS.
func mucDBFS(mau []int16) float64 {
	if len(mau) == 0 {
		return math.Inf(-1)
	}
	tong := 0.0
	for _, x := range mau {
		tong += float64(x) * float64(x)
	}
	rms := math.Sqrt(tong / float64(len(mau)))
	if rms == 0 {
		return math.Inf(-1)
	}
	return 20 * math.Log10(rms/32768)
}

// thanhMuc – thanh đo mức dạng chữ: -60 dBFS (trống) … 0 dBFS (đầy).
func thanhMuc(db float64, rong int) string {
	n := 0
	if !math.IsInf(db, -1) {
		n = int(math.Round((db + 60) / 60 * float64(rong)))
		n = max(0, min(rong, n))
	}
	return "|" + strings.Repeat("#", n) + strings.Repeat("-", rong-n) + "|"
}

func chuDB(db float64, rong int) string {
	if db < -90 {
		return fmt.Sprintf("%*s", rong, "---")
	}
	return fmt.Sprintf("%*.1f", rong, db)
}

// ---------------------------------------------------------------- Bộ đệm jitter

// boDemJitter – bộ đệm chống rung trễ cho một luồng RTP. Giữ vài khung (doSau) trước khi bắt
// đầu phát, rồi cứ mỗi 20 ms lấy đúng khung có số thứ tự kế tiếp:
//   - có khung -> phát; thiếu khung nhưng đã có khung sau -> coi như mất, phát khoảng lặng;
//   - bộ đệm rỗng -> cạn, chờ nạp lại doSau khung; khung đến sau lượt phát -> trễ, bỏ.
//
// Đồng thời thống kê theo RFC 3550: số gói mất (khoảng trống số thứ tự) và jitter (trung bình
// trượt 1/16 của chênh lệch thời gian truyền giữa hai gói liên tiếp).
type boDemJitter struct {
	doSau, toiDa                int
	khung                       map[int64][]byte // số thứ tự mở rộng -> payload
	keTiep                      int64
	dangPhat, coSeq             bool
	seqLonNhat, seqDau          int64
	nhan, tre, thieu, can, tran int
	jitter                      float64 // đơn vị: mẫu (1 ms = 16 mẫu)
	truyenTruoc                 float64
	coTruyen                    bool
}

func moiBoDem(doSau int) *boDemJitter {
	return &boDemJitter{doSau: doSau, toiDa: 50, khung: map[int64][]byte{}}
}

// moRong – số thứ tự 16 bit quay vòng sau 65535; đổi thành số nguyên tăng mãi.
func (b *boDemJitter) moRong(seq uint16) int64 {
	if !b.coSeq {
		return int64(seq)
	}
	lech := int64((seq - uint16(b.seqLonNhat)) & 0xFFFF)
	if lech < 0x8000 {
		return b.seqLonNhat + lech
	}
	return b.seqLonNhat - (0x10000 - lech)
}

func (b *boDemJitter) nhanGoi(seq uint16, ts uint32, payload []byte, lucDen float64) {
	s := b.moRong(seq)
	if !b.coSeq {
		b.coSeq, b.seqDau, b.seqLonNhat = true, s, s
	}
	if s > b.seqLonNhat {
		b.seqLonNhat = s
	}
	b.nhan++
	truyen := lucDen*tanSoMau - float64(ts) // thời gian truyền (lệch hằng số không quan trọng)
	if b.coTruyen {
		d := math.Abs(truyen - b.truyenTruoc)
		if d < tanSoMau { // bỏ qua bước nhảy lớn
			b.jitter += (d - b.jitter) / 16
		}
	}
	b.truyenTruoc, b.coTruyen = truyen, true
	if b.dangPhat && s < b.keTiep {
		b.tre++ // đến quá muộn, đã qua lượt phát
		return
	}
	b.khung[s] = append([]byte(nil), payload...)
	if len(b.khung) > b.toiDa { // tràn: bỏ khung cũ nhất
		delete(b.khung, b.nhoNhat())
		b.tran++
	}
}

func (b *boDemJitter) nhoNhat() int64 {
	first, m := true, int64(0)
	for k := range b.khung {
		if first || k < m {
			first, m = false, k
		}
	}
	return m
}

// lay – gọi mỗi 20 ms; trả về payload của khung cần phát hoặc nil (phát khoảng lặng).
func (b *boDemJitter) lay() []byte {
	if !b.dangPhat {
		if len(b.khung) < b.doSau {
			return nil
		}
		b.dangPhat, b.keTiep = true, b.nhoNhat()
	}
	if p, ok := b.khung[b.keTiep]; ok {
		delete(b.khung, b.keTiep)
		b.keTiep++
		return p
	}
	if len(b.khung) == 0 { // cạn bộ đệm: chờ nạp lại
		b.dangPhat = false
		b.can++
		return nil
	}
	b.thieu++ // mất khung này, đã có khung sau
	b.keTiep++
	return nil
}

func (b *boDemJitter) xoa() {
	clear(b.khung)
	b.dangPhat = false
}

// mat – số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận.
func (b *boDemJitter) mat() int {
	if !b.coSeq {
		return 0
	}
	return max(0, int(b.seqLonNhat-b.seqDau+1)-b.nhan)
}

func (b *boDemJitter) jitterMs() float64 { return b.jitter / (tanSoMau / 1000) }

// ---------------------------------------------------------------- Phân tích tần số

// goertzelBienDo – biên độ thành phần tần số f trong một khối mẫu, bằng thuật toán Goertzel:
//
//	s[n] = x[n] + 2cos(ω)·s[n-1] - s[n-2],  ω = 2π·f/fs;  |X|² = s1² + s2² - 2cos(ω)·s1·s2
//
// Tín hiệu nhân trước với cửa sổ Hann để giảm rò phổ; biên độ sin A ≈ 2|X| / Σw.
func goertzelBienDo(mau []int16, f float64) float64 {
	n := len(mau)
	w := 2 * math.Pi * f / tanSoMau
	heSo := 2 * math.Cos(w)
	var s1, s2, tongW float64
	k := 2 * math.Pi / float64(n-1)
	for i, x := range mau {
		cs := 0.5 - 0.5*math.Cos(k*float64(i)) // cửa sổ Hann
		tongW += cs
		s1, s2 = float64(x)*cs+heSo*s1-s2, s1
	}
	return 2 * math.Sqrt(math.Max(s1*s1+s2*s2-heSo*s1*s2, 0)) / tongW
}

// mucAmDBFS – mức RMS (dBFS) của một âm trong đoạn dài: chia khối 0,1 s (độ phân giải 10 Hz),
// đo biên độ từng khối rồi lấy trung bình công suất – không bị sai khi pha của âm nhảy giữa đoạn.
func mucAmDBFS(mau []int16, f float64) float64 {
	const khoi = 1600
	tong, n := 0.0, 0
	for i := 0; i+khoi <= len(mau); i += khoi {
		a := goertzelBienDo(mau[i:i+khoi], f)
		tong += a * a
		n++
	}
	if n == 0 {
		return math.Inf(-1)
	}
	rms := math.Sqrt(tong/float64(n)) / math.Sqrt2 // biên độ sin -> RMS
	if rms <= 0.5 {
		return math.Inf(-1)
	}
	return 20 * math.Log10(rms/32768)
}

// ---------------------------------------------------------------- Tệp WAV (viết tay header RIFF)

// ghiWAV – WAV gồm header RIFF 44 byte rồi các mẫu little-endian (khác thứ tự byte mạng của RTP):
//
//	"RIFF" <cỡ-8> "WAVE" | "fmt " 16 <PCM=1> <kênh=1> <fs> <byte/s> <byte/mẫu> <16 bit> | "data" <cỡ> ...
func ghiWAV(duongDan string, mau []int16) error {
	b := make([]byte, 44+2*len(mau))
	le := binary.LittleEndian
	copy(b[0:], "RIFF")
	le.PutUint32(b[4:], uint32(36+2*len(mau)))
	copy(b[8:], "WAVEfmt ")
	le.PutUint32(b[16:], 16)         // cỡ khối fmt
	le.PutUint16(b[20:], 1)          // định dạng PCM
	le.PutUint16(b[22:], 1)          // mono
	le.PutUint32(b[24:], tanSoMau)   // tần số lấy mẫu
	le.PutUint32(b[28:], tanSoMau*2) // byte mỗi giây
	le.PutUint16(b[32:], 2)          // byte mỗi mẫu (mọi kênh)
	le.PutUint16(b[34:], 16)         // bit mỗi mẫu
	copy(b[36:], "data")
	le.PutUint32(b[40:], uint32(2*len(mau)))
	for i, x := range mau {
		le.PutUint16(b[44+2*i:], uint16(x))
	}
	return os.WriteFile(duongDan, b, 0o644)
}

// docWAV – đọc WAV PCM 16 bit mono 16000 Hz: duyệt các khối RIFF, tìm "fmt " và "data".
func docWAV(duongDan string) ([]int16, error) {
	b, err := os.ReadFile(duongDan)
	if err != nil {
		return nil, err
	}
	if len(b) < 12 || string(b[0:4]) != "RIFF" || string(b[8:12]) != "WAVE" {
		return nil, errors.New("không phải tệp WAV")
	}
	le := binary.LittleEndian
	hopLe := false
	for i := 12; i+8 <= len(b); {
		id, co := string(b[i:i+4]), int(le.Uint32(b[i+4:]))
		than := b[i+8 : min(len(b), i+8+co)]
		switch id {
		case "fmt ":
			hopLe = len(than) >= 16 && le.Uint16(than[0:]) == 1 && le.Uint16(than[2:]) == 1 &&
				le.Uint32(than[4:]) == tanSoMau && le.Uint16(than[14:]) == 16
		case "data":
			if !hopLe {
				return nil, errors.New("cần WAV PCM 16 bit, mono, 16000 Hz")
			}
			mau := make([]int16, len(than)/2)
			for j := range mau {
				mau[j] = int16(le.Uint16(than[2*j:]))
			}
			return mau, nil
		}
		i += 8 + co + co%2 // khối RIFF được đệm tới số byte chẵn
	}
	return nil, errors.New("không thấy khối data")
}

// ---------------------------------------------------------------- Kênh điều khiển JSON

// guiJSON – mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\n' (định khung theo dòng).
func guiJSON(c net.Conn, v any) error {
	b, _ := json.Marshal(v)
	_, err := c.Write(append(b, '\n'))
	return err
}
