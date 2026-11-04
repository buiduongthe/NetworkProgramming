package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/netip"
	"os"
	"regexp"
	"strings"
	"time"
)

// Dữ liệu CLI là luồng byte KHÔNG có khung thông điệp, nên client "đọc cho tới khi gặp dấu nhắc"
// (READ_UNTIL) – định khung bằng dấu phân cách, ở đây là một biểu thức chính quy.

var promptRe = regexp.MustCompile(`[\w.-]+(\([\w-]+\))?[>#]$`) // R1>  R1#  R1(config)#  R1(config-if)#

type cliSession struct {
	conn    net.Conn
	timeout time.Duration
	buf     string
}

func dialCLI(addr string, timeout time.Duration) (*cliSession, error) {
	c, err := net.DialTimeout("tcp", addr, timeout)
	if err != nil {
		return nil, err
	}
	return &cliSession{conn: c, timeout: timeout}, nil
}

// readUntil – đọc dồn vào bộ đệm cho tới khi phần cuối khớp mẫu; trả về phần đã đọc.
func (c *cliSession) readUntil(re *regexp.Regexp) (string, error) {
	deadline := time.Now().Add(c.timeout)
	c.conn.SetReadDeadline(deadline) // hạn chót cho cả lần chờ dấu nhắc này
	chunk := make([]byte, 4096)
	for {
		text := strings.TrimRight(c.buf, " ")
		if re.MatchString(text) {
			c.buf = ""
			return text, nil
		}
		if strings.Contains(c.buf, "% Authentication failed") {
			return "", errors.New("Đăng nhập thất bại")
		}
		n, err := c.conn.Read(chunk)
		c.buf += string(chunk[:n])
		if err != nil {
			var ne net.Error
			if errors.As(err, &ne) && ne.Timeout() {
				return "", fmt.Errorf("Hết thời gian chờ dấu nhắc, đã nhận: %q", tail(c.buf, 60))
			}
			if n == 0 && !strings.Contains(c.buf, "% Authentication failed") {
				return "", errors.New("Thiết bị đóng kết nối")
			}
		}
	}
}

func tail(s string, n int) string {
	if len(s) > n {
		return s[len(s)-n:]
	}
	return s
}

func (c *cliSession) sendLine(line string) error {
	_, err := c.conn.Write([]byte(line + "\n"))
	return err
}

func (c *cliSession) login(user, password string) error {
	steps := []struct {
		re   *regexp.Regexp
		send string
	}{{regexp.MustCompile(`Username:$`), user}, {regexp.MustCompile(`Password:$`), password}}
	for _, st := range steps {
		if _, err := c.readUntil(st.re); err != nil {
			return err
		}
		if err := c.sendLine(st.send); err != nil {
			return err
		}
	}
	_, err := c.readUntil(promptRe)
	return err
}

// command – gửi một lệnh, chờ dấu nhắc kế tiếp, trả về phần kết quả (bỏ dòng vọng lại và dấu nhắc).
func (c *cliSession) command(cmd string) ([]string, error) {
	if err := c.sendLine(cmd); err != nil {
		return nil, err
	}
	text, err := c.readUntil(promptRe)
	if err != nil {
		return nil, err
	}
	out := strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n")
	if len(out) > 0 && strings.TrimSpace(out[0]) == cmd {
		out = out[1:] // bỏ dòng vọng lại
	}
	body := out[:len(out)-1] // bỏ dòng dấu nhắc
	for _, l := range body {
		if strings.HasPrefix(l, "%") {
			return nil, fmt.Errorf("Thiết bị từ chối lệnh '%s': %s", cmd, body[0])
		}
	}
	return body, nil
}

func (c *cliSession) close() {
	c.sendLine("exit")
	c.conn.Close()
}

// wantIface – một cổng trong tệp trạng thái mong muốn (tên trường JSON giống bản Python).
type wantIface struct {
	Name string `json:"ten"`
	Desc string `json:"mo_ta"`
	IP   string `json:"ip"`
	Up   bool   `json:"bat"`
}

// parseInterface – phân tích khối "interface ..." thành cùng dạng với tệp mong muốn.
func parseInterface(lines []string) wantIface {
	cur := wantIface{Up: true}
	for _, l := range lines {
		l = strings.TrimSpace(l)
		switch {
		case strings.HasPrefix(l, "description "):
			cur.Desc = strings.TrimPrefix(l, "description ")
		case strings.HasPrefix(l, "ip address "):
			f := strings.Fields(l)
			ip, _ := netip.ParseAddr(f[2])
			mask, _ := netip.ParseAddr(f[3])
			cur.IP = netip.PrefixFrom(ip, maskBits(mask)).String()
		case l == "shutdown":
			cur.Up = false
		}
	}
	return cur
}

// diffCommands – chỉ sinh lệnh cho phần khác biệt.
func diffCommands(cur, want wantIface) []string {
	var cmds []string
	if cur.Desc != want.Desc {
		if want.Desc != "" {
			cmds = append(cmds, "description "+want.Desc)
		} else {
			cmds = append(cmds, "no description")
		}
	}
	if cur.IP != want.IP {
		p := netip.MustParsePrefix(want.IP)
		cmds = append(cmds, fmt.Sprintf("ip address %s %s", p.Addr(), bitsToMask(p.Bits())))
	}
	if cur.Up != want.Up {
		if want.Up {
			cmds = append(cmds, "no shutdown")
		} else {
			cmds = append(cmds, "shutdown")
		}
	}
	return cmds
}

func runAutomation(host, port, path string, dryRun bool) {
	data, err := os.ReadFile(path)
	if err != nil {
		fmt.Println("[tu-dong] LỖI:", err)
		os.Exit(1)
	}
	var want struct {
		Ifaces []wantIface `json:"cong"`
	}
	if err := json.Unmarshal(data, &want); err != nil {
		fmt.Println("[tu-dong] LỖI JSON:", err)
		os.Exit(1)
	}
	user := getenv("DEVICE_USER", "admin")
	fmt.Printf("[tu-dong] Kết nối %s:%s, tài khoản %s\n", host, port, user)
	if dryRun {
		fmt.Println("[tu-dong] Chế độ chạy thử: chỉ in lệnh, không áp dụng")
	}
	cli, err := dialCLI(net.JoinHostPort(host, port), 5*time.Second)
	if err != nil {
		fmt.Println("[tu-dong] LỖI:", err)
		os.Exit(1)
	}
	if err := automate(cli, user, getenv("DEVICE_PASSWORD", ""), want.Ifaces, dryRun); err != nil {
		fmt.Println("[tu-dong] LỖI:", err)
		cli.close()
		os.Exit(1)
	}
	cli.close()
}

func automate(cli *cliSession, user, password string, want []wantIface, dryRun bool) error {
	if err := cli.login(user, password); err != nil {
		return err
	}
	for _, c := range []string{"enable", "terminal length 0"} { // tắt --More-- để đọc trọn kết quả
		if _, err := cli.command(c); err != nil {
			return err
		}
	}
	ver, err := cli.command("show version")
	if err != nil {
		return err
	}
	fmt.Println("[tu-dong]", ver[0])
	total := 0
	for _, item := range want {
		lines, err := cli.command("show running-config interface " + item.Name)
		if err != nil {
			return err
		}
		cmds := diffCommands(parseInterface(lines), item)
		if len(cmds) == 0 {
			fmt.Printf("[tu-dong] %s: đã đúng, không cần thay đổi\n", item.Name)
			continue
		}
		fmt.Printf("[tu-dong] %s: cần %d lệnh\n", item.Name, len(cmds))
		for _, c := range cmds {
			fmt.Println("           +", c)
		}
		if dryRun {
			continue
		}
		seq := append([]string{"configure terminal", "interface " + item.Name}, cmds...)
		for _, c := range append(seq, "end") {
			if _, err := cli.command(c); err != nil {
				return err
			}
		}
		lines, err = cli.command("show running-config interface " + item.Name) // đọc lại để kiểm chứng
		if err != nil {
			return err
		}
		if after := parseInterface(lines); after != (wantIface{Desc: item.Desc, IP: item.IP, Up: item.Up}) {
			return fmt.Errorf("%s: cấu hình sau khi áp dụng chưa khớp: %+v", item.Name, after)
		}
		total += len(cmds)
	}
	if total > 0 {
		if _, err := cli.command("write memory"); err != nil {
			return err
		}
		fmt.Printf("[tu-dong] Đã áp dụng và kiểm chứng %d lệnh, đã lưu cấu hình\n", total)
	} else if !dryRun {
		fmt.Println("[tu-dong] Thiết bị đã ở đúng trạng thái mong muốn (lũy đẳng)")
	}
	brief, err := cli.command("show ip interface brief")
	if err != nil {
		return err
	}
	fmt.Println("[tu-dong] show ip interface brief:")
	for _, l := range brief {
		fmt.Println("  " + l)
	}
	return nil
}
