// Bài mở rộng MR1.2 – Mô phỏng giao thức và kiểu truyền: TCP, UDP, unicast, broadcast, multicast, anycast.
//
// Bài mở rộng, không có trong giáo trình; minh họa trực quan Chương 1 (mô hình TCP/IP, tầng giao vận,
// các kiểu địa chỉ) trước khi lập trình socket thật ở các chương sau. Toàn bộ chạy trong bộ mô phỏng
// sự kiện rời rạc (mang.go): không mở socket, không cần quyền quản trị, kết quả tất định theo seed.
//
//	out/app <kịch_bản> [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]
//	kịch_bản: tcp | udp | unicast | broadcast | multicast | anycast            (giao thức, kiểu truyền)
//	          dong_bo | da_luong | nhom_luong | bat_dong_bo | phoi_hop   (bên trong máy chủ, maychu.go)
//	          so_sanh (bảng so sánh 5 mô hình máy chủ) | tat_ca
//	--seed  hạt giống bộ sinh ngẫu nhiên (mặc định 7)
//	--loss  xác suất mất gói trên liên kết R2–R3, 0..1 (mặc định: 0; UDP: 0.2)
//	--doan  số đoạn dữ liệu TCP (mặc định 40)
//	--mat   các đoạn TCP bị ép mất; lặp số đoạn để mất nhiều lần (mặc định 20,20)
//	--html  ghi tệp HTML hoạt hình tự chứa (JavaScript thuần, không cần mạng)
package main

import (
	_ "embed"
	"encoding/json"
	"flag"
	"fmt"
	"html/template"
	"os"
	"strconv"
	"strings"
)

//go:embed khung_hoat_hinh.html
var khungHTML string

var order = append([]string{"tcp", "udp", "unicast", "broadcast", "multicast", "anycast"}, mcOrder...)
var titles = map[string]string{
	"tcp":       "TCP – bắt tay, cửa sổ trượt, cwnd, mất gói, đóng FIN",
	"udp":       "UDP – datagram, mất gói, sai thứ tự, không gửi lại",
	"unicast":   "Unicast – một nguồn tới một đích",
	"broadcast": "Broadcast – gửi tràn trong LAN, dừng ở router",
	"multicast": "Multicast – IGMP, chỉ giao cho thành viên nhóm",
	"anycast":   "Anycast – máy phục vụ gần nhất, dự phòng khi hỏng",
}

func init() { // gộp tiêu đề các kịch bản bên trong máy chủ (maychu.go)
	for k, v := range mcTitles {
		titles[k] = v
	}
}

type options struct {
	seed, doan int
	loss       float64
	mat, html  string
}

type runData struct {
	ID      string     `json:"id"`
	Title   string     `json:"title"`
	Hops    [][]any    `json:"hops"`
	Notes   [][]any    `json:"notes"`
	Changes [][]any    `json:"changes"`
	Cwnd    [][2]int64 `json:"cwnd,omitempty"`
	Lanes   [][]any    `json:"lanes,omitempty"` // các luồng của máy chủ: [tên, [[t0, t1, trạng thái, mã], …], [việc ngắn]]
	IO      [][]any    `json:"io,omitempty"`    // I/O bất đồng bộ đang chờ: [[t0, t1, mã], …]
	Beats   [][]int64  `json:"beats,omitempty"` // nhịp tim: [[dự kiến, thực tế], …]
}

func runOne(name string, o options) runData {
	fmt.Printf("=== Kịch bản %s (seed %d) ===\n", name, o.seed)
	fmt.Println(titles[name])
	n := NewNet(o.seed)
	lossSet := o.loss >= 0
	if lossSet {
		n.links[[2]string{"R2", "R3"}].Loss = int(o.loss*1000 + 0.5)
	}
	rd := runData{ID: name, Title: titles[name]}
	if mcTitles[name] != "" {
		chayMayChu(n, name, &rd)
		fmt.Println()
		rd.Hops, rd.Notes, rd.Changes = n.hops, n.notes, n.changes
		return rd
	}
	switch name {
	case "tcp":
		var drops []int
		var shown []string
		for _, x := range strings.Split(o.mat, ",") {
			if k, err := strconv.Atoi(strings.TrimSpace(x)); err == nil {
				drops = append(drops, k)
				shown = append(shown, "#"+strconv.Itoa(k))
			}
		}
		s := strings.Join(shown, ", ")
		if s == "" {
			s = "không"
		}
		fmt.Printf("Truyền %d đoạn × %d byte từ H1 tới H5; ép mất đoạn: %s\n", o.doan, MSS, s)
		sc := NewTcp(n, o.doan, drops)
		sc.Start()
		n.Run()
		sc.Report()
		for _, p := range sc.cwndLog {
			rd.Cwnd = append(rd.Cwnd, [2]int64{p.t, int64(p.v)})
		}
	case "udp":
		udp(n, lossSet)
	case "unicast":
		unicast(n)
	case "broadcast":
		broadcast(n)
	case "multicast":
		multicast(n)
	case "anycast":
		anycast(n)
	}
	fmt.Println()
	n.Summary()
	fmt.Println()
	rd.Hops, rd.Notes, rd.Changes = n.hops, n.notes, n.changes
	return rd
}

// writeHTML – chèn dữ liệu JSON vào khung hoạt hình bằng html/template (kiểu template.JS: chèn
// nguyên văn vào <script>; json.Marshal đã thoát các ký tự <, >, & nên an toàn).
func writeHTML(path string, runs []runData) error {
	type nodeJS struct {
		Name string `json:"name"`
		Kind string `json:"kind"`
		IP   string `json:"ip"`
		X    int    `json:"x"`
		Y    int    `json:"y"`
	}
	type linkJS struct {
		A    string `json:"a"`
		B    string `json:"b"`
		Cost int    `json:"cost"`
	}
	data := struct {
		Nodes []nodeJS  `json:"nodes"`
		Links []linkJS  `json:"links"`
		Runs  []runData `json:"runs"`
	}{Runs: runs}
	for _, d := range nodeDefs {
		data.Nodes = append(data.Nodes, nodeJS{d.name, d.kind, d.ip, d.x, d.y})
	}
	for _, d := range linkDefs {
		data.Links = append(data.Links, linkJS{d.a, d.b, d.cost})
	}
	js, err := json.Marshal(data)
	if err != nil {
		return err
	}
	tpl, err := template.New("khung").Parse(khungHTML)
	if err != nil {
		return err
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	if err := tpl.Execute(f, map[string]any{"Data": template.JS(js)}); err != nil {
		return err
	}
	fmt.Println("Đã ghi hoạt hình HTML:", path)
	return nil
}

func main() {
	if len(os.Args) < 2 || (titles[os.Args[1]] == "" && os.Args[1] != "so_sanh" && os.Args[1] != "tat_ca") {
		fmt.Println("Cách dùng: out/app <tcp|udp|unicast|broadcast|multicast|anycast")
		fmt.Println("                    |dong_bo|da_luong|nhom_luong|bat_dong_bo|phoi_hop|so_sanh|tat_ca>")
		fmt.Println("           [--seed N] [--loss P] [--doan N] [--mat 20,20] [--html tệp.html]")
		os.Exit(1)
	}
	var o options
	fs := flag.NewFlagSet("mo_phong", flag.ExitOnError)
	fs.IntVar(&o.seed, "seed", 7, "hạt giống bộ sinh ngẫu nhiên")
	fs.Float64Var(&o.loss, "loss", -1, "xác suất mất gói trên R2–R3 (0..1)")
	fs.IntVar(&o.doan, "doan", 40, "số đoạn dữ liệu TCP")
	fs.StringVar(&o.mat, "mat", "20,20", "các đoạn TCP bị ép mất")
	fs.StringVar(&o.html, "html", "", "tệp HTML hoạt hình")
	fs.Parse(os.Args[2:])
	if os.Args[1] == "so_sanh" {
		soSanh(o.seed)
		return
	}
	names := []string{os.Args[1]}
	if os.Args[1] == "tat_ca" {
		names = order
	}
	var runs []runData
	for _, name := range names {
		runs = append(runs, runOne(name, o))
	}
	if o.html != "" {
		if err := writeHTML(o.html, runs); err != nil {
			fmt.Println("Lỗi ghi HTML:", err)
			os.Exit(1)
		}
	}
}
