// Package tructiep – lõi phát trực tiếp 1→n của bài mở rộng MR6.5: các thủ tục dùng chung, máy chủ
// trung chuyển (SFU), máy phát và người xem. Bản dòng lệnh (thư mục cha) và ứng dụng giao diện
// (thư mục gui/, mô-đun riêng vì cần Fyne) cùng dùng gói này – Go không cho nhập gói main.
package tructiep

// Các thủ tục dùng chung cho máy chủ, máy phát và người xem.
//
// Tự chứa (không nhập mã của bài khác): chép và mở rộng phần cần thiết của MR6.2 (âm thanh RTP, bộ
// đệm jitter) và MR6.3 (header video, chia mảnh, ghép khung, phông chữ 5×7, PNG).
//
// Hai loại datagram trên CÙNG một socket UDP, phân biệt bằng byte đầu tiên:
//   - 0x80: gói RTP âm thanh (RFC 3550): V=2, P=0, X=0, CC=0 -> byte đầu luôn là 0x80;
//   - 0x01: gói video của bài (phiên bản 1): loại 2 = giữ kết nối, 3 = mảnh khung I, 4 = mảnh khung P.
//
// Nén liên khung (inter-frame) đơn giản – ý tưởng của mọi codec video (H.264, VP8, AV1):
//   - khung khóa I (intra): nén một mình  ->  I = zlib(rgb thô);
//   - khung sai khác P (predicted): chỉ mã hóa phần THAY ĐỔI so với khung ngay trước
//     P = zlib(XOR từng byte(khung hiện tại, khung trước))
//     cảnh tĩnh -> XOR gần như toàn 0 -> nén còn vài trăm byte. Giải mã P cần đúng khung trước:
//     mất một khung là hỏng cả chuỗi tới khung I kế tiếp (nhóm khung – GOP, Group Of Pictures).
//
// Giao thức giống hệt bản Python (và các ngôn ngữ khác trong kho) tới từng byte của header; dữ liệu
// zlib (mức 6) do compress/zlib của Go sinh có thể khác byte với zlib C nhưng giải nén được lẫn nhau.

import (
	"bufio"
	"bytes"
	"compress/zlib"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"image"
	"image/png"
	"io"
	"math"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

// ---------------------------------------------------------------- Âm thanh (như MR6.2)
const (
	TanSoMau     = 16000                     // mẫu/giây
	KhungMs      = 20                        // một gói âm thanh = 20 ms
	MauMoiKhung  = TanSoMau * KhungMs / 1000 // 320 mẫu
	ByteMoiKhung = MauMoiKhung * 2           // 640 byte (L16)
	PtAmThanh    = 96                        // payload type động (RFC 3551) cho L16/16000/mono
	CoRtp        = 12                        // header RTP: V|P|X|CC, M|PT, seq, ts, ssrc
	DauRtp       = 0x80
	MsMoiMau     = 1000.0 / TanSoMau
)

// ---------------------------------------------------------------- Video (như MR6.3)
const (
	Rong, Cao              = 160, 120
	FpsMacDinh, GopMacDinh = 10, 20
	MaxDatagram            = 1200
	CoHeader               = 20                     // !BBHIHHHHI
	DuLieuToiDa            = MaxDatagram - CoHeader // 1180 byte
	PhienBan               = 1                      // cũng là byte đầu của gói video (0x01)
	LoaiGiu, LoaiI, LoaiP  = 2, 3, 4
	ManhToiDa              = 256
)

// ---------------------------------------------------------------- Giới hạn, thời gian chờ
const (
	GioiHanDong   = 64 * 1024 // một dòng JSON điều khiển tối đa 64 KB
	BoDemGopToiDa = 4 << 20   // bộ đệm GOP ở server tối đa 4 MB
	PliServerS    = 0.5       // server chuyển xin_khung_khoa tối đa 1 lần / 500 ms
	PliXemS       = 1.0       // người xem gửi xin_khung_khoa tối đa 1 lần / 1 s
	ChoThamChieuS = 0.2       // chờ khung thiếu trước khi coi là mất tham chiếu
	ChoManhS      = 0.5       // chờ đủ mảnh của một khung
	HangHienThi   = 30        // hàng khung chờ hiển thị tối đa 30 khung
	DoSauMacDinh  = 5         // bộ đệm jitter 5 khung = 100 ms
)

var goc = time.Now()

// BayGio – đồng hồ đơn điệu (giây, số thực) như time.monotonic() của Python: không nhảy khi đổi giờ
// hệ thống. time.Since dùng phần đơn điệu của time.Time.
func BayGio() float64 { return time.Since(goc).Seconds() }

// GioUnixMs – giờ thực (ms Unix, có phần lẻ) – chỉ dùng cho báo cáo đồng hồ và trễ đầu-cuối.
func GioUnixMs() float64 { return float64(time.Now().UnixNano()) / 1e6 }

// ngu – chờ s giây (s ≤ 0 thì không chờ).
func ngu(s float64) {
	if s > 0 {
		time.Sleep(time.Duration(s * float64(time.Second)))
	}
}

// ================================================================ RTP âm thanh

// DongGoiRtp – header RTP 12 byte: 0x80 (V=2), PT=96 (M=0), số thứ tự, nhãn thời gian (mẫu), SSRC.
func DongGoiRtp(seq uint16, ts, ssrc uint32, payload []byte) []byte {
	b := make([]byte, CoRtp+len(payload))
	b[0], b[1] = DauRtp, PtAmThanh
	binary.BigEndian.PutUint16(b[2:], seq)
	binary.BigEndian.PutUint32(b[4:], ts)
	binary.BigEndian.PutUint32(b[8:], ssrc)
	copy(b[CoRtp:], payload)
	return b
}

// GoiRtp – các trường của một gói RTP đã tách.
type GoiRtp struct {
	Pt      byte
	Seq     uint16
	Ts      uint32
	Ssrc    uint32
	Payload []byte
}

// TachRtp – trả về gói đã tách, ok = false nếu sai dạng.
func TachRtp(goi []byte) (GoiRtp, bool) {
	if len(goi) < CoRtp || goi[0]>>6 != 2 {
		return GoiRtp{}, false
	}
	be := binary.BigEndian
	return GoiRtp{goi[1] & 0x7F, be.Uint16(goi[2:]), be.Uint32(goi[4:]), be.Uint32(goi[8:]), goi[CoRtp:]}, true
}

// PcmSangByte – L16 truyền theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11.
func PcmSangByte(mau []int16) []byte {
	b := make([]byte, 2*len(mau))
	for i, x := range mau {
		binary.BigEndian.PutUint16(b[2*i:], uint16(x))
	}
	return b
}

func ByteSangPcm(b []byte) []int16 {
	mau := make([]int16, len(b)/2)
	for i := range mau {
		mau[i] = int16(binary.BigEndian.Uint16(b[2*i:]))
	}
	return mau
}

// MucDbfs – mức RMS theo dBFS (0 dBFS = toàn thang 32768); -Inf khi im lặng hoặc không có mẫu.
func MucDbfs(mau []int16) float64 {
	if len(mau) == 0 {
		return math.Inf(-1)
	}
	tong := 0.0
	for _, x := range mau {
		tong += float64(x) * float64(x)
	}
	rms := math.Sqrt(tong / float64(len(mau)))
	if rms <= 0 {
		return math.Inf(-1)
	}
	return 20 * math.Log10(rms/32768)
}

type khungAm struct {
	ts      uint32
	payload []byte
}

// KhungPhat – một lượt lấy của bộ đệm jitter: Mat = true khi khung này bị mất (phát im lặng).
type KhungPhat struct {
	Ts      uint32
	Payload []byte
	Mat     bool
}

// BoDemJitter – bộ đệm chống rung trễ (jitter buffer) cho luồng RTP – như MR6.2, thêm nhãn thời gian.
//
// Giữ DoSau khung trước khi bắt đầu phát, rồi cứ 20 ms lấy đúng khung có số thứ tự kế tiếp:
//   - có khung  -> trả về (ts, payload);
//   - thiếu khung nhưng đã có khung sau -> mất: trả về (ts ước lượng, Mat) – phát im lặng, đồng hồ
//     phát vẫn tiến 20 ms;
//   - rỗng -> cạn: trả về nil (đồng hồ phát dừng), chờ nạp lại DoSau khung.
type BoDemJitter struct {
	DoSau, toiDa                int
	khung                       map[int64]khungAm // số thứ tự mở rộng -> khung
	keTiep                      int64
	dangPhat                    bool
	seqLonNhat, seqDau          int64
	coSeq                       bool
	tsCuoi                      uint32
	Nhan, Tre, Thieu, Can, Tran int
}

func MoiBoDemJitter(doSau int) *BoDemJitter {
	return &BoDemJitter{DoSau: doSau, toiDa: 60, khung: map[int64]khungAm{}}
}

// moRong – số thứ tự 16 bit quay vòng -> số nguyên tăng mãi.
func (b *BoDemJitter) moRong(seq uint16) int64 {
	if !b.coSeq {
		return int64(seq)
	}
	lech := (int64(seq) - b.seqLonNhat) & 0xFFFF
	if lech < 0x8000 {
		return b.seqLonNhat + lech
	}
	return b.seqLonNhat - (0x10000 - lech)
}

func (b *BoDemJitter) nhoNhat() int64 {
	first, m := true, int64(0)
	for s := range b.khung {
		if first || s < m {
			m, first = s, false
		}
	}
	return m
}

func (b *BoDemJitter) NhanGoi(seq uint16, ts uint32, payload []byte) {
	s := b.moRong(seq)
	if !b.coSeq {
		b.seqDau, b.seqLonNhat, b.coSeq = s, s, true
	}
	if s > b.seqLonNhat {
		b.seqLonNhat = s
	}
	b.Nhan++
	if b.dangPhat && s < b.keTiep {
		b.Tre++ // đến sau lượt phát của nó
		return
	}
	b.khung[s] = khungAm{ts, payload}
	if len(b.khung) > b.toiDa {
		delete(b.khung, b.nhoNhat())
		b.Tran++
	}
}

// Lay – gọi mỗi 20 ms. Trả về khung cần phát hoặc nil (đang chờ nạp).
func (b *BoDemJitter) Lay() *KhungPhat {
	if !b.dangPhat {
		if len(b.khung) < b.DoSau {
			return nil
		}
		b.dangPhat, b.keTiep = true, b.nhoNhat()
	}
	if k, ok := b.khung[b.keTiep]; ok {
		delete(b.khung, b.keTiep)
		b.keTiep++
		b.tsCuoi = k.ts
		return &KhungPhat{Ts: k.ts, Payload: k.payload}
	}
	if len(b.khung) == 0 {
		b.dangPhat = false
		b.Can++
		return nil
	}
	b.Thieu++ // mất khung này
	b.keTiep++
	b.tsCuoi += MauMoiKhung
	return &KhungPhat{Ts: b.tsCuoi, Mat: true}
}

// Xoa – bỏ mọi khung đang giữ (khi tắt nhận âm thanh); lần sau nạp lại từ đầu.
func (b *BoDemJitter) Xoa() {
	b.khung = map[int64]khungAm{}
	b.dangPhat = false
}

// DatDoSau – đổi độ sâu khi đang chạy: tăng -> dừng phát để nạp thêm; giảm -> bỏ khung cũ nhất.
func (b *BoDemJitter) DatDoSau(n int) {
	b.DoSau = n
	if len(b.khung) < n {
		b.dangPhat = false
	}
	for len(b.khung) > n {
		s := b.nhoNhat()
		delete(b.khung, s)
		if b.dangPhat {
			b.keTiep = s + 1
		}
	}
}

// Mat – số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận.
func (b *BoDemJitter) Mat() int {
	if !b.coSeq {
		return 0
	}
	return max(0, int(b.seqLonNhat-b.seqDau+1)-b.Nhan)
}

// ================================================================ Gói video

// Header – phiên bản(1) loại(1) mã nguồn(2) số khung(4) chỉ số mảnh(2) tổng mảnh(2) rộng(2) cao(2) ts ms(4).
type Header struct {
	Loai              byte
	Ma                uint16
	SoKhung           uint32
	ChiSo, Tong, W, H uint16
	Ts                uint32
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
	be.PutUint32(b[16:], h.Ts)
	copy(b[CoHeader:], payload)
	return b
}

// ChiaManh – chia dữ liệu một khung (I hoặc P) thành datagram ≤ 1200 byte.
func ChiaManh(loai byte, ma uint16, soKhung uint32, w, h int, tsMs uint32, duLieu []byte) ([][]byte, error) {
	tong := max(1, (len(duLieu)+DuLieuToiDa-1)/DuLieuToiDa)
	if tong > ManhToiDa {
		return nil, errors.New("khung quá lớn")
	}
	ds := make([][]byte, 0, tong)
	for i := 0; i < tong; i++ {
		phan := duLieu[i*DuLieuToiDa : min(len(duLieu), (i+1)*DuLieuToiDa)]
		ds = append(ds, Header{loai, ma, soKhung, uint16(i), uint16(tong), uint16(w), uint16(h), tsMs}.DongGoi(phan))
	}
	return ds, nil
}

// GoiGiu – gói giữ kết nối / đăng ký địa chỉ UDP: loại 2, các trường khác 0.
func GoiGiu(ma uint16) []byte { return Header{Loai: LoaiGiu, Ma: ma}.DongGoi(nil) }

// TachHeader – trả về header và phần dữ liệu, ok = false nếu sai dạng.
func TachHeader(b []byte) (Header, []byte, bool) {
	if len(b) < CoHeader || len(b) > MaxDatagram || b[0] != PhienBan {
		return Header{}, nil, false
	}
	be := binary.BigEndian
	h := Header{b[1], be.Uint16(b[2:]), be.Uint32(b[4:]), be.Uint16(b[8:]), be.Uint16(b[10:]),
		be.Uint16(b[12:]), be.Uint16(b[14:]), be.Uint32(b[16:])}
	if h.Loai != LoaiGiu && h.Loai != LoaiI && h.Loai != LoaiP {
		return Header{}, nil, false
	}
	if h.Loai != LoaiGiu && (h.Tong == 0 || h.Tong > ManhToiDa || h.ChiSo >= h.Tong || h.W == 0 || h.H == 0) {
		return Header{}, nil, false
	}
	return h, b[CoHeader:], true
}

// KhungNen – một khung đủ mảnh (dữ liệu còn nén) hoặc đã giải mã (Anh = rgb thô).
type KhungNen struct {
	Loai   byte
	So     int64
	Ts     uint32
	W, H   int
	DuLieu []byte
}

type dauKhung struct {
	loai byte
	tong int
	w, h uint16
	ts   uint32
}

type khungDo struct {
	dau  dauKhung
	manh map[int][]byte
	t    float64
}

// GhepKhung – ghép mảnh thành khung (I hoặc P). Khác MR6.3: KHÔNG bỏ khung cũ còn dở khi khung mới
// đủ, vì khung P cần mọi khung trước nó – khung dở chỉ bị bỏ khi chờ quá Cho giây.
type GhepKhung struct {
	Cho                                        float64
	dang                                       map[int64]*khungDo
	xong                                       map[int64]bool // khung đã đủ hoặc đã bỏ (mảnh đến sau = trễ)
	lonNhat                                    int64
	ManhNhan, KhungDu, KhungHong, ManhMat, Tre int
}

func MoiGhep() *GhepKhung {
	return &GhepKhung{Cho: ChoManhS, dang: map[int64]*khungDo{}, xong: map[int64]bool{}, lonNhat: -1}
}

// DonDep – bỏ (tính là hỏng) các khung dở chờ quá Cho giây, trừ khung "tru" (-1: không trừ).
func (g *GhepKhung) DonDep(bayGio float64, tru int64) {
	for s, d := range g.dang {
		if bayGio-d.t > g.Cho && s != tru {
			delete(g.dang, s)
			g.KhungHong++
			g.ManhMat += d.dau.tong - len(d.manh)
			g.xong[s] = true
		}
	}
}

// BoDangDo – bỏ các khung đang ghép dở mà không tính là hỏng (khi tắt nhận video).
func (g *GhepKhung) BoDangDo() {
	for s := range g.dang {
		g.xong[s] = true
	}
	g.dang = map[int64]*khungDo{}
}

// Them – thêm một mảnh; trả về khung khi vừa đủ mảnh, ngược lại nil.
func (g *GhepKhung) Them(h Header, payload []byte, bayGio float64) *KhungNen {
	g.ManhNhan++
	so := int64(h.SoKhung)
	if g.xong[so] || so < g.lonNhat-1000 {
		g.Tre++
		return nil
	}
	dau := dauKhung{h.Loai, int(h.Tong), h.W, h.H, h.Ts}
	k := g.dang[so]
	if k == nil {
		k = &khungDo{dau: dau, manh: map[int][]byte{}, t: bayGio}
		g.dang[so] = k
	}
	if k.dau != dau {
		return nil // mâu thuẫn với mảnh trước: bỏ mảnh
	}
	k.manh[int(h.ChiSo)] = append([]byte(nil), payload...)
	g.DonDep(bayGio, so)
	if len(k.manh) < k.dau.tong {
		return nil
	}
	delete(g.dang, so)
	g.xong[so] = true
	g.lonNhat = max(g.lonNhat, so)
	if len(g.xong) > 3000 {
		for s := range g.xong {
			if s <= g.lonNhat-1000 {
				delete(g.xong, s)
			}
		}
	}
	g.KhungDu++
	var duLieu []byte
	for j := 0; j < k.dau.tong; j++ {
		duLieu = append(duLieu, k.manh[j]...)
	}
	return &KhungNen{h.Loai, so, h.Ts, int(h.W), int(h.H), duLieu}
}

// ================================================================ Mã hóa / giải mã I, P

// XorKhung – XOR từng byte hai khung cùng kích thước.
func XorKhung(a, b []byte) []byte {
	ra := make([]byte, len(a))
	for i := range a {
		ra[i] = a[i] ^ b[i]
	}
	return ra
}

// NenZlib – zlib mức 6 (như zlib.compress(…, 6) của Python).
func NenZlib(b []byte) []byte {
	var buf bytes.Buffer
	z, _ := zlib.NewWriterLevel(&buf, 6)
	z.Write(b)
	z.Close()
	return buf.Bytes()
}

// GiaiNen – giải nén zlib có giới hạn kích thước đầu ra 4 MB (chống "bom nén").
func GiaiNen(b []byte) ([]byte, error) {
	z, err := zlib.NewReader(bytes.NewReader(b))
	if err != nil {
		return nil, err
	}
	defer z.Close()
	ra, err := io.ReadAll(io.LimitReader(z, 4<<20+1))
	if err != nil {
		return nil, err
	}
	if len(ra) > 4<<20 {
		return nil, errors.New("dữ liệu giải nén quá lớn")
	}
	return ra, nil
}

func MaHoaI(anh []byte) []byte        { return NenZlib(anh) }
func MaHoaP(anh, truoc []byte) []byte { return NenZlib(XorKhung(anh, truoc)) }

type thamChieu struct {
	so   int64
	anh  []byte
	w, h int
}

type khungCho struct {
	ts     uint32
	w, h   int
	duLieu []byte
	luc    float64
}

// GiaiMaVideo – giải mã chuỗi I/P theo thứ tự số khung.
//
//   - I: giải nén -> thành khung tham chiếu (bỏ mọi khung cũ hơn đang chờ).
//   - P số k: giải được khi tham chiếu là k−1. Nếu khung k đủ mảnh mà tham chiếu < k−1: giữ chờ khung
//     thiếu tối đa ChoThamChieuS; quá hạn -> MẤT THAM CHIẾU: đếm đứng hình, bỏ mọi P tới I kế tiếp,
//     KiemTra() trả true để người xem gửi xin_khung_khoa.
//   - Chưa từng có tham chiếu (vừa vào xem): chỉ chờ I, không tính đứng hình.
type GiaiMaVideo struct {
	Cho                          float64
	tc                           *thamChieu
	MatThamChieu                 bool
	LucMat                       float64
	coLucMat                     bool
	doi                          map[int64]khungCho
	SoI, SoP, BoP, DungHinh, Loi int
}

func MoiGiaiMa() *GiaiMaVideo { return &GiaiMaVideo{Cho: ChoThamChieuS, doi: map[int64]khungCho{}} }

// ThamChieu – số khung tham chiếu hiện tại (ok = false nếu chưa có).
func (g *GiaiMaVideo) ThamChieu() (int64, bool) {
	if g.tc == nil {
		return 0, false
	}
	return g.tc.so, true
}

// CoDoi – đang có khung P chờ khung thiếu.
func (g *GiaiMaVideo) CoDoi() bool { return len(g.doi) > 0 }

// ThoiDiemMat – lúc mất tham chiếu gần nhất (ok = false nếu chưa từng mất).
func (g *GiaiMaVideo) ThoiDiemMat() (float64, bool) { return g.LucMat, g.coLucMat }

// Them – trả về danh sách khung vừa giải được (DuLieu = ảnh rgb thô).
func (g *GiaiMaVideo) Them(k *KhungNen, bayGio float64) []KhungNen {
	var ra []KhungNen
	if k.Loai == LoaiI {
		if g.tc != nil && k.So <= g.tc.so {
			return ra // I cũ hơn tham chiếu hiện tại
		}
		anh, err := GiaiNen(k.DuLieu)
		if err != nil || len(anh) != k.W*k.H*3 {
			g.Loi++
			return ra
		}
		g.tc = &thamChieu{k.So, anh, k.W, k.H}
		g.MatThamChieu = false
		for s := range g.doi {
			if s <= k.So {
				delete(g.doi, s)
			}
		}
		g.SoI++
		ra = append(ra, KhungNen{LoaiI, k.So, k.Ts, k.W, k.H, anh})
		return g.giaiTiep(ra)
	}
	if g.tc == nil || g.MatThamChieu {
		g.BoP++ // chưa có / mất tham chiếu: chờ I
		return ra
	}
	if k.So <= g.tc.so {
		return ra
	}
	g.doi[k.So] = khungCho{k.Ts, k.W, k.H, k.DuLieu, bayGio}
	return g.giaiTiep(ra)
}

func (g *GiaiMaVideo) giaiTiep(ra []KhungNen) []KhungNen {
	for g.tc != nil {
		so := g.tc.so + 1
		k, ok := g.doi[so]
		if !ok {
			break
		}
		delete(g.doi, so)
		x, err := GiaiNen(k.duLieu)
		if err != nil || len(x) != len(g.tc.anh) || k.w != g.tc.w || k.h != g.tc.h {
			g.Loi++ // P hỏng: coi như thiếu khung này
			break
		}
		anh := XorKhung(x, g.tc.anh)
		g.tc = &thamChieu{so, anh, k.w, k.h}
		g.SoP++
		ra = append(ra, KhungNen{LoaiP, so, k.ts, k.w, k.h, anh})
	}
	return ra
}

// KiemTra – true nếu vừa mất tham chiếu (khung thiếu không tới sau ChoThamChieuS).
func (g *GiaiMaVideo) KiemTra(bayGio float64) bool {
	if len(g.doi) == 0 {
		return false
	}
	cu := math.Inf(1)
	for _, v := range g.doi {
		cu = math.Min(cu, v.luc)
	}
	if bayGio-cu < g.Cho {
		return false
	}
	g.MatThamChieu = true
	g.LucMat, g.coLucMat = bayGio, true
	g.DungHinh++
	g.BoP += len(g.doi)
	g.doi = map[int64]khungCho{}
	return true
}

// ================================================================ Ảnh RGB, phông 5×7, PNG

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
	'=': "00001F001F0000", ',': "000000000C0408", '+': "0004041F040400", '?': "0E110102040004",
	' ': "00000000000000",
}

type Mau [3]byte

// ToMau – tô hình chữ nhật (x, y, rw, rh) bằng màu m (cắt theo mép phải/dưới của ảnh).
func ToMau(anh []byte, w, x, y, rw, rh int, m Mau) {
	rw = max(0, min(rw, w-x))
	for yy := y; yy < y+rh; yy++ {
		dau := (yy*w + x) * 3
		if dau+rw*3 > len(anh) {
			break
		}
		for i := 0; i < rw; i++ {
			copy(anh[dau+3*i:], m[:])
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

// PhongTo – phóng to k lần bằng lặp điểm gần nhất.
func PhongTo(anh []byte, w, h, k int) []byte {
	if k == 1 {
		return append([]byte(nil), anh...)
	}
	ra := make([]byte, w*k*h*k*3)
	for y := 0; y < h*k; y++ {
		for x := 0; x < w*k; x++ {
			copy(ra[(y*w*k+x)*3:(y*w*k+x)*3+3], anh[((y/k)*w+x/k)*3:])
		}
	}
	return ra
}

// AnhNRGBA – RGB thô -> image.NRGBA (dùng cho PNG và giao diện).
func AnhNRGBA(anh []byte, w, h int) *image.NRGBA {
	img := image.NewNRGBA(image.Rect(0, 0, w, h))
	for i := 0; i < w*h && 3*i+2 < len(anh); i++ {
		copy(img.Pix[4*i:4*i+3], anh[3*i:3*i+3])
		img.Pix[4*i+3] = 255
	}
	return img
}

// GhiPNG – mã hóa bằng image/png của thư viện chuẩn (khối IHDR, IDAT nén zlib, IEND, CRC-32).
func GhiPNG(duongDan string, anh []byte, w, h int) error {
	f, err := os.Create(duongDan)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, AnhNRGBA(anh, w, h))
}

// AnhNhan – một ảnh RGB kèm nhãn để ghép.
type AnhNhan struct {
	W, H int
	Rgb  []byte
	Nhan string
}

// GhepAnh – ghép các ảnh thành một PNG: cot cột, nhãn dưới mỗi ảnh. Trả về kích thước ảnh ghép.
func GhepAnh(duongDan string, ds []AnhNhan, cot int) (int, int, error) {
	oW := 0
	for _, a := range ds {
		oW = max(oW, a.W)
	}
	oW += 16
	var caoHang []int
	for i := 0; i < len(ds); i += cot {
		c := 0
		for _, a := range ds[i:min(len(ds), i+cot)] {
			c = max(c, a.H)
		}
		caoHang = append(caoHang, c+30)
	}
	W, H := min(cot, len(ds))*oW+16, 8
	for _, c := range caoHang {
		H += c
	}
	anh := bytes.Repeat([]byte{0xFF}, W*H*3)
	for n, a := range ds {
		x0 := 16 + (n%cot)*oW
		y0 := 8
		for _, c := range caoHang[:n/cot] {
			y0 += c
		}
		for y := 0; y < a.H; y++ {
			copy(anh[((y0+y)*W+x0)*3:((y0+y)*W+x0+a.W)*3], a.Rgb[y*a.W*3:(y+1)*a.W*3])
		}
		VeChu(anh, W, H, x0, y0+a.H+8, catRune(a.Nhan, oW/12), 2, Mau{0, 0, 0})
	}
	return W, H, GhiPNG(duongDan, anh, W, H)
}

// catRune – n ký tự đầu (theo ký tự Unicode, như s[:n] của Python).
func catRune(s string, n int) string {
	r := []rune(s)
	if len(r) > n {
		r = r[:n]
	}
	return string(r)
}

// CatRune – bản công khai cho chương trình dòng lệnh và giao diện.
func CatRune(s string, n int) string { return catRune(s, n) }

// ================================================================ Nguồn giả lập "bảng clapper"

var bangMau = []Mau{{230, 120, 60}, {70, 170, 90}, {70, 120, 220}, {200, 80, 180}, {200, 190, 60}}

// NguonClapper – camera giả lập kiểu bảng clapper của nhà quay phim: nền tĩnh (sóng chéo + hạt nhiễu
// thưa như MR6.3), một ô vuông chạy ngang (chuyển động nhỏ), đồng hồ phương tiện T=12.345 và số khung.
// Ở mỗi giây tròn (ts % 1000 < 1000/fps) cả khung CHỚP TRẮNG (giữ mờ vân nền) – cùng lúc tiếng bíp
// của NguonAmClapper: nhìn hình, nghe tiếng là thấy lệch tiếng-hình.
type NguonClapper struct {
	ten          string
	Fps          int
	nen, nenChop []byte
}

func MoiClapper(ten string, ma, fps int) *NguonClapper {
	m := bangMau[(max(1, ma)-1)%len(bangMau)]
	r := MoiNgauNhien(uint32(ma)) // như random.Random(ma) của Python
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
	// Khung chớp gần như trắng nhưng còn vân nền: khung I rơi vào lúc chớp vẫn có kích thước như
	// khung I thường. Bảng tra 256 phần tử: v -> 255 − (255 − v)/4.
	var bang [256]byte
	for v := range bang {
		bang[v] = byte(255 - (255-v)/4)
	}
	chop := make([]byte, len(nen))
	for i, v := range nen {
		chop[i] = bang[v]
	}
	return &NguonClapper{ten: ten, Fps: fps, nen: nen, nenChop: chop}
}

// LayKhung – khung thứ so tại nhãn tsMs (ms phương tiện).
func (c *NguonClapper) LayKhung(so int, tsMs int64) []byte {
	chop := float64(tsMs%1000) < 1000/float64(max(1, c.Fps))
	x := 6 + int((tsMs/10)%(Rong-24)) // ô vuông chạy 100 điểm ảnh/giây
	var anh []byte
	var mauChu, nenChu Mau
	if chop { // chớp trắng: v -> 255 − (255 − v)/4
		anh = append([]byte(nil), c.nenChop...)
		mauChu, nenChu = Mau{0, 0, 0}, Mau{255, 255, 255}
	} else {
		anh = append([]byte(nil), c.nen...)
		mauChu, nenChu = Mau{255, 255, 255}, Mau{20, 20, 20}
		ToMau(anh, Rong, x, 52, 12, 12, Mau{255, 230, 0})
	}
	dongHo := fmt.Sprintf("T=%d.%03d", tsMs/1000, tsMs%1000)
	ToMau(anh, Rong, 4, 4, 6*2*len(dongHo)+4, 18, nenChu)
	VeChu(anh, Rong, Cao, 6, 6, dongHo, 2, mauChu)
	nhan := fmt.Sprintf("#%04d", so)
	ToMau(anh, Rong, 4, Cao-22, 6*2*len(nhan)+4, 18, nenChu)
	VeChu(anh, Rong, Cao, 6, Cao-20, nhan, 2, mauChu)
	ToMau(anh, Rong, Rong-30, Cao-15, 27, 11, Mau{200, 0, 0})
	VeChu(anh, Rong, Cao, Rong-28, Cao-13, "LIVE", 1, Mau{255, 255, 255})
	ten := catRune(c.ten, 3)
	VeChu(anh, Rong, Cao, Rong-6*2*utf8.RuneCountInString(ten)-4, 6, ten, 2, mauChu)
	return anh
}

// NguonAmClapper – micro giả lập: âm 440 Hz biên độ thấp liên tục + tiếng "bíp" 1000 Hz dài 100 ms ở
// mỗi giây tròn của đồng hồ phương tiện (mẫu thứ n: n % 16000 < 1600). Mẫu đầu tiên = lúc bấm Phát.
type NguonAmClapper struct {
	n            int
	w, a, wb, ab float64
}

func MoiAmClapper() *NguonAmClapper {
	return &NguonAmClapper{w: 2 * math.Pi * 440 / TanSoMau, a: 0.08 * 32767,
		wb: 2 * math.Pi * 1000 / TanSoMau, ab: 0.4 * 32767}
}

func (s *NguonAmClapper) LayKhung() []int16 {
	ra := make([]int16, MauMoiKhung)
	for i := range ra {
		n := s.n + i
		x := s.a * math.Sin(s.w*float64(n))
		if n%TanSoMau < TanSoMau/10 {
			x += s.ab * math.Sin(s.wb*float64(n))
		}
		ra[i] = int16(math.RoundToEven(x))
	}
	s.n += MauMoiKhung
	return ra
}

// ================================================================ Kênh điều khiển JSON theo dòng

// Tin – một thông điệp điều khiển (đối tượng JSON).
type Tin map[string]any

// Chuoi, So, Bool – đọc trường với giá trị mặc định khi thiếu hoặc sai kiểu.
func (t Tin) Chuoi(k string) string {
	s, _ := t[k].(string)
	return s
}

func (t Tin) So(k string, macDinh float64) float64 {
	if f, ok := t[k].(float64); ok {
		return f
	}
	return macDinh
}

func (t Tin) Bool(k string, macDinh bool) bool {
	switch v := t[k].(type) {
	case bool:
		return v
	case float64:
		return v != 0
	case string:
		return v != ""
	case nil:
		if _, co := t[k]; co {
			return false
		}
	}
	return macDinh
}

// DoiTuong – đối tượng JSON giữ nguyên thứ tự khóa như lúc tạo ("loai" luôn đứng đầu, như dict của
// Python); map của Go thì bị sắp xếp khóa theo chữ cái khi mã hóa.
type DoiTuong []any

// J – tạo đối tượng JSON từ cặp khóa, giá trị: J("loai", "chao", "ma", 1).
func J(kv ...any) DoiTuong { return DoiTuong(kv) }

func (d DoiTuong) MarshalJSON() ([]byte, error) {
	var buf bytes.Buffer
	buf.WriteByte('{')
	for i := 0; i+1 < len(d); i += 2 {
		if i > 0 {
			buf.WriteByte(',')
		}
		k, _ := json.Marshal(fmt.Sprint(d[i]))
		v, err := maHoa(d[i+1])
		if err != nil {
			return nil, err
		}
		buf.Write(k)
		buf.WriteByte(':')
		buf.Write(v)
	}
	buf.WriteByte('}')
	return buf.Bytes(), nil
}

// maHoa – json.Marshal nhưng không đổi < > & thành \u003c… (giữ chữ như Python).
func maHoa(v any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	return bytes.TrimRight(buf.Bytes(), "\n"), nil
}

// GuiJSON – mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n'.
func GuiJSON(c net.Conn, v any) error {
	b, err := maHoa(v)
	if err != nil {
		return err
	}
	_, err = c.Write(append(b, '\n'))
	return err
}

// SoChu – số trong JSON in như Python in số nguyên (160, không phải 160.0); thiếu -> "None".
func SoChu(v any) string {
	switch x := v.(type) {
	case float64:
		return strconv.FormatFloat(x, 'f', -1, 64)
	case nil:
		return "None"
	}
	return fmt.Sprint(v)
}

// ErrDong – dòng điều khiển quá dài, bị cắt hoặc không phải đối tượng JSON.
var ErrDong = errors.New("dòng điều khiển sai dạng")

// DocDong – đọc một dòng JSON; io.EOF khi kết nối đóng; dòng > 64 KB hoặc sai dạng -> ErrDong.
func DocDong(r *bufio.Reader) (Tin, error) {
	tin, _, err := DocDongTho(r)
	return tin, err
}

// DocDongTho – như DocDong, trả thêm nguyên văn dòng (kể cả '\n') để chuyển tiếp không đổi byte nào.
func DocDongTho(r *bufio.Reader) (Tin, []byte, error) {
	var dong []byte
	for {
		phan, err := r.ReadSlice('\n')
		dong = append(dong, phan...)
		if len(dong) > GioiHanDong {
			return nil, nil, ErrDong
		}
		if err == bufio.ErrBufferFull {
			continue
		}
		if err != nil {
			if len(dong) == 0 {
				return nil, nil, io.EOF
			}
			return nil, nil, ErrDong // bị cắt giữa dòng
		}
		break
	}
	var tin Tin
	if err := json.Unmarshal(dong, &tin); err != nil || tin == nil {
		return nil, nil, ErrDong
	}
	return tin, dong, nil
}

// TenHopLe – tên 1–16 ký tự, không có khoảng trắng đầu/cuối, chỉ ký tự in được.
func TenHopLe(ten string) bool {
	n := utf8.RuneCountInString(ten)
	if n < 1 || n > 16 || strings.TrimSpace(ten) != ten || !utf8.ValidString(ten) {
		return false
	}
	for _, c := range ten {
		if !unicode.IsPrint(c) {
			return false
		}
	}
	return true
}

// DocLenhStdin – đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu).
func DocLenhStdin(t0 float64, xuLy func(string)) {
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		dong := strings.TrimSpace(sc.Text())
		if strings.HasPrefix(dong, "@") {
			moc, con, _ := strings.Cut(dong, " ")
			if t, err := strconv.ParseFloat(moc[1:], 64); err == nil {
				ngu(t - (BayGio() - t0))
			}
			dong = strings.TrimSpace(con)
		}
		if dong != "" {
			xuLy(dong)
		}
	}
}

// ChuMs – số ms để in: "–" nếu chưa có (nil); dau = true in dấu +/−.
func ChuMs(x *float64, dau bool) string {
	if x == nil {
		return "–"
	}
	if dau {
		return fmt.Sprintf("%+.0f", *x)
	}
	return fmt.Sprintf("%.0f", *x)
}

// conTro – địa chỉ của một bản sao x (cho các số "có thể chưa có").
func conTro(x float64) *float64 { return &x }
