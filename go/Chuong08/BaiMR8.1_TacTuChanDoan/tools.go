package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"sort"
	"strconv"
	"syscall"
	"time"
)

// toolError – lời gọi công cụ bị từ chối hoặc thất bại; kết quả gửi lại LLM dưới dạng {"loi": ...}.
type toolError struct{ msg string }

func (e toolError) Error() string { return e.msg }

var allowedNets = []netip.Prefix{ // chính sách đích: chỉ mạng phòng thí nghiệm
	netip.MustParsePrefix("127.0.0.0/8"), netip.MustParsePrefix("10.0.0.0/8"),
	netip.MustParsePrefix("172.16.0.0/12"), netip.MustParsePrefix("192.168.0.0/16"),
}

// ---------- Thân công cụ (chạy bằng socket API, không gọi shell) ----------

func resolve(name string) []string {
	addrs, err := net.LookupHost(name)
	if err != nil {
		return []string{}
	}
	set := map[string]bool{}
	for _, a := range addrs {
		set[a] = true
	}
	out := make([]string, 0, len(set))
	for a := range set {
		out = append(out, a)
	}
	sort.Strings(out)
	return out
}

func checkTarget(host string) ([]string, error) {
	addrs := resolve(host)
	if len(addrs) == 0 {
		return nil, toolError{"không phân giải được " + host}
	}
	for _, a := range addrs {
		ip, err := netip.ParseAddr(a)
		ok := false
		for _, n := range allowedNets {
			if err == nil && n.Contains(ip.Unmap()) {
				ok = true
			}
		}
		if !ok {
			return nil, toolError{a + " nằm ngoài mạng phòng thí nghiệm"}
		}
	}
	return addrs, nil
}

func toolResolve(args map[string]any) (map[string]any, error) {
	return map[string]any{"dia_chi": resolve(args["ten"].(string))}, nil
}

func toolPort(args map[string]any) (map[string]any, error) {
	addrs, err := checkTarget(args["host"].(string))
	if err != nil {
		return nil, err
	}
	port := int(args["cong"].(float64))
	timeout := 1000.0
	if t, ok := args["thoi_gian_cho_ms"].(float64); ok {
		timeout = t
	}
	state := "FILTERED"
	for _, a := range addrs {
		c, err := net.DialTimeout("tcp", net.JoinHostPort(a, strconv.Itoa(port)), time.Duration(timeout)*time.Millisecond)
		if err == nil {
			c.Close()
			return map[string]any{"trang_thai": "OPEN", "dia_chi": a}, nil
		}
		if errors.Is(err, syscall.ECONNREFUSED) {
			state = "CLOSED"
		}
	}
	return map[string]any{"trang_thai": state}, nil
}

// prop – một tham số trong lược đồ (tập con nhỏ của JSON Schema).
type prop struct {
	Type        string `json:"type"`
	Description string `json:"description,omitempty"`
	Min         *int   `json:"minimum,omitempty"`
	Max         *int   `json:"maximum,omitempty"`
}

type schema struct {
	Type       string          `json:"type"`
	Required   []string        `json:"required"`
	Properties map[string]prop `json:"properties"`
}

type tool struct {
	Description string
	Schema      schema
	Fn          func(map[string]any) (map[string]any, error)
}

func ip(v int) *int { return &v }

// tools – danh mục công cụ: chỉ những công cụ này mới được chạy.
var tools = map[string]tool{
	"phan_giai_ten": {"Phân giải tên máy thành danh sách địa chỉ IP",
		schema{"object", []string{"ten"}, map[string]prop{"ten": {Type: "string", Description: "tên máy cần phân giải"}}},
		toolResolve},
	"kiem_tra_cong": {"Kiểm tra cổng TCP: OPEN, CLOSED hoặc FILTERED",
		schema{"object", []string{"host", "cong"}, map[string]prop{
			"host":             {Type: "string"},
			"cong":             {Type: "integer", Min: ip(1), Max: ip(65535)},
			"thoi_gian_cho_ms": {Type: "integer", Min: ip(100), Max: ip(5000)}}},
		toolPort},
}

// validate – kiểm tra tham số theo lược đồ: đủ trường, không trường lạ, đúng kiểu, trong miền.
func validate(s schema, args map[string]any) error {
	for _, k := range s.Required {
		if _, ok := args[k]; !ok {
			return toolError{"thiếu tham số " + k}
		}
	}
	for k, v := range args {
		p, ok := s.Properties[k]
		if !ok {
			return toolError{"tham số lạ " + k}
		}
		switch p.Type {
		case "integer":
			f, ok := v.(float64) // encoding/json giải mã mọi số thành float64
			if !ok || f != float64(int(f)) {
				return toolError{k + " phải là số nguyên"}
			}
			if p.Min != nil && (int(f) < *p.Min || int(f) > *p.Max) {
				return toolError{fmt.Sprintf("%s ngoài miền [%d, %d]", k, *p.Min, *p.Max)}
			}
		case "string":
			if _, ok := v.(string); !ok {
				return toolError{k + " phải là chuỗi"}
			}
		}
	}
	return nil
}

// runTool – ba lớp kiểm soát trước khi chạy: danh mục, tham số, chính sách đích (trong thân công cụ).
func runTool(name, arguments string) (map[string]any, error) {
	t, ok := tools[name]
	if !ok {
		return nil, toolError{"công cụ không có trong danh mục"}
	}
	var args map[string]any
	if err := json.Unmarshal([]byte(arguments), &args); err != nil {
		return nil, toolError{"tham số không phải JSON"}
	}
	if err := validate(t.Schema, args); err != nil {
		return nil, err
	}
	return t.Fn(args)
}
