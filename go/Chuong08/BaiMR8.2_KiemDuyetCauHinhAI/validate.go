package main

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"net/netip"
	"regexp"
)

// Config – cấu hình LLM được yêu cầu sinh, sau khi qua kiểm tra lược đồ. JSON được giải mã
// trước vào map[string]any để tự kiểm tra kiểu (LLM có thể trả sai kiểu, ví dụ "id": "30").
type Config struct {
	VLAN   []VLAN
	ACL    []ACL
	DichVu map[string]any
}

// VLAN – một VLAN đã qua kiểm tra lược đồ.
type VLAN struct {
	ID      int
	Ten     string
	Mang    string
	CongMac string
}

// ACL – một danh sách điều khiển truy cập.
type ACL struct {
	Ten    string
	QuyTac []string
}

// Infra – hạ tầng đang dùng (ha_tang.json).
type Infra struct {
	Khoi   string `json:"khoi_dia_chi"`
	DaDung []struct {
		Ten  string `json:"ten"`
		VLAN int    `json:"vlan"`
		Mang string `json:"mang"`
	} `json:"da_dung"`
}

var fenceRe = regexp.MustCompile("(?s)```(?:json)?\\s*\\n(.*?)```")
var forbiddenRule = regexp.MustCompile(`permit\s+(ip\s+)?any\s+any`)

// extractJSON – lấy JSON từ khối ```json ... ``` nếu có, nếu không thì dùng toàn bộ văn bản.
func extractJSON(text string) (map[string]any, error) {
	if m := fenceRe.FindStringSubmatch(text); m != nil {
		text = m[1]
	}
	var root any
	if err := json.Unmarshal([]byte(text), &root); err != nil {
		return nil, err
	}
	obj, ok := root.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("gốc phải là đối tượng JSON")
	}
	return obj, nil
}

// checkSchema – đủ trường, đúng kiểu; trả về cấu hình có kiểu tĩnh nếu đạt.
func checkSchema(obj map[string]any) (*Config, []string) {
	var errs []string
	vl, ok1 := obj["vlan"].([]any)
	al, ok2 := obj["acl"].([]any)
	dv, ok3 := obj["dich_vu"].(map[string]any)
	for _, c := range []struct {
		ok  bool
		key string
	}{{ok1, "vlan"}, {ok2, "acl"}, {ok3, "dich_vu"}} {
		if !c.ok {
			errs = append(errs, fmt.Sprintf("thiếu khóa %s hoặc sai kiểu", c.key))
		}
	}
	cfg := &Config{DichVu: dv}
	for i, raw := range vl {
		m, _ := raw.(map[string]any)
		var v VLAN
		id, okID := m["id"].(float64) // encoding/json giải mã số thành float64
		if !okID || id != float64(int(id)) {
			errs = append(errs, fmt.Sprintf("vlan[%d].id thiếu hoặc sai kiểu", i))
		}
		v.ID = int(id)
		for _, f := range []struct {
			key string
			dst *string
		}{{"ten", &v.Ten}, {"mang", &v.Mang}, {"cong_mac_dinh", &v.CongMac}} {
			s, ok := m[f.key].(string)
			if !ok {
				errs = append(errs, fmt.Sprintf("vlan[%d].%s thiếu hoặc sai kiểu", i, f.key))
			}
			*f.dst = s
		}
		cfg.VLAN = append(cfg.VLAN, v)
	}
	for i, raw := range al {
		m, _ := raw.(map[string]any)
		a := ACL{}
		var okTen bool
		a.Ten, okTen = m["ten"].(string)
		if !okTen {
			errs = append(errs, fmt.Sprintf("acl[%d].ten thiếu hoặc sai kiểu", i))
		}
		// quy_tac phải là danh sách chuỗi; bỏ qua phần tử sai kiểu sẽ làm lọt quy tắc cấm
		rules, okDs := m["quy_tac"].([]any)
		for _, r := range rules {
			s, ok := r.(string)
			okDs = okDs && ok
			a.QuyTac = append(a.QuyTac, s)
		}
		if !okDs {
			errs = append(errs, fmt.Sprintf("acl[%d].quy_tac thiếu hoặc sai kiểu", i))
		}
		cfg.ACL = append(cfg.ACL, a)
	}
	if len(errs) > 0 {
		return nil, errs
	}
	return cfg, nil
}

func lastAddr(p netip.Prefix) netip.Addr {
	b := p.Addr().As4()
	v := binary.BigEndian.Uint32(b[:]) | (^uint32(0) >> p.Bits())
	binary.BigEndian.PutUint32(b[:], v)
	return netip.AddrFrom4(b)
}

func maskOf(p netip.Prefix) netip.Addr {
	var b [4]byte
	binary.BigEndian.PutUint32(b[:], ^uint32(0)<<(32-p.Bits()))
	return netip.AddrFrom4(b)
}

// checkSemantics – VLAN, mạng con, cổng mặc định, chồng lấn, đụng hạ tầng.
func checkSemantics(cfg *Config, infra Infra) []string {
	var errs []string
	block := netip.MustParsePrefix(infra.Khoi)
	used := map[int]string{}
	for _, d := range infra.DaDung {
		used[d.VLAN] = d.Ten
	}
	seen := map[int]bool{}
	type named struct {
		name string
		net  netip.Prefix
	}
	var nets []named
	for _, v := range cfg.VLAN {
		switch {
		case v.ID < 1 || v.ID > 4094:
			errs = append(errs, fmt.Sprintf("%s: VLAN %d ngoài miền 1-4094", v.Ten, v.ID))
		case used[v.ID] != "":
			errs = append(errs, fmt.Sprintf("%s: VLAN %d đã dùng cho %s", v.Ten, v.ID, used[v.ID]))
		case seen[v.ID]:
			errs = append(errs, fmt.Sprintf("%s: VLAN %d bị trùng", v.Ten, v.ID))
		}
		seen[v.ID] = true
		net, err := netip.ParsePrefix(v.Mang)
		if err != nil || !net.Addr().Is4() || net.Masked() != net { // không chấp nhận bit máy
			errs = append(errs, fmt.Sprintf("%s: mạng %s không hợp lệ", v.Ten, v.Mang))
			continue
		}
		if net.Bits() < block.Bits() || !block.Contains(net.Addr()) {
			errs = append(errs, fmt.Sprintf("%s: mạng %s ngoài khối %s", v.Ten, net, block))
		}
		if gw, err := netip.ParseAddr(v.CongMac); err != nil || !gw.Is4() {
			errs = append(errs, fmt.Sprintf("%s: cổng mặc định %s không phải IPv4 hợp lệ", v.Ten, v.CongMac))
		} else if !net.Contains(gw) || gw == net.Addr() || gw == lastAddr(net) {
			errs = append(errs, fmt.Sprintf("%s: cổng mặc định %s không phải địa chỉ máy của %s", v.Ten, gw, net))
		}
		for _, o := range nets {
			if net.Overlaps(o.net) {
				errs = append(errs, fmt.Sprintf("%s: mạng %s chồng lấn %s (%s)", v.Ten, net, o.name, o.net))
			}
		}
		for _, d := range infra.DaDung {
			if net.Overlaps(netip.MustParsePrefix(d.Mang)) {
				errs = append(errs, fmt.Sprintf("%s: mạng %s đụng hạ tầng %s (%s)", v.Ten, net, d.Ten, d.Mang))
			}
		}
		nets = append(nets, named{v.Ten, net})
	}
	return errs
}

// checkPolicy – chính sách an toàn.
func checkPolicy(cfg *Config) []string {
	var errs []string
	for _, a := range cfg.ACL {
		for _, r := range a.QuyTac {
			if forbiddenRule.MatchString(r) {
				errs = append(errs, fmt.Sprintf("ACL %s: cấm quy tắc \"%s\"", a.Ten, r))
			}
		}
	}
	if t, ok := cfg.DichVu["telnet"].(bool); !ok || t {
		errs = append(errs, "dich_vu.telnet phải tắt (mật khẩu đi dạng rõ), dùng SSH")
	}
	return errs
}

// validate – toàn bộ quy trình kiểm duyệt một câu trả lời của LLM.
func validate(text string, infra Infra) (*Config, []string) {
	obj, err := extractJSON(text)
	if err != nil {
		return nil, []string{"JSON hỏng: " + err.Error()}
	}
	cfg, errs := checkSchema(obj)
	if errs != nil {
		return nil, errs
	}
	return cfg, append(checkSemantics(cfg, infra), checkPolicy(cfg)...)
}

// render – chuyển cấu hình đã duyệt thành lệnh CLI (kiểu IOS) – mẫu đơn giản.
func render(cfg *Config) []string {
	var lines []string
	for _, v := range cfg.VLAN {
		p := netip.MustParsePrefix(v.Mang)
		lines = append(lines, fmt.Sprintf("vlan %d", v.ID), " name "+v.Ten,
			fmt.Sprintf("interface Vlan%d", v.ID), fmt.Sprintf(" ip address %s %s", v.CongMac, maskOf(p)))
	}
	for _, a := range cfg.ACL {
		lines = append(lines, "ip access-list extended "+a.Ten)
		for _, r := range a.QuyTac {
			lines = append(lines, " "+r)
		}
	}
	if t, _ := cfg.DichVu["telnet"].(bool); t {
		lines = append(lines, "service telnet")
	} else {
		lines = append(lines, "no service telnet")
	}
	return lines
}
