package main

import (
	"bufio"
	"fmt"
	"net"
	"net/netip"
	"os"
	"strings"
	"sync"
	"sync/atomic"
)

// Thiết bị mạng mô phỏng có CLI qua TCP (kiểu Telnet), giống bản Python thiet_bi_gia.py.
// CẢNH BÁO: mật khẩu đi dạng rõ như Telnet; thiết bị thật phải dùng SSH.

const hostname = "R1"
const pageLines = 8 // số dòng mỗi trang khi bật phân trang

type ifaceState struct {
	Desc, IP, Mask string
	Shut           bool
}

var (
	stateMu    sync.Mutex
	ifaceOrder = []string{"Gi0/0", "Gi0/1", "Gi0/2"}
	ifaces     = map[string]*ifaceState{ // trạng thái ban đầu của thiết bị
		"Gi0/0": {Desc: "Lien ket R2", IP: "10.20.1.225", Mask: "255.255.255.252"},
		"Gi0/1": {Shut: true},
		"Gi0/2": {Shut: true},
	}
	sessionID atomic.Int64
)

func dlog(format string, a ...any) { fmt.Printf("[thiet-bi] "+format+"\n", a...) }

func getenv(k, def string) string {
	if v, ok := os.LookupEnv(k); ok {
		return v
	}
	return def
}

// runningConfig – sinh cấu hình đang chạy (toàn bộ khi only rỗng, hoặc một cổng).
func runningConfig(only string) []string {
	var lines []string
	if only == "" {
		lines = []string{"!", "hostname " + hostname, "!"}
	}
	stateMu.Lock()
	defer stateMu.Unlock()
	for _, name := range ifaceOrder {
		if only != "" && name != only {
			continue
		}
		i := ifaces[name]
		lines = append(lines, "interface "+name)
		if i.Desc != "" {
			lines = append(lines, " description "+i.Desc)
		}
		if i.IP != "" {
			lines = append(lines, " ip address "+i.IP+" "+i.Mask)
		} else {
			lines = append(lines, " no ip address")
		}
		if i.Shut {
			lines = append(lines, " shutdown")
		}
		lines = append(lines, "!")
	}
	if only == "" {
		lines = append(lines, "end")
	}
	return lines
}

func ipBrief() []string {
	lines := []string{fmt.Sprintf("%-10s%-14s%-22s%s", "Interface", "IP-Address", "Status", "Protocol")}
	stateMu.Lock()
	defer stateMu.Unlock()
	for _, name := range ifaceOrder {
		i := ifaces[name]
		ip, status, proto := i.IP, "up", "up"
		if ip == "" {
			ip = "unassigned"
		}
		if i.Shut {
			status, proto = "administratively down", "down"
		}
		lines = append(lines, fmt.Sprintf("%-10s%-14s%-22s%s", name, ip, status, proto))
	}
	return lines
}

type session struct {
	conn   net.Conn
	r      *bufio.Reader // đọc theo dòng và theo từng byte
	id     int64
	mode   string // user, enable, config, config-if
	iface  string
	paging bool
}

func (s *session) send(t string) { s.conn.Write([]byte(t)) }

func (s *session) readLine() (string, error) {
	line, err := s.r.ReadString('\n')
	if err != nil {
		return "", err
	}
	return strings.TrimRight(line, "\r\n"), nil
}

func (s *session) prompt() {
	suffix := map[string]string{"user": ">", "enable": "#", "config": "(config)#", "config-if": "(config-if)#"}[s.mode]
	s.send(hostname + suffix)
}

// show – in nhiều dòng; nếu đang phân trang thì dừng mỗi pageLines dòng chờ một phím.
func (s *session) show(lines []string) error {
	for k, line := range lines {
		s.send(line + "\r\n")
		if s.paging && (k+1)%pageLines == 0 && k+1 < len(lines) {
			s.send(" --More-- ")
			if _, err := s.r.ReadByte(); err != nil {
				return err
			}
			s.send("\r          \r") // xóa chữ --More-- như terminal thật
		}
	}
	return nil
}

func (s *session) login() bool {
	s.send("\r\n" + hostname + " - thiết bị mô phỏng cho bài thực hành\r\n\r\nUsername: ")
	user, err := s.readLine()
	if err != nil {
		return false
	}
	s.send("Password: ")
	pw, err := s.readLine() // không vọng lại mật khẩu
	if err != nil {
		return false
	}
	s.send("\r\n")
	if user != getenv("DEVICE_USER", "admin") || pw != getenv("DEVICE_PASSWORD", "lab123") {
		dlog("phiên #%d: đăng nhập SAI (user='%s')", s.id, user)
		s.send("% Authentication failed\r\n")
		return false
	}
	dlog("phiên #%d: đăng nhập %s", s.id, user)
	return true
}

func (s *session) run() {
	if !s.login() {
		return
	}
	for {
		s.prompt()
		line, err := s.readLine()
		if err != nil {
			return
		}
		cmd := strings.TrimSpace(line)
		s.send(cmd + "\r\n") // vọng lại dòng lệnh
		if cmd == "" {
			continue
		}
		if !s.execute(cmd) {
			return
		}
	}
}

func (s *session) invalid() { s.send("% Invalid input detected at '^' marker.\r\n") }

// execute – thực hiện một lệnh; trả về false khi phiên kết thúc.
func (s *session) execute(cmd string) bool {
	w := strings.Fields(cmd)
	version := []string{"Thiết bị mô phỏng R1, phần mềm SIM-IOS 1.0", fmt.Sprintf("%d cổng GigabitEthernet", len(ifaceOrder))}
	switch {
	case cmd == "exit" && (s.mode == "user" || s.mode == "enable"):
		return false
	case s.mode == "user":
		switch cmd {
		case "enable":
			s.mode = "enable"
		case "show version":
			s.show(version)
		default:
			s.invalid()
		}
	case s.mode == "enable":
		switch {
		case cmd == "terminal length 0":
			s.paging = false
		case cmd == "show version":
			s.show(version)
		case cmd == "show ip interface brief":
			s.show(ipBrief())
		case cmd == "show running-config":
			s.show(runningConfig(""))
		case len(w) == 4 && w[0] == "show" && w[1] == "running-config" && w[2] == "interface":
			if _, ok := ifaces[w[3]]; ok {
				s.show(runningConfig(w[3]))
			} else {
				s.invalid()
			}
		case cmd == "configure terminal" || cmd == "conf t":
			s.send("Enter configuration commands, one per line.  End with CNTL/Z.\r\n")
			s.mode = "config"
		case cmd == "write memory" || cmd == "wr":
			s.send("Building configuration...\r\n[OK]\r\n")
			dlog("phiên #%d: lưu cấu hình (write memory)", s.id)
		default:
			s.invalid()
		}
	case cmd == "end":
		s.mode, s.iface = "enable", ""
	case s.mode == "config":
		if _, ok := ifaces[safeIndex(w, 1)]; ok && len(w) == 2 && w[0] == "interface" {
			s.mode, s.iface = "config-if", w[1]
		} else if cmd == "exit" {
			s.mode = "enable"
		} else {
			s.invalid()
		}
	case s.mode == "config-if":
		s.configIf(cmd, w)
	}
	return true
}

func safeIndex(w []string, i int) string {
	if i < len(w) {
		return w[i]
	}
	return ""
}

func (s *session) configIf(cmd string, w []string) {
	if cmd == "exit" {
		s.mode, s.iface = "config", ""
		return
	}
	stateMu.Lock()
	i := ifaces[s.iface]
	ok := true
	switch {
	case w[0] == "description" && len(w) > 1:
		i.Desc = strings.TrimPrefix(cmd, "description ")
	case cmd == "no description":
		i.Desc = ""
	case len(w) == 4 && w[0] == "ip" && w[1] == "address":
		ip, err1 := netip.ParseAddr(w[2])
		mask, err2 := netip.ParseAddr(w[3])
		if err1 != nil || err2 != nil || !ip.Is4() || maskBits(mask) < 0 {
			stateMu.Unlock()
			s.send("% Invalid address\r\n")
			return
		}
		i.IP, i.Mask = w[2], w[3]
	case cmd == "shutdown":
		i.Shut = true
	case cmd == "no shutdown":
		i.Shut = false
	default:
		ok = false
	}
	stateMu.Unlock()
	if !ok {
		s.invalid()
		return
	}
	dlog("phiên #%d: %s: %s", s.id, s.iface, cmd)
}

// runDevice – vòng chấp nhận kết nối; mỗi phiên CLI một goroutine.
func runDevice(host, port string) {
	ln, err := net.Listen("tcp", net.JoinHostPort(host, port))
	if err != nil {
		dlog("Lỗi: %v", err)
		os.Exit(1)
	}
	dlog("%s lắng nghe CLI tại %s:%s", hostname, host, port)
	stopOnInterrupt(ln, "[thiet-bi] Dừng thiết bị")
	for {
		c, err := ln.Accept()
		if err != nil {
			return
		}
		go func(c net.Conn) {
			s := &session{conn: c, r: bufio.NewReader(c), id: sessionID.Add(1), mode: "user", paging: true}
			defer func() {
				c.Close()
				dlog("phiên #%d (%s) kết thúc", s.id, c.RemoteAddr())
			}()
			s.run()
		}(c)
	}
}
