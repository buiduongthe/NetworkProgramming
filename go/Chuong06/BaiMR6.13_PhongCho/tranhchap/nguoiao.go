package tranhchap

// NGHIỆP VỤ MR6.13 của bộ mô phỏng tải (khung ở mophong.go): nguCanh – phòng chờ bật hay tắt (đọc từ
// cap_nhat đầu tiên); nguoiAo – kịch bản của MỘT người (mua 1–4 vé, `lan` không dùng):
//   - phòng chờ bật:  xep_hang -> cho_luot (máy chủ giữ yêu cầu tới khi tới lượt) -> mua;
//   - phòng chờ tắt:  mua ngay; bị "qua_tai" thì bấm lại ngay (tối đa 3 lần) – như người dùng nhấn F5;
//   - 1/10 người là "TAB THỨ HAI" của người liền trước (u10 đóng vai u9…): cùng xếp hàng, cùng được lượt,
//     cùng bấm mua -> chế độ an toàn chỉ nhận một đơn, khong_khoa nhận cả hai (sai phạm "sai lượt").
//
// DoMua ghi thời gian từ lần bấm "mua" đầu tiên tới kết quả cuối của từng người (gồm cả các lần bấm lại) –
// SoSanhPhongCho dùng để so sánh bật/tắt phòng chờ. Khứ hồi của bộ mô phỏng gồm cả cho_luot – tức là cả
// thời gian đứng trong phòng chờ.

import (
	"fmt"
	"math/rand/v2"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// SoLanMacDinh – mỗi người một lần mua (1–4 vé).
const SoLanMacDinh = 1

// ThamSoDatLai – so sánh 4 chế độ: phòng chờ bật, cấp lượt 200 người/s, thùng 20 token.
var ThamSoDatLai = TS{{"phong_cho", int64(1)}, {"toc_do", 200.0}, {"dung_luong", int64(20)}}

// BamLai – bị quá tải / xung đột: bấm lại tối đa 3 lần.
const BamLai = 3

// doMua – kết quả cuối của từng người, thời gian mua (ms), số lần bị từ chối vì quá tải – an toàn đa luồng.
type doMua struct {
	mu     sync.Mutex
	tre    []float64
	kq     map[string]int64
	quaTai int64
}

var DoMua = &doMua{kq: map[string]int64{}}

func (d *doMua) datLai() {
	d.mu.Lock()
	d.tre, d.kq, d.quaTai = nil, map[string]int64{}, 0
	d.mu.Unlock()
}

func (d *doMua) ketThuc(ma string, treMs *float64) {
	d.mu.Lock()
	d.kq[ma]++
	if treMs != nil {
		d.tre = append(d.tre, *treMs)
	}
	d.mu.Unlock()
}

func (d *doMua) demQuaTai() {
	d.mu.Lock()
	d.quaTai++
	d.mu.Unlock()
}

// nguCanh – phòng chờ bật hay tắt (đọc từ cap_nhat đầu tiên).
type nguCanh struct{ phongCho bool }

func moiNguCanh(tt M) *nguCanh { b, _ := tt["phong_cho"].(bool); return &nguCanh{b} }

func (nc *nguCanh) capNhat(M) {}

func (nc *nguCanh) nhanKq(M) {}

type nguoiAo struct {
	nc       *nguCanh
	ai       string
	soVe     int64
	dang     string
	bam      int
	tMua     time.Time
	daBamMua bool
}

func moiNguoiAo(ten string, nc *nguCanh, _ int) *nguoiAo {
	na := &nguoiAo{nc: nc, ai: ten, soVe: int64(1 + rand.IntN(4))}
	if i, err := strconv.Atoi(ten[1:]); err == nil && i%10 == 0 { // tab thứ hai của người u(i-1)
		na.ai = fmt.Sprintf("u%d", i-1)
	}
	return na
}

func (na *nguoiAo) gui(loai string, truong ...KV) *YeuCau {
	na.dang = loai
	return &YeuCau{loai, append(TS{{"nguoi", na.ai}}, truong...)}
}

func (na *nguoiAo) mua() *YeuCau {
	if !na.daBamMua {
		na.tMua, na.daBamMua = time.Now(), true
	}
	return na.gui("mua", KV{"so_ve", na.soVe})
}

func (na *nguoiAo) yeuCauDau() *YeuCau {
	if !na.nc.phongCho {
		return na.mua()
	}
	return na.gui("xep_hang")
}

func (na *nguoiAo) sauKq(tin M) *YeuCau {
	ma, la := tin["ma_loi"].(string)
	if !la {
		ma = "None"
	}
	ok, _ := tin["ok"].(bool)
	switch na.dang {
	case "xep_hang":
		if ok {
			return na.gui("cho_luot")
		}
		if ma == "xung_dot" && na.bam < BamLai {
			na.bam++
			return na.gui("xep_hang")
		}
		DoMua.ketThuc(ma, nil)
		return nil
	case "cho_luot":
		if ok {
			return na.mua()
		}
		DoMua.ketThuc(ma, nil) // hết vé khi đang đứng trong hàng
		return nil
	}
	if ma == "qua_tai" {
		DoMua.demQuaTai()
	}
	if (ma == "qua_tai" || ma == "xung_dot") && na.bam < BamLai {
		na.bam++
		return na.mua()
	}
	t := float64(time.Since(na.tMua).Nanoseconds()) / 1e6
	if ok {
		ma = "ok"
	}
	DoMua.ketThuc(ma, &t)
	return nil
}

// ------------------------------------------------------------ so sánh bật/tắt phòng chờ

// HangPhongCho – một dòng bảng so sánh bật/tắt phòng chờ.
type HangPhongCho struct {
	PhongCho                       string
	MuaDuoc, HetVe, QuaTai, BoCuoc int64
	P50Ms, P99Ms                   *float64
	Giay                           float64
	SaiPham                        int64
}

const TieuDePhongCho = "  Phòng chờ Mua được Hết vé Quá tải Bỏ cuộc p50 ms p99 ms"

// BangPhongCho – các dòng bảng so sánh bật/tắt phòng chờ (≤ 64 ký tự). Mua được, Hết vé, Bỏ cuộc: số
// NGƯỜI (kết quả cuối); Quá tải: số LẦN bị từ chối "qua_tai"; p50/p99: thời gian mua của từng người.
func BangPhongCho(hang []HangPhongCho) []string {
	ds := []string{TieuDePhongCho}
	var tg []string
	for _, h := range hang {
		ds = append(ds, fmt.Sprintf("  %-9s%9d %6d %7d %7d %6s %6s", h.PhongCho, h.MuaDuoc, h.HetVe, h.QuaTai, h.BoCuoc,
			fmtMs(h.P50Ms), fmtMs(h.P99Ms)))
		tg = append(tg, fmt.Sprintf("%s %.2f s", h.PhongCho, h.Giay))
	}
	if len(hang) > 0 {
		ds = append(ds, "  Thời gian cả lượt: "+strings.Join(tg, ", "))
	}
	return ds
}

// SoSanhPhongCho – hai lượt ở cùng chế độ (mặc định khoa): phòng chờ TẮT rồi BẬT; đo thời gian mua của
// từng người.
func SoSanhPhongCho(host string, cong, n, k int, bao func(string), cheDo string) ([]HangPhongCho, error) {
	goc := ThamSoDatLai
	defer func() { ThamSoDatLai = goc }()
	var hang []HangPhongCho
	for _, bat := range []int64{0, 1} {
		ThamSoDatLai = append(TS(nil), goc...).Dat("phong_cho", bat)
		DoMua.datLai()
		t0 := time.Now()
		h, err := ChayLuot(host, cong, n, k, 1, bao, cheDo, true, nil)
		if err != nil {
			return hang, err
		}
		if h == nil {
			break
		}
		DoMua.mu.Lock()
		tre := append([]float64(nil), DoMua.tre...)
		kq, quaTai := DoMua.kq, DoMua.quaTai
		DoMua.mu.Unlock()
		sort.Float64s(tre)
		ten := "tắt"
		if bat == 1 {
			ten = "bật"
		}
		hang = append(hang, HangPhongCho{PhongCho: ten, MuaDuoc: kq["ok"], QuaTai: quaTai, HetVe: kq["het_ve"],
			BoCuoc: kq["qua_tai"], P50Ms: PhanVi(tre, 50), P99Ms: PhanVi(tre, 99), Giay: time.Since(t0).Seconds(),
			SaiPham: h.SaiPham})
	}
	return hang, nil
}
