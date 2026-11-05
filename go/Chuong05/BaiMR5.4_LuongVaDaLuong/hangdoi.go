package main

// (d) Nhà sản xuất – người tiêu dùng trên một kết nối TCP.
//
//	goroutine "nhận"   : gửi "PHAT 9", đọc từng dòng "GOI k dữ-liệu", gửi vào kênh việc (bộ đệm 4 – đầy thì
//	                     phải CHỜ: áp lực ngược, không đọc thêm từ socket);
//	3 goroutine "xử-lý-k": nhận gói, xử lý 80 ms + tính tổng byte, gửi kết quả vào kênh kết quả;
//	goroutine "gửi"    : ghi "KQ k tổng" vào CÙNG kết nối (một goroutine đọc, một goroutine ghi: an toàn).
//
// Kết thúc êm bằng "viên thuốc độc": gói có k = 0. Trong Go, cách thường dùng hơn là close(kênh) – mọi
// goroutine đang `range` trên kênh đều thoát; ở đây giữ viên thuốc độc để giống các bản ngôn ngữ khác.
// (Cách khác: context.WithCancel khi cần dừng NGAY, bỏ việc dở.)
import (
	"bufio"
	"fmt"
	"net"
	"strconv"
	"strings"
	"sync"
	"time"
)

const nGoi, nTho, sucChua, msXuLy = 9, 3, 4, 80

type goi struct {
	k    int // k = 0: viên thuốc độc
	text string
	v    int
}

func runHangDoi(host, port string) {
	say("(d) Nhà sản xuất – người tiêu dùng: goroutine nhận đọc socket → kênh (tối đa %d) → %d goroutine xử lý → goroutine gửi", sucChua, nTho)
	conn, err := net.DialTimeout("tcp", net.JoinHostPort(host, port), 10*time.Second)
	if err != nil {
		say("Lỗi: %v", err)
		return
	}
	defer conn.Close()
	r := bufio.NewReader(conn)
	viec := make(chan goi, sucChua)
	ketQua := make(chan goi, nGoi+nTho)
	dem := make([]int, nTho+1)
	soLanCho := 0
	var wg sync.WaitGroup
	t := time.Now()

	wg.Add(1)
	go func(n *NhatKy) { // nhà sản xuất
		defer wg.Done()
		n.Log("kết nối dịch vụ, gửi PHAT %d", nGoi)
		fmt.Fprintf(conn, "PHAT %d\n", nGoi)
		for range nGoi {
			line, _ := r.ReadString('\n')
			p := strings.Fields(line)
			k, _ := strconv.Atoi(p[1])
			g := goi{k: k, text: p[2]}
			select {
			case viec <- g:
			default:
				soLanCho++
				n.Log("hàng đợi đầy (%d/%d) → chờ chỗ trống (áp lực ngược)", sucChua, sucChua)
				viec <- g // chặn tới khi một goroutine xử lý lấy bớt
			}
			n.Log("GOI %d → hàng đợi", k)
		}
		n.Log("hết dữ liệu → đặt %d viên thuốc độc (poison pill) vào hàng đợi", nTho)
		for range nTho {
			viec <- goi{}
		}
	}(nk("nhận"))

	for i := 1; i <= nTho; i++ {
		wg.Add(1)
		go func(i int, n *NhatKy) { // người tiêu dùng
			defer wg.Done()
			for g := range viec {
				if g.k == 0 {
					n.NguCanh = ""
					n.Log("gặp viên thuốc độc → dừng")
					ketQua <- goi{}
					return
				}
				n.NguCanh = fmt.Sprintf("yc=%d", g.k) // ngữ cảnh truyền tường minh qua *NhatKy
				time.Sleep(msXuLy * time.Millisecond)
				g.v = tongByte(g.text)
				n.Log("xử lý GOI %d: tổng byte = %d", g.k, g.v)
				dem[i]++ // mỗi goroutine chỉ ghi ô của mình → không cần khóa
				ketQua <- g
			}
		}(i, nk(fmt.Sprintf("xử-lý-%d", i)))
	}

	wg.Add(1)
	go func(n *NhatKy) { // goroutine gửi
		defer wg.Done()
		w := bufio.NewWriter(conn)
		for conLai := nTho; conLai > 0; {
			g := <-ketQua
			if g.k == 0 {
				conLai--
				continue
			}
			fmt.Fprintf(w, "KQ %d %d\n", g.k, g.v)
			w.Flush()
			n.Log("gửi KQ %d %d", g.k, g.v)
		}
		n.Log("cả %d goroutine xử lý đã dừng → hết việc gửi", nTho)
	}(nk("gửi"))

	wg.Wait()
	traLoi, _ := r.ReadString('\n')
	dt := time.Since(t).Seconds()
	nk("chính").Log("dịch vụ trả lời: %s", strings.TrimSpace(traLoi))
	say("")
	say("  %-12s%16s", "Goroutine", "Số gói đã xử lý")
	for i := 1; i <= nTho; i++ {
		say("  %-12s%16d", fmt.Sprintf("xử-lý-%d", i), dem[i])
	}
	say("  Goroutine nhận phải chờ vì hàng đợi đầy: %d lần; tổng %.2f s (1 goroutine xử lý ≈ %.2f s)",
		soLanCho, dt, float64(nGoi*msXuLy)/1000)
}
