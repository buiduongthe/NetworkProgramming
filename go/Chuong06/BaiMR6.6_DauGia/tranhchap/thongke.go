package tranhchap

// Đo đạc: phân vị theo hạng gần nhất, số liệu máy chủ từ lần đặt lại gần nhất (khung chung MR6.6–MR6.14).

import (
	"math"
	"sort"
	"sync"
	"time"
)

var batDauTienTrinh = time.Now()

// GioDonDieu – giây kể từ khi chương trình chạy theo đồng hồ đơn điệu (time.monotonic của Python):
// hạn phiên, hạn giữ chỗ… tính theo đồng hồ này, không theo giờ hệ thống.
func GioDonDieu() float64 { return time.Since(batDauTienTrinh).Seconds() }

// PhanVi – phân vị p (0–100) theo HẠNG GẦN NHẤT: phần tử thứ ⌈p/100 · n⌉ (đếm từ 1) của dãy đã sắp
// xếp. Ví dụ n = 1000: p99 là phần tử thứ 990. Dãy rỗng -> nil.
func PhanVi(ds []float64, p float64) *float64 {
	if len(ds) == 0 {
		return nil
	}
	i := int(math.Ceil(p / 100 * float64(len(ds))))
	if i < 1 {
		i = 1
	}
	v := ds[i-1]
	return &v
}

// DemThongKe – tên các bộ đếm (thứ tự như bản Python).
var DemThongKe = []string{"so_yc", "thanh_cong", "tu_choi", "xung_dot", "thu_lai", "het_lan_thu", "khoa_cho_lau",
	"hang_doi_max"}

// ThongKe – số liệu của máy chủ từ lần đặt lại gần nhất; mọi hàm an toàn đa luồng (một khóa nhỏ riêng).
type ThongKe struct {
	mu           sync.Mutex
	dem          map[string]int64
	maLoi        map[string]int64 // lý do từ chối -> số lần
	treUs        []float64        // độ trễ xử lý từng yêu cầu (µs)
	tDau, tCuoi  float64
	coDau        bool
	giay, demGiy int64
	ycS          int64
}

func MoiThongKe() *ThongKe {
	t := &ThongKe{}
	t.DatLai()
	return t
}

func (t *ThongKe) DatLai() {
	t.mu.Lock()
	defer t.mu.Unlock()
	t.dem = map[string]int64{}
	for _, k := range DemThongKe {
		t.dem[k] = 0
	}
	t.maLoi = map[string]int64{}
	t.treUs = nil
	t.tDau, t.tCuoi, t.coDau = 0, 0, false
	t.giay, t.demGiy, t.ycS = int64(GioDonDieu()), 0, 0
}

func (t *ThongKe) Cong(ten string, n int64) {
	t.mu.Lock()
	t.dem[ten] += n
	t.mu.Unlock()
}

func (t *ThongKe) DatMax(ten string, x int64) {
	t.mu.Lock()
	if x > t.dem[ten] {
		t.dem[ten] = x
	}
	t.mu.Unlock()
}

func (t *ThongKe) BatDauYC() {
	t.mu.Lock()
	t.dem["so_yc"]++
	if !t.coDau {
		t.tDau, t.coDau = GioDonDieu(), true
	}
	t.mu.Unlock()
}

func (t *ThongKe) XongYC(kq M, treUs float64) {
	bayGio := GioDonDieu()
	t.mu.Lock()
	defer t.mu.Unlock()
	if ok, _ := kq["ok"].(bool); ok {
		t.dem["thanh_cong"]++
	} else {
		t.dem["tu_choi"]++
		ma, co := kq["ma_loi"].(string)
		if !co {
			ma = "khac"
		}
		t.maLoi[ma]++
	}
	if len(t.treUs) < GiuDoTre {
		t.treUs = append(t.treUs, treUs)
	}
	t.tCuoi = bayGio
	t.sangGiay(bayGio)
	t.demGiy++
}

// sangGiay – yc_s = số yêu cầu xong trong giây (đồng hồ) liền trước.
func (t *ThongKe) sangGiay(bayGio float64) {
	g := int64(bayGio)
	if g != t.giay {
		if g == t.giay+1 {
			t.ycS = t.demGiy
		} else {
			t.ycS = 0
		}
		t.giay, t.demGiy = g, 0
	}
}

// AnhChup – các bộ đếm, ma_loi, yc_s, thong_luong, p50_us/p95_us/p99_us (nil nếu chưa có mẫu).
func (t *ThongKe) AnhChup() M {
	t.mu.Lock()
	t.sangGiay(GioDonDieu())
	d := M{}
	for k, v := range t.dem {
		d[k] = v
	}
	ml := M{}
	for k, v := range t.maLoi {
		ml[k] = v
	}
	d["ma_loi"] = ml
	d["yc_s"] = t.ycS
	tre := append([]float64(nil), t.treUs...)
	xong := t.dem["thanh_cong"] + t.dem["tu_choi"]
	khoang := 0.0
	if t.coDau && t.tCuoi != 0 {
		khoang = t.tCuoi - t.tDau
	}
	t.mu.Unlock()
	if khoang > 0 {
		d["thong_luong"] = LamTron(float64(xong) / khoang)
	} else {
		d["thong_luong"] = int64(0)
	}
	sort.Float64s(tre)
	for _, p := range []struct {
		k string
		p float64
	}{{"p50_us", 50}, {"p95_us", 95}, {"p99_us", 99}} {
		if v := PhanVi(tre, p.p); v != nil {
			d[p.k] = LamTron(*v)
		} else {
			d[p.k] = nil
		}
	}
	return d
}
