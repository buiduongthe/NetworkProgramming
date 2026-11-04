// Bài 7.6 – Phiên gửi thư SMTP thủ công trên socket, có STARTTLS và AUTH PLAIN.
//
// Cài đặt Mã giả 7.7 (Mục 7.5.1, Giáo trình Lập trình mạng): SMTP_REPLY, CMD, SEND_MAIL, DOT_STUFF.
// Kế thừa: Mã giả 2.2 (SAFE_CONNECT), 3.5 (READ_LINE). TLS_WRAP dùng gói crypto/tls (Mục 8.4).
// Tài khoản lấy từ biến môi trường SMTP_USER, SMTP_PASS (không ghi mật khẩu trong mã nguồn).
//
// Chạy:  out/app client [host] [port] [tệp_CA]
//
//	(mặc định localhost 587 cert.pem; tệp_CA = "-" dùng kho chứng chỉ của hệ điều hành)
package main

import (
	"bufio"
	"crypto/tls"
	"crypto/x509"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"net"
	"os"
	"strconv"
	"strings"
	"time"
)

// SmtpError: server trả mã không mong đợi.
type SmtpError struct {
	code int
	text []string
}

func (e *SmtpError) Error() string { return strconv.Itoa(e.code) + " " + strings.Join(e.text, " ") }

// Reader – Mã giả 3.5, Reader { sock, pending }: bufio.Reader giữ phần byte nhận dư.
type Reader struct {
	sock net.Conn
	buf  *bufio.Reader
}

func newReader(sock net.Conn) *Reader { return &Reader{sock: sock, buf: bufio.NewReader(sock)} }

// readLine – READ_LINE(r, max_len).
func (r *Reader) readLine(maxLen int) (string, error) {
	line, err := r.buf.ReadString('\n')
	if len(line) > maxLen {
		return "", errors.New("Dòng quá dài")
	}
	if err == io.EOF {
		return "", errors.New("Server đóng kết nối")
	}
	if err != nil {
		return "", err
	}
	return strings.TrimRight(line, "\r\n"), nil
}

// ProtocolError: phản hồi SMTP sai định dạng hoặc kết nối đóng giữa chừng.
type ProtocolError struct{ msg string }

func (e *ProtocolError) Error() string { return e.msg }

// smtpReply – Mã giả 7.7, SMTP_REPLY: đọc phản hồi nhiều dòng ("250-..." ... "250 ..." hoặc chỉ "250").
func smtpReply(r *Reader) (int, []string, error) {
	var text []string
	for {
		l, err := r.readLine(1024)
		if err != nil || len(l) < 3 { // l = NULL OR LENGTH(l) < 3: RAISE ProtocolError
			if err == nil {
				err = &ProtocolError{"Phản hồi SMTP quá ngắn: '" + l + "'"}
			}
			return 0, nil, &ProtocolError{err.Error()}
		}
		fmt.Printf("[client] S: %s\n", l)
		if len(l) > 4 {
			text = append(text, l[4:]) // l[4 .. END]
		} else {
			text = append(text, "") // rỗng nếu l chỉ có 3 chữ số
		}
		if len(l) == 3 || l[3] == ' ' { // dòng cuối
			code, err := strconv.Atoi(l[:3])
			if err != nil {
				return 0, nil, &ProtocolError{"Mã phản hồi SMTP sai: " + l}
			}
			return code, text, nil
		}
	}
}

// cmd – Mã giả 7.7, dòng 10–14: gửi lệnh, kiểm tra chữ số đầu của mã phản hồi.
// shown: dòng in ra màn hình (che thông tin xác thực); rỗng = in nguyên lệnh.
func cmd(r *Reader, line string, expect int, shown string) error {
	if shown == "" {
		shown = line
	}
	fmt.Printf("[client] C: %s\n", shown)
	if _, err := r.sock.Write([]byte(line + "\r\n")); err != nil { // SEND_ALL
		return err
	}
	code, text, err := smtpReply(r)
	if err != nil {
		return err
	}
	if code/100 != expect { // code DIV 100 ≠ expect
		return &SmtpError{code, text}
	}
	return nil
}

// dotStuff: thêm '.' vào đầu dòng bắt đầu bằng '.', chuẩn hóa xuống dòng về CRLF (Mục 3.4.2).
func dotStuff(msg string) string {
	lines := strings.Split(msg, "\n")
	for i, l := range lines {
		if strings.HasPrefix(l, ".") {
			lines[i] = "." + l
		}
	}
	return strings.Join(lines, "\r\n")
}

// sendMail – Mã giả 7.7, dòng 16–30.
func sendMail(host string, port int, cafile, user, secret, from, to, msg string) error {
	s, err := net.DialTimeout("tcp", net.JoinHostPort(host, strconv.Itoa(port)), 5*time.Second) // SAFE_CONNECT(host, 587, 5000)
	if err != nil {
		return err
	}
	r := newReader(s)
	defer func() { r.sock.Close() }() // CLOSE(r.sock)
	r.sock.SetDeadline(time.Now().Add(30 * time.Second))
	code, _, err := smtpReply(r) // 220: lời chào của server
	if err != nil {
		return err
	}
	if code != 220 {
		return &SmtpError{code, []string{"Không phải lời chào 220"}}
	}
	if err := cmd(r, "EHLO client.example", 2, ""); err != nil {
		return err
	}
	if err := cmd(r, "STARTTLS", 2, ""); err != nil { // 220: sẵn sàng TLS
		return err
	}
	// TLS_WRAP(s, CLIENT, host): kiểm tra chứng chỉ theo nơi tin cậy và tên máy; Reader mới
	cfg := &tls.Config{ServerName: host, MinVersion: tls.VersionTLS12}
	if cafile != "-" {
		pem, err := os.ReadFile(cafile)
		if err != nil {
			return err
		}
		cfg.RootCAs = x509.NewCertPool()
		cfg.RootCAs.AppendCertsFromPEM(pem)
	}
	t := tls.Client(s, cfg)
	if err := t.Handshake(); err != nil {
		return &TlsError{err}
	}
	r = newReader(t)
	fmt.Printf("[client] Đã thiết lập %s, chứng chỉ server hợp lệ cho '%s'\n", tls.VersionName(t.ConnectionState().Version), host)
	if err := cmd(r, "EHLO client.example", 2, ""); err != nil { // chào lại sau khi có TLS
		return err
	}
	token := base64.StdEncoding.EncodeToString([]byte("\x00" + user + "\x00" + secret))
	if err := cmd(r, "AUTH PLAIN "+token, 2, "AUTH PLAIN ****"); err != nil { // không in thông tin xác thực
		return err
	}
	if err := cmd(r, "MAIL FROM:<"+from+">", 2, ""); err != nil {
		return err
	}
	if err := cmd(r, "RCPT TO:<"+to+">", 2, ""); err != nil {
		return err
	}
	if err := cmd(r, "DATA", 3, ""); err != nil { // 354: bắt đầu nội dung
		return err
	}
	body := dotStuff(msg)
	if err := cmd(r, body+"\r\n.", 2, fmt.Sprintf("<nội dung thư %d byte> + dòng '.'", len(body))); err != nil {
		return err
	}
	return cmd(r, "QUIT", 2, "")
}

// TlsError: bắt tay TLS thất bại (chứng chỉ không tin cậy hoặc sai tên máy).
type TlsError struct{ err error }

func (e *TlsError) Error() string { return e.err.Error() }

// buildMessage: thư Internet tối giản – tiêu đề (Subject mã hóa MIME UTF-8), dòng trống, thân UTF-8.
func buildMessage(from, to string) string {
	subject := "=?utf-8?B?" + base64.StdEncoding.EncodeToString([]byte("Thư thử nghiệm Bài 7.6")) + "?="
	now := time.Now().UTC()
	return strings.Join([]string{
		"From: <" + from + ">", "To: <" + to + ">", "Subject: " + subject,
		"Date: " + now.Format("Mon, 02 Jan 2006 15:04:05 +0000"),
		"Message-ID: <" + strconv.FormatInt(now.Unix(), 10) + "@client.example>",
		"MIME-Version: 1.0", "Content-Type: text/plain; charset=utf-8", "Content-Transfer-Encoding: 8bit",
		"", "Xin chào, đây là thư gửi bằng phiên SMTP tự cài đặt.",
		".Dòng này bắt đầu bằng dấu chấm (được DOT_STUFF thêm một dấu chấm).", "Hết thư."}, "\n")
}

func clientMain(args []string) {
	host, port, cafile := "localhost", 587, "cert.pem"
	if len(args) >= 1 {
		host = args[0]
	}
	if len(args) >= 2 {
		port = mustPort(args[1])
	}
	if len(args) >= 3 {
		cafile = args[2]
	}
	user, secret := os.Getenv("SMTP_USER"), os.Getenv("SMTP_PASS")
	from, to := "sinhvien@example.com", "giangvien@example.com"
	err := sendMail(host, port, cafile, user, secret, from, to, buildMessage(from, to))
	var se *SmtpError
	var te *TlsError
	switch {
	case err == nil:
		fmt.Println("[client] Gửi thư thành công")
	case errors.As(err, &se):
		fmt.Printf("[client] Lỗi SMTP: %v\n", err)
		os.Exit(1)
	case errors.As(err, &te): // chứng chỉ không tin cậy hoặc sai tên máy
		fmt.Printf("[client] Lỗi TLS: %v\n", err)
		os.Exit(1)
	default:
		fmt.Printf("[client] Lỗi: %v\n", err)
		os.Exit(1)
	}
}
