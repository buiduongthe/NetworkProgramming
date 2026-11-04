// Bài mở rộng MR1.1 – Quản lý địa chỉ IP (IPAM) bằng gói net/netip.
//
// Bài mở rộng, không có trong giáo trình; nối tiếp Mục 1.2.3 (địa chỉ IP, tiền tố mạng)
// và Bài 1.1. Chương trình đọc một kế hoạch địa chỉ (tệp JSON) và:
//  1. Chia khối địa chỉ thành các mạng con có kích thước khác nhau (VLSM):
//     yêu cầu lớn được cấp trước để các khối luôn thẳng hàng (aligned).
//  2. Liệt kê phần địa chỉ còn trống dưới dạng các khối CIDR.
//  3. Kiểm tra các tiền tố đề xuất: sai cú pháp, có bit máy, nằm ngoài khối, chồng lấn.
//  4. Cấp phát địa chỉ cho thiết bị trong một mạng con (bỏ qua địa chỉ cổng mặc định).
//
// Chỉ xử lý IPv4 cho gọn; netip dùng được cho cả IPv6.
//
// Chạy:  go run . [tệp_kế_hoạch.json ...]      (mặc định ke_hoach.json)
package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"net/netip"
	"os"
	"sort"
)

// Plan – cấu trúc tệp kế hoạch (tên trường JSON giống bản Python).
type Plan struct {
	Block    string    `json:"khoi_dia_chi"`
	Requests []Request `json:"yeu_cau"`
	Checks   []string  `json:"kiem_tra"`
	Assign   *struct {
		Net     string   `json:"mang"`
		Devices []string `json:"thiet_bi"`
	} `json:"cap_phat"`
}

// Request – một yêu cầu mạng con.
type Request struct {
	Name  string `json:"ten"`
	Hosts int    `json:"so_may"`
}

// Alloc – một mạng con đã cấp.
type Alloc struct {
	Name  string
	Hosts int
	Net   netip.Prefix
}

// toU32 / fromU32 – đổi địa chỉ IPv4 sang số nguyên 32 bit để làm phép tính khối.
func toU32(a netip.Addr) uint32 { b := a.As4(); return binary.BigEndian.Uint32(b[:]) }
func fromU32(v uint32) netip.Addr {
	var b [4]byte
	binary.BigEndian.PutUint32(b[:], v)
	return netip.AddrFrom4(b)
}

// broadcast – địa chỉ cuối của mạng (địa chỉ quảng bá).
func broadcast(p netip.Prefix) netip.Addr {
	return fromU32(toU32(p.Addr()) + uint32(size(p)) - 1)
}

func size(p netip.Prefix) uint64 { return 1 << (32 - p.Bits()) }

// mask – mặt nạ mạng dạng chấm thập phân.
func mask(p netip.Prefix) netip.Addr { return fromU32(^uint32(0) << (32 - p.Bits())) }

// prefixForHosts – độ dài tiền tố nhỏ nhất chứa được n máy (+2 địa chỉ mạng và quảng bá).
func prefixForHosts(n int) int {
	sz, bits := 4, 30 // khối nhỏ nhất dùng ở đây là /30
	for sz-2 < n {
		sz *= 2
		bits--
	}
	return bits
}

// planVLSM – cấp mạng con cho từng yêu cầu, lớn trước nhỏ sau.
func planVLSM(block netip.Prefix, reqs []Request) ([]Alloc, error) {
	order := append([]Request(nil), reqs...)
	sort.SliceStable(order, func(i, j int) bool { return order[i].Hosts > order[j].Hosts })
	cursor := uint64(toU32(block.Addr()))
	end := cursor + size(block) - 1
	var result []Alloc
	for _, r := range order {
		plen := prefixForHosts(r.Hosts)
		sz := uint64(1) << (32 - plen)
		cursor = (cursor + sz - 1) / sz * sz // làm tròn lên bội của kích thước khối
		if cursor+sz-1 > end {
			return nil, fmt.Errorf("Không đủ chỗ cho %s (%d máy cần /%d) trong %s", r.Name, r.Hosts, plen, block)
		}
		result = append(result, Alloc{r.Name, r.Hosts, netip.PrefixFrom(fromU32(uint32(cursor)), plen)})
		cursor += sz
	}
	return result, nil
}

// rangeToCIDR – tách dải [lo, hi] thành ít khối CIDR nhất (mỗi khối thẳng hàng và lớn nhất có thể).
func rangeToCIDR(lo, hi uint64) []netip.Prefix {
	var out []netip.Prefix
	for lo <= hi {
		bits := 32
		for bits > 0 {
			sz := uint64(1) << (32 - (bits - 1))
			if lo%sz != 0 || lo+sz-1 > hi {
				break
			}
			bits--
		}
		out = append(out, netip.PrefixFrom(fromU32(uint32(lo)), bits))
		lo += uint64(1) << (32 - bits)
	}
	return out
}

// freeBlocks – các khoảng trống giữa những mạng đã cấp, viết dưới dạng CIDR.
func freeBlocks(block netip.Prefix, allocs []Alloc) []netip.Prefix {
	used := make([]netip.Prefix, len(allocs))
	for i, a := range allocs {
		used[i] = a.Net
	}
	sort.Slice(used, func(i, j int) bool { return used[i].Addr().Less(used[j].Addr()) })
	cursor := uint64(toU32(block.Addr()))
	end := cursor + size(block) - 1
	var out []netip.Prefix
	for _, u := range used {
		start := uint64(toU32(u.Addr()))
		if start > cursor {
			out = append(out, rangeToCIDR(cursor, start-1)...)
		}
		cursor = start + size(u)
	}
	if cursor <= end {
		out = append(out, rangeToCIDR(cursor, end)...)
	}
	return out
}

// checkPrefix – kiểm tra một tiền tố đề xuất, trả về chuỗi kết luận.
func checkPrefix(text string, block netip.Prefix, allocs []Alloc) string {
	p, err := netip.ParsePrefix(text)
	if err != nil || !p.Addr().Is4() {
		return "KHÔNG HỢP LỆ (sai cú pháp địa chỉ hoặc tiền tố)"
	}
	note := ""
	if p.Masked() != p { // phần máy của địa chỉ khác 0
		p = p.Masked()
		note = fmt.Sprintf("có bit máy, chuẩn hóa %s; ", p)
	}
	if p.Bits() < block.Bits() || !block.Contains(p.Addr()) {
		return note + fmt.Sprintf("NGOÀI KHỐI %s", block)
	}
	for _, a := range allocs {
		if p.Overlaps(a.Net) {
			return note + fmt.Sprintf("CHỒNG LẤN với %s (%s)", a.Name, a.Net)
		}
	}
	return note + "HỢP LỆ, còn trống"
}

func main() {
	paths := os.Args[1:]
	if len(paths) == 0 {
		paths = []string{"ke_hoach.json"}
	}
	ok := true
	for _, p := range paths {
		if !runPlan(p) {
			ok = false
		}
	}
	if !ok {
		os.Exit(1)
	}
}

// runPlan – xử lý một tệp kế hoạch; trả về false nếu kế hoạch không thực hiện được.
func runPlan(path string) bool {
	data, err := os.ReadFile(path)
	if err != nil {
		fmt.Println("[ipam] LỖI:", err)
		return false
	}
	var plan Plan
	if err := json.Unmarshal(data, &plan); err != nil {
		fmt.Println("[ipam] LỖI JSON:", err)
		return false
	}
	fmt.Printf("===== Kế hoạch: %s =====\n", path)
	block := netip.MustParsePrefix(plan.Block)
	kind := "công cộng"
	if block.Addr().IsPrivate() {
		kind = "riêng (RFC 1918)"
	}
	fmt.Printf("[ipam] Khối địa chỉ %s: %d địa chỉ, %s\n", block, size(block), kind)
	allocs, err := planVLSM(block, plan.Requests)
	if err != nil {
		fmt.Println("[ipam] LỖI:", err)
		return false
	}

	fmt.Println("\n[ipam] 1) Chia mạng con (VLSM, yêu cầu lớn cấp trước)")
	fmt.Printf("  %-14s %4s %8s %-16s %-15s %s\n", "Tên", "Cần", "Khả dụng", "Mạng con", "Mặt nạ", "Dải khả dụng")
	var used uint64
	for _, a := range allocs {
		first, last := a.Net.Addr().Next(), broadcast(a.Net).Prev() // bỏ địa chỉ mạng, quảng bá
		fmt.Printf("  %-14s %4d %8d %-16s %-15s %s - %s\n", a.Name, a.Hosts, size(a.Net)-2, a.Net, mask(a.Net), first, last)
		used += size(a.Net)
	}
	fmt.Printf("  Đã dùng %d/%d địa chỉ (%.1f%%)\n", used, size(block), float64(used)*100/float64(size(block)))

	fmt.Println("\n[ipam] 2) Khối còn trống")
	for _, f := range freeBlocks(block, allocs) {
		fmt.Printf("  %-17s (%d địa chỉ)\n", f, size(f))
	}

	if len(plan.Checks) > 0 {
		fmt.Println("\n[ipam] 3) Kiểm tra tiền tố đề xuất")
		for _, t := range plan.Checks {
			fmt.Printf("  %-17s -> %s\n", t, checkPrefix(t, block, allocs))
		}
	}

	if plan.Assign != nil {
		for _, a := range allocs {
			if a.Name != plan.Assign.Net {
				continue
			}
			fmt.Printf("\n[ipam] 4) Cấp phát địa chỉ trong %s (%s)\n", a.Name, a.Net)
			ip := a.Net.Addr().Next() // địa chỉ khả dụng đầu tiên làm cổng mặc định
			last := broadcast(a.Net).Prev()
			fmt.Printf("  %-8s %s/%d\n", "gateway", ip, a.Net.Bits())
			for _, d := range plan.Assign.Devices {
				ip = ip.Next()
				if last.Less(ip) {
					fmt.Printf("[ipam] LỖI: Mạng %s đã hết địa chỉ khi cấp cho %s\n", a.Net, d)
					return false
				}
				fmt.Printf("  %-8s %s/%d\n", d, ip, a.Net.Bits())
			}
		}
	}
	return true
}
