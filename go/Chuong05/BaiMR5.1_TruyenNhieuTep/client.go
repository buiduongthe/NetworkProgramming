// Client truyền nhiều tệp đồng thời: điều khiển start/pause/resume/stop, giới hạn tốc độ theo
// phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256. Mỗi lượt là một goroutine.
package main

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"sync"
	"time"
)

const (
	ttChay    = "chạy"
	ttTamDung = "tạm dừng"
	ttDung    = "dừng"
	ttXong    = "xong"
	ttLoi     = "lỗi"
)

type LuotTruyen struct {
	ten, huong, cucBo string
	mu                sync.Mutex
	trangThai         string
	tong, xongByte    int64
	shaServer         string
	shaOK, daKiem     bool
	t0                time.Time
	thoiGian          float64
	bucket            *ThungToken
}

func (t *LuotTruyen) tt() string     { t.mu.Lock(); defer t.mu.Unlock(); return t.trangThai }
func (t *LuotTruyen) setTT(s string) { t.mu.Lock(); t.trangThai = s; t.mu.Unlock() }
func (t *LuotTruyen) them(n int)     { t.mu.Lock(); t.xongByte += int64(n); t.mu.Unlock() }
func (t *LuotTruyen) daLam() int64   { t.mu.Lock(); defer t.mu.Unlock(); return t.xongByte }
func (t *LuotTruyen) dungKhi() bool  { s := t.tt(); return s == ttTamDung || s == ttDung }

type QuanLy struct {
	host           string
	port           int
	thuMuc, khoSrv string
	session        *ThungToken
	mu             sync.Mutex
	luot           map[string]*LuotTruyen
}

func newQuanLy(host string, port int, thuMuc, khoSrv string) *QuanLy {
	os.MkdirAll(thuMuc, 0o755)
	return &QuanLy{host: host, port: port, thuMuc: thuMuc, khoSrv: khoSrv,
		session: newThung(khongGioiHan), luot: map[string]*LuotTruyen{}}
}

func (q *QuanLy) datTocDoPhien(rate float64) {
	q.session.datRate(rate)
	fmt.Printf("[client] Đặt tốc độ phiên = %s/s\n", dinhDangByte(int64(rate)))
}

func (q *QuanLy) layLuot(ten string) *LuotTruyen {
	q.mu.Lock()
	defer q.mu.Unlock()
	return q.luot[ten]
}

func (q *QuanLy) batDau(ten, huong string, rateTep float64) {
	if t := q.layLuot(ten); t != nil && t.tt() == ttChay {
		fmt.Printf("[client] %s đang chạy\n", ten)
		return
	}
	t := &LuotTruyen{ten: ten, huong: huong, cucBo: filepath.Join(q.thuMuc, ten),
		trangThai: ttChay, bucket: newThung(rateTep), t0: time.Now()}
	q.mu.Lock()
	q.luot[ten] = t
	q.mu.Unlock()
	if huong == "download" {
		go q.taiXuong(t)
	} else {
		go q.taiLen(t)
	}
	gh := ""
	if rateTep < khongGioiHan {
		gh = fmt.Sprintf(", giới hạn tệp %s/s", dinhDangByte(int64(rateTep)))
	}
	fmt.Printf("[client] BẮT ĐẦU %s %s%s\n", huong, ten, gh)
}

func (q *QuanLy) tamDung(ten string) {
	if t := q.layLuot(ten); t != nil && t.tt() == ttChay {
		t.setTT(ttTamDung)
		fmt.Printf("[client] TẠM DỪNG %s (giữ .part)\n", ten)
	}
}

func (q *QuanLy) taiTiep(ten string) {
	if t := q.layLuot(ten); t != nil && t.tt() == ttTamDung {
		fmt.Printf("[client] TẢI TIẾP %s\n", ten)
		q.batDau(ten, t.huong, t.bucket.rate)
	}
}

func (q *QuanLy) dungHan(ten string) {
	if t := q.layLuot(ten); t != nil && (t.tt() == ttChay || t.tt() == ttTamDung) {
		t.setTT(ttDung)
		fmt.Printf("[client] DỪNG %s (xóa .part)\n", ten)
	}
}

func (q *QuanLy) trangThai() {
	fmt.Println("[client] --- Trạng thái ---")
	q.mu.Lock()
	defer q.mu.Unlock()
	for ten, t := range q.luot {
		pct := 0.0
		if t.tong > 0 {
			pct = 100 * float64(t.xongByte) / float64(t.tong)
		}
		fmt.Printf("[client]   %-12s %-8s %-8s %s %5.1f%%  %s/%s\n",
			ten, t.huong, t.trangThai, thanhTienDo(pct), pct,
			dinhDangByte(t.xongByte), dinhDangByte(t.tong))
	}
}

func kichThuoc(path string) int64 {
	if info, err := os.Stat(path); err == nil {
		return info.Size()
	}
	return 0
}

func (q *QuanLy) taiXuong(t *LuotTruyen) {
	part := t.cucBo + ".part"
	off := kichThuoc(part)
	conn, err := net.Dial("tcp", fmt.Sprintf("%s:%d", q.host, q.port))
	if err != nil {
		if !t.dungKhi() {
			t.setTT(ttLoi)
			fmt.Printf("[client] %s: lỗi kết nối %v\n", t.ten, err)
		}
		return
	}
	guiKhung(conn, Khung{Op: "download", Name: t.ten, Offset: off})
	resp, err := nhanKhung(conn)
	if err != nil || !resp.OK {
		conn.Close()
		t.setTT(ttLoi)
		fmt.Printf("[client] %s: lỗi %s\n", t.ten, resp.Loi)
		return
	}
	t.mu.Lock()
	t.tong, t.shaServer, t.xongByte = resp.Size, resp.Sha256, off
	t.mu.Unlock()
	if off >= t.tong {
		conn.Close()
		q.hoanTatTaiXuong(t, part)
		return
	}
	flag := os.O_RDWR | os.O_CREATE
	f, _ := os.OpenFile(part, flag, 0o644)
	f.Seek(off, 0)
	buf := make([]byte, chunk)
	for t.daLam() < t.tong {
		s := t.tt()
		if s == ttDung {
			f.Close()
			conn.Close()
			os.Remove(part)
			return
		}
		if s == ttTamDung {
			f.Close()
			conn.Close()
			return
		}
		m, rerr := conn.Read(buf)
		if m > 0 {
			f.Write(buf[:m])
			t.them(m)
			q.session.tieuThu(float64(m), t.dungKhi)
			t.bucket.tieuThu(float64(m), t.dungKhi)
		}
		if rerr != nil {
			break
		}
	}
	f.Close()
	conn.Close()
	if t.daLam() >= t.tong {
		q.hoanTatTaiXuong(t, part)
	}
}

func (q *QuanLy) hoanTatTaiXuong(t *LuotTruyen, part string) {
	shaOK := sha256Tep(part) == t.shaServer
	os.Rename(part, t.cucBo)
	t.mu.Lock()
	t.shaOK, t.daKiem = shaOK, true
	t.thoiGian = time.Since(t.t0).Seconds()
	t.trangThai = ttXong
	tong, tg := t.tong, t.thoiGian
	t.mu.Unlock()
	fmt.Printf("[client] XONG download %s: %s trong %.1fs (%s/s), SHA-256 %s\n",
		t.ten, dinhDangByte(tong), tg, dinhDangByte(int64(float64(tong)/maxF(tg, 0.001))), khopSai(shaOK))
}

func (q *QuanLy) taiLen(t *LuotTruyen) {
	total := kichThuoc(t.cucBo)
	partSrv := filepath.Join(q.khoSrv, t.ten+".part")
	off := kichThuoc(partSrv)
	sha := sha256Tep(t.cucBo)
	conn, err := net.Dial("tcp", fmt.Sprintf("%s:%d", q.host, q.port))
	if err != nil {
		t.setTT(ttLoi)
		return
	}
	guiKhung(conn, Khung{Op: "upload", Name: t.ten, Offset: off, Size: total, Sha256: sha})
	resp, err := nhanKhung(conn)
	if err != nil || !resp.OK {
		conn.Close()
		t.setTT(ttLoi)
		return
	}
	t.mu.Lock()
	t.tong, t.xongByte = total, off
	t.mu.Unlock()
	f, _ := os.Open(t.cucBo)
	f.Seek(off, 0)
	buf := make([]byte, chunk)
	for t.daLam() < total {
		if t.dungKhi() {
			f.Close()
			conn.Close()
			return
		}
		m, rerr := f.Read(buf)
		if m > 0 {
			q.session.tieuThu(float64(m), t.dungKhi)
			t.bucket.tieuThu(float64(m), t.dungKhi)
			conn.Write(buf[:m])
			t.them(m)
		}
		if rerr != nil {
			break
		}
	}
	f.Close()
	ket, _ := nhanKhung(conn)
	conn.Close()
	t.mu.Lock()
	t.thoiGian = time.Since(t.t0).Seconds()
	t.shaOK = ket.Sha256OK
	t.trangThai = ttXong
	tg := t.thoiGian
	t.mu.Unlock()
	fmt.Printf("[client] XONG upload %s: %s trong %.1fs (%s/s), server kiểm SHA-256 %s\n",
		t.ten, dinhDangByte(total), tg, dinhDangByte(int64(float64(total)/maxF(tg, 0.001))), khopSai(ket.Sha256OK))
}

func khopSai(ok bool) string {
	if ok {
		return "KHỚP"
	}
	return "SAI"
}

func maxF(a, b float64) float64 {
	if a > b {
		return a
	}
	return b
}
