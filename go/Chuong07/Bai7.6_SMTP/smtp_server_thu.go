// Bài 7.6 – Server SMTP thử nghiệm cục bộ (chỉ để chạy demo client, không chuyển thư đi đâu).
//
// Bắt buộc STARTTLS (chứng chỉ tự ký cert.pem/key.pem, tạo bằng tao_chung_chi.sh) và AUTH PLAIN sau TLS;
// tài khoản thử lấy từ biến môi trường SMTP_USER, SMTP_PASS (mặc định sinhvien / matkhau-thu).
// Thư nhận được chỉ được in ra màn hình. Đây không phải nội dung của mã giả, chỉ đóng vai server
// như aiosmtpd ở bản Python, để bài Go tự chứa.
//
// Chạy:  out/app server [host] [port]      (mặc định 127.0.0.1 587; cổng < 1024 cần quyền quản trị)
package main

import (
	"bufio"
	"crypto/subtle"
	"crypto/tls"
	"encoding/base64"
	"fmt"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"sync"
)

var printMu sync.Mutex

func slog(format string, a ...any) {
	printMu.Lock()
	defer printMu.Unlock()
	fmt.Printf("[server thử] "+format+"\n", a...)
}

func envOr(name, def string) string {
	if v := os.Getenv(name); v != "" {
		return v
	}
	return def
}

// smtpSession: một phiên SMTP phía server.
func smtpSession(conn net.Conn, tlsCfg *tls.Config, user, pass string) {
	defer func() { conn.Close() }()
	r := bufio.NewReader(conn)
	reply := func(lines ...string) { // mỗi dòng "mã văn_bản"; mọi dòng trừ dòng cuối dùng '-' sau mã
		var b strings.Builder
		for i, l := range lines {
			sep := "-"
			if i == len(lines)-1 {
				sep = " "
			}
			b.WriteString(l[:3] + sep + l[4:] + "\r\n")
		}
		conn.Write([]byte(b.String()))
	}
	secure, authed := false, false
	var from string
	var rcpts []string
	reply("220 smtp.thu.local Go SMTP thử")
	for {
		line, err := r.ReadString('\n')
		if err != nil {
			return
		}
		line = strings.TrimRight(line, "\r\n")
		verb, arg, _ := strings.Cut(line, " ")
		verb = strings.ToUpper(verb)
		switch {
		case verb == "EHLO" || verb == "HELO":
			ext := []string{"250 smtp.thu.local", "250 SIZE 33554432", "250 8BITMIME"}
			if secure {
				ext = append(ext, "250 AUTH PLAIN") // chỉ cho AUTH sau TLS
			} else {
				ext = append(ext, "250 STARTTLS")
			}
			reply(append(ext, "250 HELP")...)
		case verb == "STARTTLS" && !secure:
			reply("220 Ready to start TLS")
			t := tls.Server(conn, tlsCfg)
			if err := t.Handshake(); err != nil {
				slog("Bắt tay TLS lỗi: %v", err)
				return
			}
			conn, r, secure = t, bufio.NewReader(t), true // bỏ mọi byte nhận trước TLS
		case verb == "QUIT":
			reply("221 Bye")
			return
		case verb == "NOOP" || verb == "RSET":
			reply("250 OK")
		case !secure:
			reply("530 Must issue a STARTTLS command first")
		case verb == "AUTH":
			mech, initial, _ := strings.Cut(arg, " ")
			if strings.ToUpper(mech) != "PLAIN" {
				reply("504 5.5.4 Unrecognized authentication type")
				continue
			}
			raw, err := base64.StdEncoding.DecodeString(initial)
			parts := strings.Split(string(raw), "\x00") // authzid \0 user \0 pass
			if err != nil || len(parts) != 3 {
				reply("501 5.5.2 Can't decode base64")
				continue
			}
			authed = subtle.ConstantTimeCompare([]byte(parts[1]), []byte(user)) == 1 &&
				subtle.ConstantTimeCompare([]byte(parts[2]), []byte(pass)) == 1
			if authed {
				slog("AUTH PLAIN của '%s': thành công", parts[1])
				reply("235 2.7.0 Authentication successful")
			} else {
				slog("AUTH PLAIN của '%s': sai thông tin", parts[1])
				reply("535 5.7.8 Authentication credentials invalid")
			}
		case !authed:
			reply("530 5.7.0 Authentication required")
		case verb == "MAIL" && strings.HasPrefix(strings.ToUpper(arg), "FROM:"):
			from, rcpts = strings.Trim(arg[5:], "<> "), nil
			reply("250 OK")
		case verb == "RCPT" && strings.HasPrefix(strings.ToUpper(arg), "TO:"):
			rcpts = append(rcpts, strings.Trim(arg[3:], "<> "))
			reply("250 OK")
		case verb == "DATA":
			reply("354 End data with <CR><LF>.<CR><LF>")
			var content []string
			for {
				l, err := r.ReadString('\n')
				if err != nil {
					return
				}
				l = strings.TrimRight(l, "\r\n")
				if l == "." { // dòng "." kết thúc thư
					break
				}
				content = append(content, strings.TrimPrefix(l, ".")) // bỏ dấu chấm do DOT_STUFF thêm
			}
			size := len(strings.Join(content, "\r\n")) + 2
			slog("Thư từ %s tới %s (%d byte):", from, strings.Join(rcpts, ", "), size)
			for _, l := range content {
				slog("  | %s", l)
			}
			reply("250 OK: thư đã được nhận")
		default:
			reply("500 Error: command not recognized")
		}
	}
}

func serverMain(args []string) {
	host, port := "127.0.0.1", 587
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		port = mustPort(args[1])
	}
	cert, err := tls.LoadX509KeyPair("cert.pem", "key.pem")
	if err != nil {
		fail("[server thử] Không nạp được cert.pem/key.pem (chạy sh tao_chung_chi.sh): " + err.Error())
	}
	tlsCfg := &tls.Config{Certificates: []tls.Certificate{cert}, MinVersion: tls.VersionTLS12}
	user, pass := envOr("SMTP_USER", "sinhvien"), envOr("SMTP_PASS", "matkhau-thu")
	srv, err := net.Listen("tcp4", net.JoinHostPort(host, strconv.Itoa(port)))
	if err != nil {
		fail("[server thử] Không mở được cổng: " + err.Error())
	}
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, os.Interrupt)
	go func() {
		<-ch
		slog("Dừng server")
		os.Exit(0)
	}()
	slog("SMTP tại %s:%d (bắt buộc STARTTLS, AUTH sau TLS)", host, port)
	for {
		conn, err := srv.Accept()
		if err != nil {
			continue
		}
		go smtpSession(conn, tlsCfg, user, pass)
	}
}
