package tranhchap

// Bộ mô phỏng tải – KHUNG (giống nhau ở 8 bài tranh chấp): n người dùng ảo trên k kết nối, rào xuất
// phát, pipelining. Phần NGHIỆP VỤ ở nguoiao.go: nguCanh (điều một kết nối biết về trạng thái chung),
// nguoiAo (kịch bản của MỘT người dùng ảo), SoLanMacDinh, ThamSoDatLai.
//
// Một lượt: mở k kết nối (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi goroutine
// đứng chờ ở RÀO XUẤT PHÁT -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người mình chở trong MỘT
// lần Write; trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian tổng, yc/s, khứ
// hồi p50/p95/p99, rồi hỏi thong_ke của máy chủ (sai phạm theo sổ kiểm toán).

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"sync"
	"time"
)

// YeuCau – một yêu cầu của người dùng ảo (khung tự thêm ma_yc).
type YeuCau struct {
	Loai   string
	Truong TS
}

// raoXuatPhat – threading.Barrier(n) có hủy (abort) và hạn chờ.
type raoXuatPhat struct {
	mu   sync.Mutex
	n    int
	dem  int
	mo   chan struct{}
	vo   chan struct{}
	daVo bool
	daMo bool
}

func moiRao(n int) *raoXuatPhat {
	return &raoXuatPhat{n: n, mo: make(chan struct{}), vo: make(chan struct{})}
}

var loiRaoVo = errors.New("rào xuất phát bị hủy")

func (r *raoXuatPhat) cho(han time.Duration) error {
	r.mu.Lock()
	if r.daVo {
		r.mu.Unlock()
		return loiRaoVo
	}
	r.dem++
	if r.dem == r.n {
		r.daMo = true
		close(r.mo)
	}
	r.mu.Unlock()
	var het <-chan time.Time
	if han > 0 {
		t := time.NewTimer(han)
		defer t.Stop()
		het = t.C
	}
	select {
	case <-r.mo:
		return nil
	case <-r.vo:
		return loiRaoVo
	case <-het:
		r.huy()
		return errors.New("hết thời gian chờ")
	}
}

func (r *raoXuatPhat) huy() {
	r.mu.Lock()
	defer r.mu.Unlock()
	if !r.daVo && !r.daMo {
		r.daVo = true
		close(r.vo)
	}
}

// demThuTu – Counter giữ thứ tự gặp đầu tiên (most_common xếp theo số giảm dần, bằng nhau thì theo thứ tự gặp).
type demThuTu struct {
	khoa []string
	so   map[string]int64
}

func (d *demThuTu) cong(k string, n int64) {
	if d.so == nil {
		d.so = map[string]int64{}
	}
	if _, co := d.so[k]; !co {
		d.khoa = append(d.khoa, k)
	}
	d.so[k] += n
}

func (d *demThuTu) nhieuNhat() []KV {
	var ds []KV
	for _, k := range d.khoa {
		ds = append(ds, KV{k, d.so[k]})
	}
	sort.SliceStable(ds, func(a, b int) bool { return ds[a].V.(int64) > ds[b].V.(int64) })
	return ds
}

type ketQuaKetNoi struct {
	rtt        []float64
	ok, tuChoi int64
	lyDo       demThuTu
	loi        string
	tXong      time.Time
}

// chayKetNoi – goroutine của một kết nối: chào -> chờ rào -> gửi lô yêu cầu đầu -> đọc, gửi tiếp tới khi xong.
func chayKetNoi(host string, cong, i int, dsTen []string, lan int, rao *raoXuatPhat, kq *ketQuaKetNoi, wg *sync.WaitGroup) {
	kn := MoiKetNoiKhach()
	defer func() {
		if e := recover(); e != nil {
			kq.loi = fmt.Sprint(e)
			rao.huy()
		}
		kq.tXong = time.Now()
		kn.Dong()
		wg.Done()
	}()
	if _, err := kn.KetNoi(host, cong, fmt.Sprintf("mp%d", i+1), "nguoi_dung", len(dsTen), 5); err != nil {
		kq.loi = err.Error() // lỗi trước rào: phá rào để goroutine chính biết
		rao.huy()
		return
	}
	nc := moiNguCanh(kn.TrangThai())
	kn.KhiCapNhat = nc.capNhat
	con := len(dsTen) // số người chưa xong
	var nhan func(na *nguoiAo, tin M, rtt float64)
	gui := func(na *nguoiAo, yc *YeuCau) *YeuCauLo {
		if yc == nil {
			con--
			return nil
		}
		return &YeuCauLo{yc.Loai, yc.Truong, func(tin M, rtt float64) { nhan(na, tin, rtt) }}
	}
	nhan = func(na *nguoiAo, tin M, rtt float64) { // chạy trên chính goroutine này (trong VongDoc)
		kq.rtt = append(kq.rtt, rtt)
		if ok, _ := tin["ok"].(bool); ok {
			kq.ok++
		} else {
			kq.tuChoi++
			ma, la := tin["ma_loi"].(string)
			if !la {
				ma = "khac"
			}
			kq.lyDo.cong(ma, 1)
		}
		nc.nhanKq(tin)
		if tiep := gui(na, na.sauKq(tin)); tiep != nil {
			_, _ = kn.GuiYC(tiep.Loai, tiep.KhiKq, tiep.Truong)
		}
	}
	dsNguoi := make([]*nguoiAo, len(dsTen))
	for j, ten := range dsTen {
		dsNguoi[j] = moiNguoiAo(ten, nc, lan)
	}
	if err := rao.cho(0); err != nil { // RÀO XUẤT PHÁT: mọi kết nối sẵn sàng mới bắn
		kq.loi = err.Error()
		return
	}
	var lo []YeuCauLo
	for _, na := range dsNguoi {
		if x := gui(na, na.yeuCauDau()); x != nil {
			lo = append(lo, *x)
		}
	}
	if len(lo) > 0 {
		if err := kn.GuiLo(lo); err != nil {
			kq.loi = err.Error()
			return
		}
	}
	kn.VongDoc(func() bool { return con == 0 })
	if con != 0 {
		kq.loi = "máy chủ đóng kết nối giữa chừng"
	}
}

func fmtMs(v *float64) string {
	if v == nil {
		return "–"
	}
	return fmt.Sprintf("%.1f", *v)
}

// ChayLuot – một lượt mô phỏng; trả về một hàng của bảng so sánh, hoặc nil nếu không mở được đủ kết nối.
func ChayLuot(host string, cong, n, k, lan int, bao func(string), cheDo string, datLai bool, treMs *float64) (*HangSoSanh, error) {
	k = max(1, min(k, n))
	qt, err := MoiQuanTri(host, cong, "mo_phong")
	if err != nil {
		return nil, err
	}
	defer qt.Dong()
	if treMs != nil {
		if err := qt.Tre(*treMs); err != nil {
			return nil, err
		}
	}
	if datLai { // đặt lại trước (máy chủ in tổng kết lượt trước đúng chế độ)
		if err := qt.DatLai(ThamSoDatLai); err != nil {
			return nil, err
		}
	}
	if cheDo != "" {
		if err := qt.CheDo(cheDo); err != nil {
			return nil, err
		}
	}
	tk, err := qt.ThongKe()
	if err != nil {
		return nil, err
	}
	cheDo = Chuoi(tk["che_do"])
	bao(fmt.Sprintf("=== %s: %d người × %d lần, %d kết nối ===", cheDo, n, lan, k))
	nhom := make([][]string, k)
	for i := 0; i < n; i++ {
		nhom[i%k] = append(nhom[i%k], fmt.Sprintf("u%d", i+1))
	}
	rao := moiRao(k + 1)
	ketQua := make([]*ketQuaKetNoi, k)
	var wg sync.WaitGroup
	for i := range k {
		ketQua[i] = &ketQuaKetNoi{}
		wg.Add(1)
		go chayKetNoi(host, cong, i, nhom[i], lan, rao, ketQua[i], &wg)
	}
	if err := rao.cho(60 * time.Second); err != nil {
		loi := "hết thời gian chờ"
		for _, r := range ketQua {
			if r.loi != "" && r.loi != loiRaoVo.Error() {
				loi = r.loi
				break
			}
		}
		bao(fmt.Sprintf("Không mở được đủ %d kết nối: %s", k, loi))
		return nil, nil
	}
	t0 := time.Now()
	bao(fmt.Sprintf("Rào xuất phát: %d/%d kết nối sẵn sàng -> bắn", k, k))
	wg.Wait()
	t1 := t0
	var loi []string
	var rtt []float64
	var ok, tuChoi int64
	var lyDo demThuTu
	for _, r := range ketQua {
		if r.tXong.After(t1) {
			t1 = r.tXong
		}
		if r.loi != "" {
			loi = append(loi, r.loi)
		}
		rtt = append(rtt, r.rtt...)
		ok += r.ok
		tuChoi += r.tuChoi
		for _, x := range r.lyDo.khoa {
			lyDo.cong(x, r.lyDo.so[x])
		}
	}
	if len(loi) > 0 {
		bao(fmt.Sprintf("%d kết nối lỗi: %s", len(loi), loi[0]))
	}
	sort.Float64s(rtt)
	tong := ok + tuChoi
	giay := max(t1.Sub(t0).Seconds(), 1e-6)
	ycS := LamTron(float64(tong) / giay)
	p50, p95, p99 := PhanVi(rtt, 50), PhanVi(rtt, 95), PhanVi(rtt, 99)
	bao(fmt.Sprintf("%d yc xong trong %.2f s = %d yc/s", tong, giay, ycS))
	bao(fmt.Sprintf("Khứ hồi ms: p50 %s p95 %s p99 %s", fmtMs(p50), fmtMs(p95), fmtMs(p99)))
	bao(fmt.Sprintf("Thành công %d, từ chối %d", ok, tuChoi))
	if ds := lyDo.nhieuNhat(); len(ds) > 0 {
		var p []string
		for _, kv := range ds {
			p = append(p, fmt.Sprintf("%s %d", kv.K, kv.V))
		}
		bao("  lý do: " + strings.Join(p, ", "))
	}
	tk, err = qt.ThongKe()
	if err != nil {
		return nil, err
	}
	bao(fmt.Sprintf("Máy chủ: xung đột %d, thử lại %d, hàng đợi max %d", SoNguyen(tk["xung_dot"]),
		SoNguyen(tk["thu_lai"]), SoNguyen(tk["hang_doi_max"])))
	bao(fmt.Sprintf("Máy chủ xử lý µs: p50 %s p95 %s p99 %s", SoUs(tk["p50_us"]), SoUs(tk["p95_us"]), SoUs(tk["p99_us"])))
	bao(DongSaiPham(tk))
	viDu := DSChuoi(tk["vi_du_sai_pham"])
	for j, v := range viDu {
		if j < 2 {
			bao("  ví dụ: " + v)
		}
	}
	bao(Chuoi(tk["tom_tat"]))
	return &HangSoSanh{CheDo: cheDo, ThanhCong: ok, SaiPham: SoNguyen(tk["sai_pham_tong"]),
		XungDot: SoNguyen(tk["xung_dot"]), YcS: ycS, P99Ms: p99, TuChoi: tuChoi, TomTat: Chuoi(tk["tom_tat"])}, nil
}

// SoSanh – chạy lần lượt 4 chế độ (đặt lại trạng thái trước mỗi lần); trả lại chế độ cũ.
func SoSanh(host string, cong, n, k, lan int, bao func(string), treMs *float64) ([]HangSoSanh, error) {
	qt, err := MoiQuanTri(host, cong, "so_sanh")
	if err != nil {
		return nil, err
	}
	defer qt.Dong()
	tk, err := qt.ThongKe()
	if err != nil {
		return nil, err
	}
	goc := Chuoi(tk["che_do"])
	var hang []HangSoSanh
	for _, cd := range CheDoDS {
		h, err := ChayLuot(host, cong, n, k, lan, bao, cd, true, treMs)
		if err != nil {
			return hang, err
		}
		if h == nil {
			break
		}
		hang = append(hang, *h)
	}
	return hang, qt.CheDo(goc)
}
